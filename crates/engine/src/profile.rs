//! Game profile in TOML, with `extends` inheritance.
//!
//! Settings sections merge key by key; in `buttons` and `layer.*` a child binding replaces the
//! base binding as a whole (mixing `tap` from one file with `press` from another makes no sense).

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use toml::{Table, Value};

use crate::keys::Output;

/// Controller inputs a binding can sit on. `Touch` is a finger on the touchpad,
/// `PadLeft` / `PadRight` are touchpad clicks on that half.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Btn {
    L1,
    R1,
    L2,
    R2,
    L3,
    R3,
    Cross,
    Circle,
    Square,
    Triangle,
    Up,
    Down,
    Left,
    Right,
    Create,
    Options,
    Ps,
    Touchpad,
    Mic,
    Touch,
    PadLeft,
    PadRight,
}

pub const ALL_BTNS: [(Btn, &str); 22] = [
    (Btn::L1, "l1"),
    (Btn::R1, "r1"),
    (Btn::L2, "l2"),
    (Btn::R2, "r2"),
    (Btn::L3, "l3"),
    (Btn::R3, "r3"),
    (Btn::Cross, "cross"),
    (Btn::Circle, "circle"),
    (Btn::Square, "square"),
    (Btn::Triangle, "triangle"),
    (Btn::Up, "dpad_up"),
    (Btn::Down, "dpad_down"),
    (Btn::Left, "dpad_left"),
    (Btn::Right, "dpad_right"),
    (Btn::Create, "create"),
    (Btn::Options, "options"),
    (Btn::Ps, "ps"),
    (Btn::Touchpad, "touchpad"),
    (Btn::Mic, "mic"),
    (Btn::Touch, "touch"),
    (Btn::PadLeft, "pad_left"),
    (Btn::PadRight, "pad_right"),
];

impl Btn {
    pub fn parse(name: &str) -> Option<Self> {
        ALL_BTNS.iter().find(|(_, n)| n.eq_ignore_ascii_case(name.trim())).map(|(b, _)| *b)
    }

    pub fn name(self) -> &'static str {
        ALL_BTNS.iter().find(|(b, _)| *b == self).map_or("?", |(_, n)| n)
    }
}

/// What a button does. `press` follows the button; `tap`/`hold` split by HOLD time;
/// `double` fires on a second press within the double-press window (the tap waits for the
/// window to pass, so it never fires together with the double).
///
/// `next` is the second weapon of a pair, the way JoyShockMapper presets did it with state
/// files: pressing the same button again gives `next`, however much later; pressing another
/// paired button starts over with its `press`.
///
/// `latch` is console-style sprint: one click holds the keys while the left stick is tilted; they
/// let go when the stick comes back to the centre or on the next click.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Binding {
    pub press: Vec<Output>,
    pub tap: Vec<Output>,
    pub hold: Vec<Output>,
    pub double: Vec<Output>,
    pub next: Vec<Output>,
    pub latch: Vec<Output>,
    /// Hold time of this binding, instead of `timing.hold_ms` (menu buttons want a longer one).
    pub hold_ms: Option<u32>,
}

