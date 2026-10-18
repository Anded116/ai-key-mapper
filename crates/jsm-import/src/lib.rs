//! JoyShockMapper `.txt` preset → aikm TOML profile.
//!
//! The preset is read the way JSM reads it: line by line, later lines win, included files are
//! processed in place. Weapon state files (`ZR,N = 6\ "Game/N.txt"\`) become `next` pairs. What
//! the engine cannot do lands in a `# не перенесено` block at the end of the profile.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use engine::profile::ALL_BTNS;
use engine::{Output, Profile};

#[derive(Default)]
pub struct Options {
    pub game: Option<String>,
    pub exe: Option<String>,
}

pub struct Imported {
    pub toml: String,
    pub profile: Profile,
    /// Preset lines the profile does not carry, with the reason.
    pub skipped: Vec<String>,
}

/// Converts the preset at `path`. Includes and state files are looked up the way JSM does,
/// relative to its folder, so keep the preset inside its `GyroConfigs` tree.
pub fn import(path: &Path, opts: &Options) -> Result<Imported, String> {
    let mut im = Importer::new(path);
    im.process_file(path, 0)?;
    let toml = im.finish(opts);
    let profile = Profile::parse(&toml).map_err(|e| format!("профиль не прошёл проверку движка: {e}\n\n{toml}"))?;
    Ok(Imported { toml, profile, skipped: im.skipped })
}

// ---------- parsing ----------

type Keys = Vec<String>;

#[derive(Clone, Default, PartialEq, Debug)]
struct Bind {
    press: Keys,
    tap: Keys,
    hold: Keys,
    double: Keys,
    next: Keys,
}

#[derive(Clone, Debug)]
struct Src {
    file: String,
    text: String,
    comment: String,
}

impl Src {
    fn show(&self) -> String {
        if self.file.is_empty() {
            self.text.clone()
        } else {
            format!("[{}] {}", self.file, self.text)
        }
    }
}

/// Controller input names in JSM.
enum In {
    Btn(&'static str),
    /// Left stick direction: forward, left, back, right.
    Stick(usize),
    Ring,
    /// Touchpad grid cell T1, T2, ...
    Grid(u32),
    Unsupported(&'static str),
}

fn input(name: &str) -> Option<In> {
    let up = name.trim().to_ascii_uppercase();
    let btn = match up.as_str() {
        "UP" => "dpad_up",
        "DOWN" => "dpad_down",
        "LEFT" => "dpad_left",
        "RIGHT" => "dpad_right",
        "L" => "l1",
        "R" => "r1",
        "ZL" => "l2",
        "ZR" => "r2",
        "L3" => "l3",
        "R3" => "r3",
        "N" => "triangle",
        "E" => "circle",
        "S" => "cross",
        "W" => "square",
        "+" => "options",
        "-" => "create",
        "HOME" | "PS" => "ps",
        "CAPTURE" => "touchpad",
        "MIC" => "mic",
        "TOUCH" => "touch",
        "LUP" => return Some(In::Stick(0)),
        "LLEFT" => return Some(In::Stick(1)),
        "LDOWN" => return Some(In::Stick(2)),
        "LRIGHT" => return Some(In::Stick(3)),
        "LRING" => return Some(In::Ring),
        "RUP" | "RDOWN" | "RLEFT" | "RRIGHT" | "RRING" | "MUP" | "MDOWN" | "MLEFT" | "MRIGHT" | "MRING" => {
            return Some(In::Unsupported("правый стик как кнопки"))
        }
        "TUP" | "TDOWN" | "TLEFT" | "TRIGHT" | "TRING" => return Some(In::Unsupported("свайпы тачпада")),
        "ZLF" | "ZRF" => return Some(In::Unsupported("полное нажатие курка")),
        "SL" | "SR" | "LSL" | "LSR" | "RSL" | "RSR" => return Some(In::Unsupported("кнопки Joy-Con")),
        "LEAN_LEFT" | "LEAN_RIGHT" => return Some(In::Unsupported("наклон геймпада")),
        _ => {
            let n = up.strip_prefix('T').and_then(|d| d.parse::<u32>().ok());
            return n.map(In::Grid);
        }
    };
    Some(In::Btn(btn))
}

enum Target {
    Button { layer: &'static str, btn: &'static str },
    Double(&'static str),
    Stick(usize),
    Ring,
    GridClick(u32),
    GridTouch,
    Setting(String),
    Unsupported(&'static str),
}

fn target(lhs: &str) -> Target {
    let parts: Vec<&str> = lhs.split(',').map(str::trim).collect();
    match parts.as_slice() {
        [one] => {
            if *one != "+" && *one != "-" {
                if one.contains('+') {
                    return Target::Unsupported("одновременное нажатие кнопок");
                }
                if one.contains('*') {
                    return Target::Unsupported("диагональ крестовины");
                }
            }
            match input(one) {
                Some(In::Btn(b)) => Target::Button { layer: "", btn: b },
                Some(In::Stick(i)) => Target::Stick(i),
                Some(In::Ring) => Target::Ring,
                Some(In::Grid(_)) => Target::GridTouch,
                Some(In::Unsupported(why)) => Target::Unsupported(why),
                None => Target::Setting(one.to_ascii_uppercase()),
            }
        }
        [a, b] => match (input(a), input(b)) {
            (Some(In::Btn(x)), Some(In::Btn(y))) if x == y => Target::Double(x),
            (Some(In::Grid(n)), Some(In::Btn("touchpad"))) => Target::GridClick(n),
            (Some(In::Btn(layer)), Some(In::Btn(btn))) => Target::Button { layer, btn },
            _ => Target::Unsupported("аккорд с этими входами"),
        },
        _ => Target::Unsupported("аккорд из трёх кнопок"),
    }
}

enum Line {
    Empty,
    Include(String),
    Command(String),
    Assign(Target, String),
}

/// Splits off the `#` comment (outside quotes); returns the code and the comment text.
fn split_comment(raw: &str) -> (&str, &str) {
    let mut quoted = false;
    for (i, c) in raw.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '#' if !quoted => return (&raw[..i], raw[i + 1..].trim_matches(|c: char| c == '#' || c.is_whitespace())),
            _ => {}
        }
    }
    (raw, "")
}

fn parse_line(code: &str) -> Line {
    let code = code.trim();
    if code.is_empty() {
        return Line::Empty;
    }
    let Some((lhs, rhs)) = code.split_once('=') else {
        let lower = code.to_ascii_lowercase();
        if code.starts_with('"') || lower.ends_with(".txt") || lower.ends_with(".txt\"") {
            return Line::Include(code.trim_matches('"').to_string());
        }
        return Line::Command(code.to_ascii_uppercase());
    };
    Line::Assign(target(lhs.trim()), rhs.trim().to_string())
}

#[derive(Default, Debug)]
struct Tok {
    keys: Vec<String>,
    file: Option<String>,
    prefix: Option<char>,
    suffix: Option<char>,
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Tokens as JSM reads them: `[!^]?(key|"file"|symbol)[\/+'_]?`. `Z+X` (turbo suffix glued to
/// the next key) is read as keys pressed together, which is what presets mean by it.
fn tokenize(s: &str) -> Result<Vec<Tok>, String> {
    let c: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        if c[i].is_whitespace() {
            i += 1;
            continue;
        }
        let mut tok = Tok::default();
        if (c[i] == '!' || c[i] == '^') && c.get(i + 1).is_some_and(|n| !n.is_whitespace()) {
            tok.prefix = Some(c[i]);
            i += 1;
        }
        if c[i] == '"' {
            let close = c[i + 1..].iter().position(|&x| x == '"').ok_or("незакрытая кавычка")? + i + 1;
            tok.file = Some(c[i + 1..close].iter().collect());
            i = close + 1;
        } else {
            loop {
                let mut word = String::new();
                if is_word(c[i]) {
                    while i < c.len() && is_word(c[i]) {
                        word.push(c[i]);
                        i += 1;
                    }
                    if word.len() > 1 && word.ends_with('_') {
                        word.pop();
                        tok.suffix = Some('_');
                    }
                } else {
                    word.push(c[i]);
                    i += 1;
                }
                tok.keys.push(word);
                if tok.suffix.is_none() && c.get(i) == Some(&'+') && c.get(i + 1).is_some_and(|&n| is_word(n)) {
                    i += 1;
                    continue;
                }
                break;
            }
        }
        if tok.suffix.is_none() && i < c.len() && "\\/+'".contains(c[i]) {
            tok.suffix = Some(c[i]);
            i += 1;
        }
        out.push(tok);
    }
    Ok(out)
}

/// JSM key name → aikm key name.
fn key_name(jsm: &str) -> Option<String> {
    let up = jsm.to_ascii_uppercase();
    let name = match up.as_str() {
        "CONTROL" | "LCONTROL" => "LCtrl",
        "RCONTROL" => "RCtrl",
        "SHIFT" | "LSHIFT" => "LShift",
        "ALT" | "LALT" => "LAlt",
        "CAPS_LOCK" => "CapsLock",
        ";" => "Semicolon",
        "'" => "Quote",
        "," => "Comma",
        "." => "Period",
        "/" => "Slash",
        "\\" => "Backslash",
        "[" => "LeftBracket",
        "]" => "RightBracket",
        "`" => "Backquote",
        "-" => "Minus",
        "=" | "+" => "Equals",
        "TOGGLEMOVEMENT" => return None,
        _ => jsm,
    };
    Output::parse(name).map(|o| o.name().to_string())
}

const COMMANDS: &[&str] = &[
    "CALIBRATE",
    "GYRO_OFF",
    "GYRO_ON",
    "GYRO_INVERT",
    "GYRO_INV_X",
    "GYRO_INV_Y",
    "GYRO_TRACKBALL",
    "GYRO_TRACK_X",
    "GYRO_TRACK_Y",
    "SMALL_RUMBLE",
    "BIG_RUMBLE",
    "RECONNECT_CONTROLLERS",
];

enum Value {
    None,
    Bind(Bind, Option<String>),
    Command(String),
    Bad(String),
}

fn value(rhs: &str) -> Value {
    let toks = match tokenize(rhs) {
        Ok(t) => t,
        Err(e) => return Value::Bad(e.to_string()),
    };
    if toks.is_empty() {
        return Value::None;
    }
    if let [t] = toks.as_slice() {
        if t.file.is_none() && t.prefix.is_none() && t.suffix.is_none() && t.keys.len() == 1 {
            let up = t.keys[0].to_ascii_uppercase();
            if up == "NONE" {
                return Value::None;
            }
            if COMMANDS.contains(&up.as_str()) {
                return Value::Command(up);
            }
        }
    }
    let mapped = |t: &Tok| -> Result<Keys, String> {
        t.keys
            .iter()
            .map(|k| {
                key_name(k).ok_or_else(|| match k.to_ascii_uppercase() {
                    up if up.starts_with("X_") || up.starts_with("PS_") => format!("{k}: кнопка виртуального геймпада"),
                    _ => format!("клавиша {k} не поддерживается"),
                })
            })
            .collect()
    };
    let files: Vec<&Tok> = toks.iter().filter(|t| t.file.is_some()).collect();
    if !files.is_empty() {
        let shape = files.len() == 1
            && toks.iter().all(|t| t.prefix.is_none())
            && files[0].suffix == Some('\\')
            && toks.iter().filter(|t| t.file.is_none()).all(|t| matches!(t.suffix, None | Some('\\')));
        if !shape {
            return Value::Bad("загрузка файла с другими событиями".into());
        }
        let mut press = Vec::new();
        for t in toks.iter().filter(|t| t.file.is_none()) {
            match mapped(t) {
                Ok(k) => press.extend(k),
                Err(e) => return Value::Bad(e),
            }
        }
        return Value::Bind(Bind { press, ..Bind::default() }, files[0].file.clone());
    }
    if toks.iter().any(|t| t.prefix.is_some() || t.suffix.is_some()) {
        return Value::Bad("модификаторы ! ^ \\ / + ' _ (события, переключатель, турбо)".into());
    }
    let mut keys = Vec::new();
    for t in &toks {
        match mapped(t) {
            Ok(k) => keys.push(k),
            Err(e) => return Value::Bad(e),
        }
    }
    let mut it = keys.into_iter();
    match (it.next(), it.next(), it.next()) {
        (Some(press), None, None) => Value::Bind(Bind { press, ..Bind::default() }, None),
        (Some(tap), Some(hold), None) => Value::Bind(Bind { tap, hold, ..Bind::default() }, None),
        _ => Value::Bad("больше двух действий на кнопке".into()),
    }
}

// ---------- importer ----------

struct Entry {
    bind: Bind,
    state: Option<String>,
    src: Src,
}

struct Importer {
    main: PathBuf,
    stack: Vec<PathBuf>,
    settings: BTreeMap<String, (String, Src)>,
    /// (layer, button) → binding; layer "" is the base layer.
    binds: BTreeMap<(&'static str, &'static str), Entry>,
    doubles: BTreeMap<&'static str, (Keys, Src)>,
    stick: [Option<(String, Src)>; 4],
    ring: Option<(String, Src)>,
    grid_clicks: BTreeMap<u32, (Keys, Src)>,
    gyro_off_btn: Option<&'static str>,
    calibrate_btn: Option<&'static str>,
    /// WASD side of a hybrid ↔ WASD toggle file: stick keys, ring key, ring mode.
    toggle_keys: Option<[String; 4]>,
    toggle_ring: Option<String>,
    toggle_ring_mode: String,
    info: Vec<String>,
    skipped: Vec<String>,
}

impl Importer {
    fn new(main: &Path) -> Self {
        Self {
            main: main.to_path_buf(),
            stack: Vec::new(),
            settings: BTreeMap::new(),
            binds: BTreeMap::new(),
            doubles: BTreeMap::new(),
            stick: Default::default(),
            ring: None,
            grid_clicks: BTreeMap::new(),
            gyro_off_btn: None,
            calibrate_btn: None,
            toggle_keys: None,
            toggle_ring: None,
            toggle_ring_mode: "OUTER".into(),
            info: Vec::new(),
            skipped: Vec::new(),
        }
    }

    fn skip(&mut self, src: &Src, why: &str) {
        self.skipped.push(format!("{} — {why}", src.show()));
    }

    fn label(&self, path: &Path) -> String {
        if path == self.main {
            return String::new();
        }
        let dir = self.main.parent().unwrap_or(Path::new(""));
        let rel = dir.ancestors().find_map(|a| path.strip_prefix(a).ok()).unwrap_or(path);
        rel.to_string_lossy().replace('\\', "/")
    }

    /// Finds an include the way JSM does (relative to its folder), whatever the folder depth or
    /// letter case: tries the longest tail of the path first, next to the preset and above it.
    fn resolve(&self, path: &str) -> Option<PathBuf> {
        let comps: Vec<&str> = path.split(['/', '\\']).filter(|c| !c.is_empty()).collect();
        let dir = self.main.parent()?;
        for k in 0..comps.len() {
            for base in dir.ancestors().take(6) {
                if let Some(p) = find_ci(base, &comps[k..]) {
                    return Some(p);
                }
            }
        }
        None
    }

    fn read(path: &Path) -> Result<String, String> {
        std::fs::read(path).map(|b| String::from_utf8_lossy(&b).into_owned()).map_err(|e| format!("{}: {e}", path.display()))
    }

    fn process_file(&mut self, path: &Path, depth: usize) -> Result<(), String> {
        let text = Self::read(path)?;
        let file = self.label(path);
        self.stack.push(path.to_path_buf());
        for raw in text.lines() {
            let (code, comment) = split_comment(raw);
            let src = Src { file: file.clone(), text: code.trim().to_string(), comment: comment.to_string() };
            match parse_line(code) {
                Line::Empty => {}
                Line::Command(c) if c == "RESET_MAPPINGS" => {}
                Line::Command(_) => self.skip(&src, "команда JSM"),
                Line::Include(p) => match self.resolve(&p) {
                    None => self.skip(&src, "файл не найден"),
                    Some(_) if depth >= 8 => self.skip(&src, "слишком глубокая вложенность"),
                    Some(f) if self.stack.contains(&f) => self.skip(&src, "файл подключает сам себя"),
                    Some(f) => self.process_file(&f, depth + 1)?,
                },
                Line::Assign(t, rhs) => self.assign(t, &rhs, src),
            }
        }
        self.stack.pop();
        Ok(())
    }

    fn assign(&mut self, t: Target, rhs: &str, src: Src) {
        match t {
            Target::Setting(name) => {
                self.settings.insert(name, (rhs.to_string(), src));
            }
            Target::Unsupported(why) => self.skip(&src, why),
            Target::Button { layer, btn } => match value(rhs) {
                Value::None => {
                    self.binds.remove(&(layer, btn));
                }
                Value::Bind(bind, state) => {
                    self.binds.insert((layer, btn), Entry { bind, state, src });
                }
                Value::Command(c) if c == "CALIBRATE" && layer.is_empty() => self.calibrate_btn = Some(btn),
                Value::Command(c) if c == "GYRO_OFF" && layer.is_empty() => self.gyro_off_btn = Some(btn),
                Value::Command(_) => self.skip(&src, "команда JSM на кнопке"),
                Value::Bad(why) => self.skip(&src, &why),
            },
            Target::Double(btn) => match value(rhs) {
                Value::None => {
                    self.doubles.remove(btn);
                }
                Value::Bind(b, None) if b.tap.is_empty() => {
                    self.doubles.insert(btn, (b.press, src));
                }
                Value::Bad(why) => self.skip(&src, &why),
                _ => self.skip(&src, "двойное нажатие с тапом/удержанием или файлом"),
            },
            Target::Stick(_) | Target::Ring => {
                let key = match value(rhs) {
                    Value::None => None,
                    Value::Bind(b, None) if b.press.len() == 1 => Some((b.press[0].clone(), src.clone())),
                    Value::Bad(why) => return self.skip(&src, &why),
                    _ => return self.skip(&src, "на стике переносится только одна клавиша"),
                };
                match t {
                    Target::Stick(i) => self.stick[i] = key,
                    _ => self.ring = key,
                }
            }
            Target::GridClick(n) => match value(rhs) {
                Value::None => {
                    self.grid_clicks.remove(&n);
                }
                Value::Bind(b, None) if b.tap.is_empty() => {
                    self.grid_clicks.insert(n, (b.press, src));
                }
                Value::Bad(why) => self.skip(&src, &why),
                _ => self.skip(&src, "клик тачпада с тапом/удержанием или файлом"),
            },
            Target::GridTouch => {
                if !matches!(value(rhs), Value::None) {
                    self.skip(&src, "касание ячейки тачпада без клика");
                }
            }
        }
    }

    fn take(&mut self, name: &str) -> Option<(String, Src)> {
        self.settings.remove(name)
    }

    /// First number of a setting; JSM allows "X Y" for separate axes.
    fn number(&mut self, name: &str) -> Option<f64> {
        let (v, src) = self.take(name)?;
        let nums: Vec<f64> = v.split_whitespace().filter_map(|x| x.parse().ok()).collect();
        match nums.as_slice() {
            [] => {
                self.skip(&src, "не число");
                None
            }
            [a, rest @ ..] => {
                if rest.iter().any(|b| b != a) {
                    self.skip(&src, &format!("раздельные значения по осям, взято {a}"));
                }
                Some(*a)
            }
        }
    }

    fn word(&mut self, name: &str) -> Option<(String, Src)> {
        self.take(name).map(|(v, s)| (v.trim().to_ascii_uppercase(), s))
    }

    /// Weapon state files: `ZR,N = 6\ "Game/N.txt"\` then, in N.txt, `ZR,N = 7` → press 6, next 7.
    fn state_pairs(&mut self) {
        let mut used = BTreeSet::new();
        let keys: Vec<_> = self.binds.iter().filter(|(_, e)| e.state.is_some()).map(|(k, _)| *k).collect();
        for key in keys {
            let (state, src) = {
                let e = &self.binds[&key];
                (e.state.clone().unwrap(), e.src.clone())
            };
            let text = self.resolve(&state).and_then(|p| Self::read(&p).ok().map(|t| (p, t)));
            let Some((path, text)) = text else {
                if self.binds[&key].bind.press.is_empty() {
                    self.binds.remove(&key);
                }
                self.skip(&src, "файл состояния не найден");
                continue;
            };
            let label = self.label(&path);
            let lines: Vec<(Line, Src)> = text
                .lines()
                .map(|raw| {
                    let (code, comment) = split_comment(raw);
                    (parse_line(code), Src { file: label.clone(), text: code.trim().to_string(), comment: comment.to_string() })
                })
                .collect();
            if self.binds[&key].bind.press.is_empty() {
                self.movement_toggle(key, &src, lines);
                continue;
            }
            used.insert(label.clone());
            for (line, line_src) in lines {
                match line {
                    Line::Empty => {}
                    Line::Assign(Target::Button { layer, btn }, rhs) => {
                        let Value::Bind(b, _) = value(&rhs) else { continue };
                        if (layer, btn) == key {
                            let e = self.binds.get_mut(&key).unwrap();
                            if b.press != e.bind.press && !b.press.is_empty() {
                                e.bind.next = b.press;
                            }
                        } else if self.binds.get(&(layer, btn)).is_some_and(|o| o.bind.press != b.press) {
                            self.skip(&line_src, "в файле состояния кнопка меняется, перенесено первое значение");
                        }
                    }
                    _ => self.skip(&line_src, "строка файла состояния"),
                }
            }
        }
        // The same line can come from several state files.
        let mut seen = BTreeSet::new();
        self.skipped.retain(|s| seen.insert(s.clone()));
        if !used.is_empty() {
            let list: Vec<_> = used.into_iter().collect();
            self.info.push(format!("Пары оружия (поле next) собраны из файлов состояния: {}.", list.join(", ")));
        }
    }

    /// A button that only loads a file setting the left stick directions switches hybrid ↔ WASD:
    /// that is the engine's `ToggleMovement`. The file's keys and ring become the WASD side.
    fn movement_toggle(&mut self, key: (&'static str, &'static str), src: &Src, lines: Vec<(Line, Src)>) {
        let moves = lines.iter().any(|(l, _)| matches!(l, Line::Assign(Target::Stick(_), _)));
        if !moves {
            self.binds.remove(&key);
            return self.skip(src, "кнопка только меняет файл состояния");
        }
        let mut stick: [Option<String>; 4] = Default::default();
        for (line, line_src) in lines {
            let single = |rhs: &str| match value(rhs) {
                Value::Bind(b, None) if b.press.len() == 1 && b.tap.is_empty() => Some(b.press[0].clone()),
                _ => None,
            };
            match line {
                Line::Empty => {}
                Line::Assign(Target::Stick(i), rhs) => stick[i] = single(&rhs),
                Line::Assign(Target::Ring, rhs) => self.toggle_ring = single(&rhs),
                Line::Assign(Target::Setting(s), rhs) if s == "LEFT_RING_MODE" => self.toggle_ring_mode = rhs.trim().to_ascii_uppercase(),
                Line::Assign(Target::Button { layer, btn }, _) if (layer, btn) == key => {}
                // Weapon lines repeat the state the toggle came from.
                Line::Assign(Target::Button { layer, btn }, rhs)
                    if matches!(value(&rhs), Value::Bind(_, Some(_))) && self.binds.contains_key(&(layer, btn)) => {}
                _ => self.skip(&line_src, "строка файла переключения движения"),
            }
        }
        if stick.iter().all(Option::is_some) {
            self.toggle_keys = Some(stick.map(Option::unwrap));
        }
        let e = self.binds.get_mut(&key).unwrap();
        e.bind.press = vec!["ToggleMovement".into()];
        e.state = None;
    }

    fn finish(&mut self, opts: &Options) -> String {
        self.state_pairs();

        for (btn, (keys, src)) in std::mem::take(&mut self.doubles) {
            match self.binds.get_mut(&("", btn)) {
                Some(e) if !e.bind.press.is_empty() || !e.bind.tap.is_empty() => e.bind.double = keys,
                _ => self.skip(&src, "двойное нажатие без одиночного"),
            }
        }

        // Touchpad: a 2×1 grid is exactly the engine's left / right half click.
        let mode = self.word("TOUCHPAD_MODE");
        let grid = self.take("GRID_SIZE");
        let halves = matches!(&mode, Some((m, _)) if m == "GRID_AND_STICK")
            && grid.as_ref().is_some_and(|(g, _)| g.split_whitespace().collect::<Vec<_>>() == ["2", "1"]);
        if !halves {
            for (_, src) in mode.iter().chain(grid.iter()) {
                self.skip(src, "режим тачпада: в движке только клик по левой / правой половине");
            }
        }
        for (n, (keys, src)) in std::mem::take(&mut self.grid_clicks) {
            match (halves, n) {
                (true, 1) => self.put_pad("pad_left", keys, src),
                (true, 2) => self.put_pad("pad_right", keys, src),
                _ => self.skip(&src, "клик по ячейке тачпада вне сетки 2×1"),
            }
        }

        let mut out = String::new();
        let game = opts.game.clone().unwrap_or_else(|| {
            self.main.file_stem().map_or("game".into(), |s| s.to_string_lossy().into_owned())
        });
        let file = self.main.file_name().map_or(String::new(), |s| s.to_string_lossy().into_owned());
        out += &format!("# Импорт пресета JoyShockMapper «{file}» (aikm-import).\n");

        // [calibration]
        let real_world = self.number("REAL_WORLD_CALIBRATION");
        let in_game = self.number("IN_GAME_SENS");
        // [gyro]
        let gyro_sens = self.number("GYRO_SENS");
        let min_sens = self.number("MIN_GYRO_SENS").or(gyro_sens);
        let max_sens = self.number("MAX_GYRO_SENS").or(gyro_sens);
        let min_thr = self.number("MIN_GYRO_THRESHOLD");
        let max_thr = self.number("MAX_GYRO_THRESHOLD");
        let mut gyro_off = self.gyro_off_btn.map(str::to_string);
        if let Some((v, src)) = self.word("GYRO_OFF") {
            match input(&v) {
                Some(In::Btn(b)) => gyro_off = Some(b.to_string()),
                _ if v == "NONE" => gyro_off = None,
                _ => self.skip(&src, "выключение гироскопа этим входом"),
            }
        }
        if let Some((_, src)) = self.take("GYRO_ON") {
            self.skip(&src, "гироскоп только при удержании: в движке есть лишь выключение (gyro.off)");
        }
        let invert_x = self.word("GYRO_AXIS_X").is_some_and(|(v, _)| v == "INVERTED");
        let invert_y = self.word("GYRO_AXIS_Y").is_some_and(|(v, _)| v == "INVERTED");
        let calibrate = match self.calibrate_btn {
            Some(b) => Some(b.to_string()),
            // PS calibrates by default; a preset that binds PS to a key did not want that.
            None if self.binds.contains_key(&("", "ps")) => Some("none".into()),
            None => None,
        };
        // [flick]
        let flick = match self.word("RIGHT_STICK_MODE") {
            Some((m, _)) if m == "FLICK" || m == "FLICK_ONLY" => true,
            Some((_, src)) => {
                self.skip(&src, "правый стик не flick: прицел стиком движок не умеет, стик выключен");
                false
            }
            None => {
                self.info.push("RIGHT_STICK_MODE не задан (в JSM это AIM): flick выключен, правый стик молчит.".into());
                false
            }
        };
        let flick_time = self.number("FLICK_TIME");
        let flick_fwd = self.number("FLICK_DEADZONE_ANGLE");
        let dz_in = self.number("STICK_DEADZONE_INNER");
        let dz_out = self.number("STICK_DEADZONE_OUTER");
        let r_in = self.number("RIGHT_STICK_DEADZONE_INNER").or(dz_in);
        let r_out = self.number("RIGHT_STICK_DEADZONE_OUTER").or(dz_out);
        let l_in = self.number("LEFT_STICK_DEADZONE_INNER").or(dz_in);
        let l_out = self.number("LEFT_STICK_DEADZONE_OUTER").or(dz_out);
        // [movement]
        let wasd = self.stick.iter().all(Option::is_some);
        if !wasd {
            for (_, src) in self.stick.clone().iter().flatten() {
                self.skip(src, "левый стик частично на клавишах: движок отдаёт стик игре целиком или переводит в WASD");
            }
        }
        let ring_mode = self.word("LEFT_RING_MODE").map_or("OUTER".to_string(), |(v, _)| v);
        let (mut walk, mut sprint) = (None, None);
        if let Some((key, src)) = self.ring.clone() {
            match (wasd, ring_mode.as_str()) {
                (true, "INNER") => walk = Some(key),
                (true, _) => sprint = Some(key),
                (false, _) => self.skip(&src, "кольцо стика без WASD"),
            }
        }
        let mut move_keys: Option<Vec<String>> = wasd.then(|| self.stick.iter().flatten().map(|(k, _)| k.clone()).collect());
        if let (false, Some(keys)) = (wasd, &self.toggle_keys) {
            move_keys = Some(keys.to_vec());
            match (self.toggle_ring.clone(), self.toggle_ring_mode.as_str()) {
                (Some(k), "INNER") => walk = Some(k),
                (Some(k), _) => sprint = Some(k),
                _ => {}
            }
        }
        // [timing], [triggers]
        let hold_ms = self.number("HOLD_PRESS_TIME");
        let double_ms = self.number("DBL_PRESS_WINDOW");
        let trigger = self.number("TRIGGER_THRESHOLD");

        let leftovers = std::mem::take(&mut self.settings);
        for (_, (_, src)) in leftovers {
            self.skip(&src, "настройка JSM, которой нет в движке");
        }
        if real_world.is_none() {
            self.info.push("REAL_WORLD_CALIBRATION не задан: калибровка по умолчанию движка, flick будет промахиваться.".into());
        }
        if min_sens.is_none() && max_sens.is_none() {
            self.info.push("Чувствительность гироскопа в пресете не задана: взята по умолчанию движка.".into());
        }

        for line in &self.info {
            out += &format!("# {line}\n");
        }
        out += &format!("game = {}\n", q(&game));
        match &opts.exe {
            Some(exe) => out += &format!("exe = {}\n", q(exe)),
            None => out += "# exe = \"game.exe\"           # впиши exe игры, тогда профиль подхватится сам\n",
        }

        let mut sect = |title: &str, rows: Vec<(String, String)>| {
            if !rows.is_empty() {
                out += &format!("\n[{title}]\n");
                for (k, v) in rows {
                    out += &format!("{k} = {v}\n");
                }
            }
        };
        let mut rows = Vec::new();
        opt_num(&mut rows, "real_world", real_world);
        opt_num(&mut rows, "in_game_sens", in_game);
        sect("calibration", rows);

        let mut rows = Vec::new();
        opt_num(&mut rows, "min_sens", min_sens);
        opt_num(&mut rows, "max_sens", max_sens);
        opt_num(&mut rows, "min_threshold", min_thr);
        opt_num(&mut rows, "max_threshold", max_thr);
        rows.push(("off".into(), q(gyro_off.as_deref().unwrap_or("none"))));
        if let Some(c) = &calibrate {
            rows.push(("calibrate".into(), q(c)));
        }
        if invert_x {
            rows.push(("invert_x".into(), "true".into()));
        }
        if invert_y {
            rows.push(("invert_y".into(), "true".into()));
        }
        sect("gyro", rows);

        let mut rows = vec![("enabled".to_string(), flick.to_string())];
        opt_num(&mut rows, "time", flick_time);
        opt_num(&mut rows, "forward_deadzone", flick_fwd);
        opt_num(&mut rows, "deadzone_inner", r_in);
        opt_num(&mut rows, "deadzone_outer", r_out);
        sect("flick", rows);

        let mut rows = vec![("mode".to_string(), q(if wasd { "wasd" } else { "hybrid" }))];
        if let Some(keys) = &move_keys {
            rows.push(("keys".into(), list(keys)));
            if let Some(w) = &walk {
                rows.push(("walk".into(), q(w)));
            }
            if let Some(s) = &sprint {
                rows.push(("sprint".into(), q(s)));
            }
        }
        opt_num(&mut rows, "deadzone_inner", l_in);
        opt_num(&mut rows, "deadzone_outer", l_out);
        sect("movement", rows);

        let mut rows = Vec::new();
        opt_num(&mut rows, "hold_ms", hold_ms);
        opt_num(&mut rows, "double_ms", double_ms);
        sect("timing", rows);
        let mut rows = Vec::new();
        opt_num(&mut rows, "threshold", trigger);
        sect("triggers", rows);

        let weapons = self.weapons();
        out += &self.bindings_toml("", "buttons");
        if let Some((layer, keys, slots)) = weapons {
            out += "\n[weapons]\n";
            if layer != "r2" {
                out += &format!("layer = {}\n", q(layer));
            }
            out += &format!("slots = {slots}\nkeys = {}\n", list(&keys));
        }
        let mut layers: Vec<&'static str> = self.binds.keys().map(|(l, _)| *l).filter(|l| !l.is_empty()).collect();
        layers.dedup();
        layers.sort_by_key(|l| btn_order(l));
        for layer in layers {
            out += &self.bindings_toml(layer, &format!("layer.{layer}"));
        }

        if !self.skipped.is_empty() {
            out += "\n# Не перенесено:\n";
            for s in &self.skipped {
                out += &format!("#   {s}\n");
            }
        }
        out
    }

    fn put_pad(&mut self, btn: &'static str, press: Keys, src: Src) {
        self.binds.insert(("", btn), Entry { bind: Bind { press, ..Bind::default() }, state: None, src });
    }

    /// Four face buttons of one layer giving digit keys → `[weapons]` (4 slots, or 8 with pairs).
    fn weapons(&mut self) -> Option<(&'static str, Keys, u8)> {
        const FACE: [&str; 4] = ["square", "cross", "circle", "triangle"];
        let digit = |k: &Keys| k.len() == 1 && k[0].len() == 1 && k[0].chars().all(|c| c.is_ascii_digit());
        let mut layers: Vec<&'static str> = self.binds.keys().map(|(l, _)| *l).filter(|l| !l.is_empty()).collect();
        layers.sort_by_key(|l| (*l != "r2", btn_order(l)));
        layers.dedup();
        for layer in layers {
            let entries: Vec<&Bind> = FACE.iter().filter_map(|b| self.binds.get(&(layer, *b)).map(|e| &e.bind)).collect();
            if entries.len() != 4 || !entries.iter().all(|b| digit(&b.press) && b.tap.is_empty() && b.hold.is_empty() && b.double.is_empty()) {
                continue;
            }
            let pairs = entries.iter().all(|b| digit(&b.next));
            if !pairs && !entries.iter().all(|b| b.next.is_empty()) {
                continue;
            }
            let mut keys = Vec::new();
            for b in &entries {
                keys.push(b.press[0].clone());
                if pairs {
                    keys.push(b.next[0].clone());
                }
            }
            for b in FACE {
                self.binds.remove(&(layer, b));
            }
            return Some((layer, keys, if pairs { 8 } else { 4 }));
        }
        None
    }

    fn bindings_toml(&self, layer: &str, title: &str) -> String {
        let mut rows: Vec<(&str, &Entry)> = self.binds.iter().filter(|((l, _), _)| *l == layer).map(|((_, b), e)| (*b, e)).collect();
        if rows.is_empty() {
            return String::new();
        }
        rows.sort_by_key(|(b, _)| btn_order(b));
        let lines: Vec<String> = rows.iter().map(|(b, e)| format!("{b} = {}", bind_toml(&e.bind))).collect();
        let width = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0).min(40);
        let mut out = format!("\n[{title}]\n");
        for (line, (_, e)) in lines.iter().zip(&rows) {
            if e.src.comment.is_empty() {
                out += &format!("{line}\n");
            } else {
                out += &format!("{line:<width$}  # {}\n", e.src.comment);
            }
        }
        out
    }
}

fn find_ci(base: &Path, comps: &[&str]) -> Option<PathBuf> {
    let mut p = base.to_path_buf();
    for c in comps {
        let exact = p.join(c);
        p = if exact.exists() {
            exact
        } else {
            std::fs::read_dir(&p).ok()?.filter_map(|e| e.ok()).find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(c))?.path()
        };
    }
    p.is_file().then_some(p)
}

fn btn_order(name: &str) -> usize {
    ALL_BTNS.iter().position(|(_, n)| *n == name).unwrap_or(usize::MAX)
}

fn q(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn list(keys: &[String]) -> String {
    format!("[{}]", keys.iter().map(|k| q(k)).collect::<Vec<_>>().join(", "))
}

fn keys_toml(k: &Keys) -> String {
    if k.len() == 1 {
        q(&k[0])
    } else {
        list(k)
    }
}

fn bind_toml(b: &Bind) -> String {
    if b.tap.is_empty() && b.hold.is_empty() && b.double.is_empty() && b.next.is_empty() {
        return keys_toml(&b.press);
    }
    let parts: Vec<String> = [("press", &b.press), ("tap", &b.tap), ("hold", &b.hold), ("double", &b.double), ("next", &b.next)]
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(n, v)| format!("{n} = {}", keys_toml(v)))
        .collect();
    format!("{{ {} }}", parts.join(", "))
}

fn opt_num(rows: &mut Vec<(String, String)>, key: &str, v: Option<f64>) {
    if let Some(v) = v {
        rows.push((key.to_string(), format!("{v:?}")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens() {
        let t = tokenize("6\\ \"GyroConfigs/Dusk/N.txt\"\\").unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!((t[0].keys[0].as_str(), t[0].suffix), ("6", Some('\\')));
        assert_eq!(t[1].file.as_deref(), Some("GyroConfigs/Dusk/N.txt"));
        let t = tokenize("Z+X").unwrap();
        assert_eq!(t[0].keys, ["Z", "X"]);
        let t = tokenize("SPACE+").unwrap();
        assert_eq!(t[0].suffix, Some('+'));
        let t = tokenize("!LCONTROL\\ !LCONTROL/").unwrap();
        assert_eq!((t[0].prefix, t[1].suffix), (Some('!'), Some('/')));
        assert_eq!(tokenize("\\").unwrap()[0].keys, ["\\"]);
    }

    #[test]
    fn values() {
        assert!(matches!(value("ESC M"), Value::Bind(Bind { ref tap, ref hold, .. }, None) if tap == &["Esc"] && hold == &["M"]));
        assert!(matches!(value("NONE"), Value::None));
        assert!(matches!(value(""), Value::None));
        assert!(matches!(value("CALIBRATE"), Value::Command(_)));
        assert!(matches!(value("SPACE+"), Value::Bad(_)));
        assert!(matches!(value("N5"), Value::Bad(_)));
        assert!(matches!(value("]"), Value::Bind(Bind { ref press, .. }, None) if press == &["RightBracket"]));
        assert!(matches!(value("CONTROL"), Value::Bind(Bind { ref press, .. }, None) if press == &["LCtrl"]));
    }

    #[test]
    fn targets() {
        assert!(matches!(target("ZR,N"), Target::Button { layer: "r2", btn: "triangle" }));
        assert!(matches!(target("ZR,+"), Target::Button { layer: "r2", btn: "options" }));
        assert!(matches!(target("W,W"), Target::Double("square")));
        assert!(matches!(target("T2,CAPTURE"), Target::GridClick(2)));
        assert!(matches!(target("L3+R3"), Target::Unsupported(_)));
        assert!(matches!(target("UP*RIGHT"), Target::Unsupported(_)));
        assert!(matches!(target("LIGHT_BAR"), Target::Setting(_)));
    }
}