impl Binding {
    pub fn is_empty(&self) -> bool {
        self.press.is_empty()
            && self.tap.is_empty()
            && self.hold.is_empty()
            && self.double.is_empty()
            && self.next.is_empty()
            && self.latch.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Calibration {
    /// Mouse counts per degree at in-game sensitivity 1 (JSM REAL_WORLD_CALIBRATION).
    pub real_world: f32,
    pub in_game_sens: f32,
}

impl Calibration {
    pub fn counts_per_degree(&self) -> f32 {
        self.real_world / self.in_game_sens
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GyroCfg {
    pub enabled: bool,
    pub min_sens: f32,
    pub max_sens: f32,
    /// Real-life turn speed in deg/s where `min_sens` / `max_sens` apply.
    pub min_threshold: f32,
    pub max_threshold: f32,
    /// Gyro is off while this input is held.
    pub off: Option<Btn>,
    /// Holding this input recalibrates; the new bias applies on release.
    pub calibrate: Option<Btn>,
    /// Seconds of calibration right after start; keep the pad still.
    pub startup_calibration: f32,
    pub invert_x: bool,
    pub invert_y: bool,
}

/// Stick deadzones as in JSM: inside `inner` is zero, the last `outer` of travel is full tilt,
/// lengths in between are rescaled to 0..1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Deadzone {
    pub inner: f32,
    pub outer: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FlickCfg {
    pub enabled: bool,
    /// Seconds to complete a flick.
    pub time: f32,
    /// Rescaled deflection that starts a flick (JSM: 1.0, i.e. full tilt past the outer deadzone);
    /// the flick ends below 90% of it.
    pub threshold: f32,
    /// Flicks within ± this many degrees of forward turn into plain rotation.
    pub forward_deadzone: f32,
    pub deadzone: Deadzone,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MoveMode {
    /// The game reads the left stick itself.
    #[default]
    Hybrid,
    Wasd,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MovementCfg {
    pub mode: MoveMode,
    pub deadzone: Deadzone,
    /// Forward, left, back, right.
    pub keys: [Output; 4],
    /// Held while the stick is tilted below `ring` (walk).
    pub walk: Option<Output>,
    /// Held while the stick is tilted past `ring` (sprint).
    pub sprint: Option<Output>,
    /// Rescaled deflection between the inner and the outer ring (JSM: 0.7).
    pub ring: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Timing {
    pub hold_ms: f32,
    pub double_ms: f32,
    /// How long a tap holds its key down.
    pub tap_ms: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub button: Btn,
    pub bindings: HashMap<Btn, Binding>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LightbarCfg {
    pub enabled: bool,
    pub idle: [u8; 3],
    /// Colour while the next press of this paired button gives its second weapon.
    pub pairs: Vec<(Btn, [u8; 3])>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub game: String,
    pub exe: Option<String>,
    pub calibration: Calibration,
    pub gyro: GyroCfg,
    pub flick: FlickCfg,
    pub movement: MovementCfg,
    pub timing: Timing,
    /// L2/R2 count as pressed above this analog position (JSM TRIGGER_THRESHOLD: 0 = any pull).
    pub trigger_threshold: f32,
    /// Light bar shows which paired button gives its second weapon next.
    pub lightbar: LightbarCfg,
    /// Hide the controller from games with HidHide while running (keyboard/mouse-only games).
    pub hide_controller: bool,
    pub buttons: HashMap<Btn, Binding>,
    pub layers: Vec<Layer>,
}

#[derive(Debug)]
pub struct ProfileError(pub String);

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ProfileError {}

fn err<T>(msg: impl Into<String>) -> Result<T, ProfileError> {
    Err(ProfileError(msg.into()))
}

// ---------- raw TOML shape ----------

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Raw {
    #[serde(default)]
    extends: Option<String>,
    #[serde(default)]
    game: Option<String>,
    #[serde(default)]
    exe: Option<String>,
    #[serde(default)]
    calibration: RawCalibration,
    #[serde(default)]
    gyro: RawGyro,
    #[serde(default)]
    flick: RawFlick,
    #[serde(default)]
    movement: RawMovement,
    #[serde(default)]
    timing: RawTiming,
    #[serde(default)]
    triggers: RawTriggers,
    #[serde(default)]
    controller: RawController,
    #[serde(default)]
    lightbar: RawLightbar,
    #[serde(default)]
    weapons: Option<RawWeapons>,
    #[serde(default)]
    buttons: BTreeMap<String, RawBinding>,
    #[serde(default)]
    layer: BTreeMap<String, BTreeMap<String, RawBinding>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, default)]
struct RawCalibration {
    real_world: f32,
    in_game_sens: f32,
}

impl Default for RawCalibration {
    fn default() -> Self {
        Self { real_world: 40.0, in_game_sens: 1.0 }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, default)]
struct RawGyro {
    enabled: bool,
    min_sens: f32,
    max_sens: f32,
    min_threshold: f32,
    max_threshold: f32,
    off: String,
    calibrate: String,
    startup_calibration: f32,
    invert_x: bool,
    invert_y: bool,
}

impl Default for RawGyro {
    fn default() -> Self {
        Self {
            enabled: true,
            min_sens: 2.3,
            max_sens: 3.5,
            min_threshold: 0.0,
            max_threshold: 75.0,
            off: "touch".into(),
            calibrate: "ps".into(),
            startup_calibration: 1.0,
            invert_x: false,
            invert_y: false,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, default)]
struct RawFlick {
    enabled: bool,
    time: f32,
    threshold: f32,
    forward_deadzone: f32,
    deadzone_inner: f32,
    deadzone_outer: f32,
}

impl Default for RawFlick {
    fn default() -> Self {
        Self { enabled: true, time: 0.1, threshold: 1.0, forward_deadzone: 0.0, deadzone_inner: 0.15, deadzone_outer: 0.1 }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, default)]
struct RawMovement {
    mode: String,
    deadzone_inner: f32,
    deadzone_outer: f32,
    keys: Vec<String>,
    walk: String,
    sprint: String,
    ring: f32,
}

impl Default for RawMovement {
    fn default() -> Self {
        Self {
            mode: "hybrid".into(),
            deadzone_inner: 0.15,
            deadzone_outer: 0.1,
            keys: vec!["W".into(), "A".into(), "S".into(), "D".into()],
            walk: String::new(),
            sprint: String::new(),
            ring: 0.7,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, default)]
struct RawTiming {
    hold_ms: f32,
    double_ms: f32,
    tap_ms: f32,
}

impl Default for RawTiming {
    fn default() -> Self {
        Self { hold_ms: 150.0, double_ms: 150.0, tap_ms: 40.0 }
    }
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, default)]
struct RawTriggers {
    threshold: f32,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, default)]
struct RawController {
    hide: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, default)]
struct RawLightbar {
    enabled: bool,
    idle: String,
    square: String,
    cross: String,
    circle: String,
    triangle: String,
}

impl Default for RawLightbar {
    fn default() -> Self {
        // Off by default; when on, PlayStation button colours as in the old JSM preset.
        Self {
            enabled: false,
            idle: "303030".into(),
            square: "FF3CA0".into(),
            cross: "3C64FF".into(),
            circle: "FF2828".into(),
            triangle: "28FF8C".into(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWeapons {
    #[serde(default = "default_weapon_layer")]
    layer: String,
    slots: u8,
    keys: Vec<String>,
}

fn default_weapon_layer() -> String {
    "r2".into()
}

#[derive(Deserialize, Clone)]
#[serde(untagged)]
enum Keys {
    One(String),
    Many(Vec<String>),
}

#[derive(Deserialize, Clone)]
#[serde(untagged)]
enum RawBinding {
    Keys(Keys),
    Table(RawBindingTable),
}

#[derive(Deserialize, Clone, Default)]
#[serde(deny_unknown_fields)]
struct RawBindingTable {
    press: Option<Keys>,
    tap: Option<Keys>,
    hold: Option<Keys>,
    double: Option<Keys>,
    next: Option<Keys>,
    latch: Option<Keys>,
    hold_ms: Option<u32>,
}

// ---------- loading ----------

impl Profile {
    pub fn load(path: &Path) -> Result<Self, ProfileError> {
        let table = load_merged(path, 0)?;
        let raw: Raw = Value::Table(table)
            .try_into()
            .map_err(|e: toml::de::Error| ProfileError(format!("{}: {e}", path.display())))?;
        let fallback = path.file_stem().map_or("profile".to_string(), |s| s.to_string_lossy().into_owned());
        compile(raw, &fallback)
    }

    pub fn parse(text: &str) -> Result<Self, ProfileError> {
        let raw: Raw = toml::from_str(text).map_err(|e| ProfileError(e.to_string()))?;
        if raw.extends.is_some() {
            return err("`extends` needs a file path: use Profile::load");
        }
        compile(raw, "profile")
    }
}

/// What each binding does in the game, from its line comment: `"buttons.l1"` → `"прыжок"`,
/// `"layer.r2.create"` → `"быстрое сохранение"`. The base comes first, the game's own comment wins;
/// a binding the game redefines without a comment loses the base one.
pub fn binding_notes(path: &Path) -> HashMap<String, String> {
    let mut notes = HashMap::new();
    collect_notes(path, &mut notes, 0);
    notes
}

fn collect_notes(path: &Path, notes: &mut HashMap<String, String>, depth: usize) {
    let Ok(text) = std::fs::read_to_string(path) else { return };
    if depth <= 8 {
        if let Some(Value::String(base)) = toml::from_str::<Table>(&text).ok().and_then(|mut t| t.remove("extends")) {
            collect_notes(&sibling(path, &base), notes, depth + 1);
        }
    }
    let mut section = String::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.split(']').next()) {
            section = name.trim().to_string();
            continue;
        }
        let Some((key, rest)) = line.split_once('=') else { continue };
        let key = key.trim();
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            continue;
        }
        let full = format!("{section}.{key}");
        // A `#` inside a quoted key name is not a comment.
        let mut quoted = false;
        let comment = rest.char_indices().find_map(|(i, c)| {
            if c == '"' {
                quoted = !quoted;
            }
            (c == '#' && !quoted).then(|| rest[i + 1..].trim().to_string())
        });
        match comment.filter(|c| !c.is_empty()) {
            Some(c) => notes.insert(full, c),
            None => notes.remove(&full),
        };
    }
}

fn load_merged(path: &Path, depth: usize) -> Result<Table, ProfileError> {
    if depth > 8 {
        return err("`extends` chain is too long (loop?)");
    }
    let text = std::fs::read_to_string(path).map_err(|e| ProfileError(format!("{}: {e}", path.display())))?;
    let mut table: Table = toml::from_str(&text).map_err(|e| ProfileError(format!("{}: {e}", path.display())))?;
    let Some(Value::String(base)) = table.remove("extends") else { return Ok(table) };
    let base_path = sibling(path, &base);
    let mut merged = load_merged(&base_path, depth + 1)?;
    merge_profile(&mut merged, table);
    Ok(merged)
}

fn sibling(path: &Path, name: &str) -> PathBuf {
    let mut p = path.parent().unwrap_or(Path::new(".")).join(name);
    if p.extension().is_none() {
        p.set_extension("toml");
    }
    p
}

fn merge_profile(base: &mut Table, over: Table) {
    for (key, value) in over {
        match (key.as_str(), base.get_mut(&key), value) {
            ("buttons", Some(Value::Table(b)), Value::Table(o)) => replace_entries(b, o),
            ("layer", Some(Value::Table(b)), Value::Table(o)) => {
                for (name, layer) in o {
                    match (b.get_mut(&name), layer) {
                        (Some(Value::Table(bl)), Value::Table(ol)) => replace_entries(bl, ol),
                        (_, layer) => {
                            b.insert(name, layer);
                        }
                    }
                }
            }
            (_, Some(Value::Table(b)), Value::Table(o)) => {
                for (k, v) in o {
                    b.insert(k, v);
                }
            }
            (_, _, value) => {
                base.insert(key, value);
            }
        }
    }
}

fn replace_entries(base: &mut Table, over: Table) {
    for (k, v) in over {
        base.insert(k, v);
    }
}

// ---------- compiling ----------

fn outputs(keys: &Keys, ctx: &str) -> Result<Vec<Output>, ProfileError> {
    let names = match keys {
        Keys::One(s) => vec![s.clone()],
        Keys::Many(v) => v.clone(),
    };
    let mut out = Vec::new();
    for n in names {
        if n.eq_ignore_ascii_case("none") || n.is_empty() {
            continue;
        }
        match Output::parse(&n) {
            Some(o) => out.push(o),
            None => return err(format!("{ctx}: unknown key \"{n}\"")),
        }
    }
    Ok(out)
}

fn opt_outputs(keys: &Option<Keys>, ctx: &str) -> Result<Vec<Output>, ProfileError> {
    keys.as_ref().map_or(Ok(Vec::new()), |k| outputs(k, ctx))
}

fn binding(raw: &RawBinding, ctx: &str) -> Result<Binding, ProfileError> {
    let b = match raw {
        RawBinding::Keys(k) => Binding { press: outputs(k, ctx)?, ..Binding::default() },
        RawBinding::Table(t) => Binding {
            press: opt_outputs(&t.press, ctx)?,
            tap: opt_outputs(&t.tap, ctx)?,
            hold: opt_outputs(&t.hold, ctx)?,
            double: opt_outputs(&t.double, ctx)?,
            next: opt_outputs(&t.next, ctx)?,
            latch: opt_outputs(&t.latch, ctx)?,
            hold_ms: t.hold_ms,
        },
    };
    if !b.press.is_empty() && (!b.tap.is_empty() || !b.hold.is_empty()) {
        return err(format!("{ctx}: `press` cannot be combined with `tap`/`hold`"));
    }
    let others = !b.press.is_empty() || !b.tap.is_empty() || !b.hold.is_empty() || !b.double.is_empty();
    if !b.latch.is_empty() && (others || !b.next.is_empty()) {
        return err(format!("{ctx}: `latch` cannot be combined with other fields"));
    }
    if b.hold_ms.is_some() && b.hold.is_empty() {
        return err(format!("{ctx}: `hold_ms` needs `hold`"));
    }
    if !b.next.is_empty() && b.press.is_empty() {
        return err(format!("{ctx}: `next` needs `press`"));
    }
    if !b.double.is_empty() && b.press.is_empty() && b.tap.is_empty() {
        return err(format!("{ctx}: `double` needs `press` or `tap`"));
    }
    Ok(b)
}

fn btn(name: &str, ctx: &str) -> Result<Btn, ProfileError> {
    Btn::parse(name).ok_or_else(|| ProfileError(format!("{ctx}: unknown controller input \"{name}\"")))
}

fn opt_btn(name: &str, ctx: &str) -> Result<Option<Btn>, ProfileError> {
    if name.is_empty() || name.eq_ignore_ascii_case("none") {
        Ok(None)
    } else {
        btn(name, ctx).map(Some)
    }
}

fn opt_key(name: &str, ctx: &str) -> Result<Option<Output>, ProfileError> {
    if name.is_empty() || name.eq_ignore_ascii_case("none") {
        return Ok(None);
    }
    Output::parse(name).map(Some).ok_or_else(|| ProfileError(format!("{ctx}: unknown key \"{name}\"")))
}

/// Face buttons in weapon order: □ 1/2, ✕ 3/4, ○ 5/6, △ 7/8.
const WEAPON_BUTTONS: [&str; 4] = ["square", "cross", "circle", "triangle"];

fn compile(mut raw: Raw, fallback_name: &str) -> Result<Profile, ProfileError> {
    if let Some(w) = raw.weapons.take() {
        let layer = raw.layer.entry(w.layer.clone()).or_default();
        let pairs = match w.slots {
            4 => 1,
            8 => 2,
            n => return err(format!("weapons.slots must be 4 or 8, got {n}")),
        };
        if w.keys.len() != w.slots as usize {
            return err(format!("weapons.keys needs {} keys, got {}", w.slots, w.keys.len()));
        }
        for (i, name) in WEAPON_BUTTONS.iter().enumerate() {
            let table = RawBindingTable {
                press: Some(Keys::One(w.keys[i * pairs].clone())),
                next: (pairs == 2).then(|| Keys::One(w.keys[i * 2 + 1].clone())),
                ..RawBindingTable::default()
            };
            // An explicit `[layer.*]` binding beats the weapon list.
            layer.entry(name.to_string()).or_insert(RawBinding::Table(table));
        }
    }

    let mut buttons = HashMap::new();
    for (name, b) in &raw.buttons {
        let ctx = format!("buttons.{name}");
        let bound = binding(b, &ctx)?;
        if !bound.is_empty() {
            buttons.insert(btn(name, &ctx)?, bound);
        }
    }
    let mut layers = Vec::new();
    for (layer_name, entries) in &raw.layer {
        let button = btn(layer_name, &format!("layer.{layer_name}"))?;
        let mut bindings = HashMap::new();
        for (name, b) in entries {
            let ctx = format!("layer.{layer_name}.{name}");
            let bound = binding(b, &ctx)?;
            if !bound.is_empty() {
                bindings.insert(btn(name, &ctx)?, bound);
            }
        }
        layers.push(Layer { button, bindings });
    }

    let m = &raw.movement;
    let mode = match m.mode.to_ascii_lowercase().as_str() {
        "hybrid" => MoveMode::Hybrid,
        "wasd" => MoveMode::Wasd,
        other => return err(format!("movement.mode must be \"hybrid\" or \"wasd\", got \"{other}\"")),
    };
    if m.keys.len() != 4 {
        return err("movement.keys needs 4 keys: forward, left, back, right");
    }
    let mut keys = [Output::WheelUp; 4];
    for (i, k) in m.keys.iter().enumerate() {
        keys[i] = opt_key(k, "movement.keys")?.ok_or_else(|| ProfileError("movement.keys cannot be empty".into()))?;
    }
    let c = &raw.calibration;
    if c.real_world <= 0.0 || c.in_game_sens <= 0.0 {
        return err("calibration.real_world and in_game_sens must be positive");
    }
    let g = &raw.gyro;
    Ok(Profile {
        game: raw.game.clone().unwrap_or_else(|| fallback_name.to_string()),
        exe: raw.exe.clone(),
        calibration: Calibration { real_world: c.real_world, in_game_sens: c.in_game_sens },
        gyro: GyroCfg {
            enabled: g.enabled,
            min_sens: g.min_sens,
            max_sens: g.max_sens,
            min_threshold: g.min_threshold,
            max_threshold: g.max_threshold.max(g.min_threshold),
            off: opt_btn(&g.off, "gyro.off")?,
            calibrate: opt_btn(&g.calibrate, "gyro.calibrate")?,
            startup_calibration: g.startup_calibration.max(0.0),
            invert_x: g.invert_x,
            invert_y: g.invert_y,
        },
        flick: FlickCfg {
            enabled: raw.flick.enabled,
            time: raw.flick.time.max(0.0),
            threshold: raw.flick.threshold.clamp(0.3, 1.0),
            forward_deadzone: raw.flick.forward_deadzone.clamp(0.0, 90.0),
            deadzone: deadzone(raw.flick.deadzone_inner, raw.flick.deadzone_outer, "flick")?,
        },
        movement: MovementCfg {
            mode,
            deadzone: deadzone(m.deadzone_inner, m.deadzone_outer, "movement")?,
            keys,
            walk: opt_key(&m.walk, "movement.walk")?,
            sprint: opt_key(&m.sprint, "movement.sprint")?,
            ring: m.ring.clamp(0.05, 1.0),
        },
        timing: Timing { hold_ms: raw.timing.hold_ms, double_ms: raw.timing.double_ms, tap_ms: raw.timing.tap_ms },
        trigger_threshold: raw.triggers.threshold.clamp(0.0, 0.99),
        lightbar: LightbarCfg {
            enabled: raw.lightbar.enabled,
            idle: color(&raw.lightbar.idle, "lightbar.idle")?,
            pairs: vec![
                (Btn::Square, color(&raw.lightbar.square, "lightbar.square")?),
                (Btn::Cross, color(&raw.lightbar.cross, "lightbar.cross")?),
                (Btn::Circle, color(&raw.lightbar.circle, "lightbar.circle")?),
                (Btn::Triangle, color(&raw.lightbar.triangle, "lightbar.triangle")?),
            ],
        },
        hide_controller: raw.controller.hide,
        buttons,
        layers,
    })
}

/// "RRGGBB" (an optional leading # is allowed).
fn color(hex: &str, ctx: &str) -> Result<[u8; 3], ProfileError> {
    let h = hex.trim().trim_start_matches('#');
    let n = (h.len() == 6).then(|| u32::from_str_radix(h, 16).ok()).flatten();
    match n {
        Some(n) => Ok([(n >> 16) as u8, (n >> 8) as u8, n as u8]),
        None => err(format!("{ctx}: colour must be RRGGBB, got \"{hex}\"")),
    }
}

fn deadzone(inner: f32, outer: f32, ctx: &str) -> Result<Deadzone, ProfileError> {
    if !(0.0..1.0).contains(&inner) || !(0.0..1.0).contains(&outer) || inner + outer >= 1.0 {
        return err(format!("{ctx}: deadzone_inner + deadzone_outer must be below 1"));
    }
    Ok(Deadzone { inner, outer })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(n: &str) -> Output {
        Output::parse(n).unwrap()
    }

    #[test]
    fn explicit_layer_binding_beats_weapons() {
        let p = Profile::parse(
            "[weapons]\nslots = 8\nkeys = [\"1\",\"2\",\"3\",\"4\",\"5\",\"6\",\"7\",\"8\"]\n[layer.r2]\nsquare = \"K\"",
        )
        .unwrap();
        let r2 = &p.layers[0];
        assert_eq!(r2.bindings[&Btn::Square].press, vec![Output::parse("K").unwrap()]);
        assert_eq!(r2.bindings[&Btn::Cross].next, vec![Output::parse("4").unwrap()]);
    }

    #[test]
    fn notes_come_from_comments_through_extends() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles");
        let notes = binding_notes(&dir.join("testrange.toml"));
        assert_eq!(notes["buttons.l1"], "прыжок", "from _base");
        assert_eq!(notes["buttons.dpad_up"], "фонарик", "from the game");
        assert_eq!(notes["layer.r2.create"], "быстрое сохранение");
        assert!(!notes.contains_key("calibration.in_game_sens"));
    }

    #[test]
    fn parses_bindings_and_weapons() {
        let p = Profile::parse(
            r#"
            [buttons]
            l1 = "Space"
            options = { tap = "Esc", hold = "M" }
            dpad_up = ["LShift", "W"]
            ps = "None"
            [weapons]
            slots = 8
            keys = ["1", "2", "3", "4", "5", "6", "7", "8"]
            [layer.r2]
            create = "F5"
            "#,
        )
        .unwrap();
        assert_eq!(p.buttons[&Btn::L1].press, vec![key("Space")]);
        assert_eq!(p.buttons[&Btn::Options].tap, vec![key("Esc")]);
        assert_eq!(p.buttons[&Btn::Up].press.len(), 2);
        assert!(!p.buttons.contains_key(&Btn::Ps));
        let r2 = p.layers.iter().find(|l| l.button == Btn::R2).unwrap();
        assert_eq!(r2.bindings[&Btn::Square].press, vec![key("1")]);
        assert_eq!(r2.bindings[&Btn::Square].next, vec![key("2")]);
        assert_eq!(r2.bindings[&Btn::Triangle].next, vec![key("8")]);
        assert_eq!(r2.bindings[&Btn::Create].press, vec![key("F5")]);
    }

    #[test]
    fn four_weapon_slots_have_no_double() {
        let p = Profile::parse("[weapons]\nslots = 4\nkeys = [\"1\",\"2\",\"3\",\"4\"]").unwrap();
        let r2 = &p.layers[0];
        assert_eq!(r2.bindings[&Btn::Cross].press, vec![key("2")]);
        assert!(r2.bindings[&Btn::Cross].next.is_empty());
    }

    #[test]
    fn rejects_mistakes() {
        assert!(Profile::parse("[buttons]\nl1 = \"Spacebar\"").is_err());
        assert!(Profile::parse("[buttons]\nl5 = \"Space\"").is_err());
        assert!(Profile::parse("[buttons]\nl1 = { press = \"A\", tap = \"B\" }").is_err());
        assert!(Profile::parse("[buttons]\nl1 = { double = \"A\" }").is_err());
        assert!(Profile::parse("[gyro]\nsens = 2").is_err());
        assert!(Profile::parse("[movement]\nmode = \"stick\"").is_err());
    }

    #[test]
    fn child_replaces_whole_bindings_and_merges_settings() {
        let dir = std::env::temp_dir().join(format!("aikm-profile-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("_base.toml"),
            "[gyro]\nmin_sens = 1.0\nmax_sens = 2.0\n[buttons]\noptions = { tap = \"Esc\", hold = \"M\" }\nl1 = \"Space\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("game.toml"), "extends = \"_base\"\n[gyro]\nmax_sens = 4.0\n[buttons]\noptions = \"P\"\n")
            .unwrap();
        let p = Profile::load(&dir.join("game.toml")).unwrap();
        assert_eq!((p.gyro.min_sens, p.gyro.max_sens), (1.0, 4.0));
        assert_eq!(p.buttons[&Btn::Options], Binding { press: vec![key("P")], ..Binding::default() });
        assert_eq!(p.buttons[&Btn::L1].press, vec![key("Space")]);
        assert_eq!(p.game, "game");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
