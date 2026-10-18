//! Снимок состояния для UI: событие `state` по контракту из docs/architecture.md, плюс строки
//! журнала движка событием `log`. Кадры приходят из `engine::runner` ~60 раз в секунду.

use std::collections::{BTreeMap, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use dualsense::Bus;
use engine::profile::{binding_notes, Binding, Btn, Prompts};
use engine::runner::{self, Command, Config, Frame, Handle};
use engine::{Output, Profile};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::{agent, edit};

#[derive(Serialize, Clone, Debug, Default)]
pub struct State {
    /// `None` — геймпад не подключён.
    pub pad: Option<Pad>,
    /// Нажатые кнопки: L1 R1 L2 R2 L3 R3, Cross Circle Square Triangle, Up Down Left Right,
    /// Create Options PS Touchpad Mic.
    pub buttons: Vec<String>,
    pub sticks: Sticks,
    pub triggers: Triggers,
    /// Без смещения, найденного калибровкой.
    pub gyro_dps: [f32; 3],
    pub touch: Vec<Touch>,
    /// Активный слой: `None` — базовый, `Some("r2")` — пока держат R2.
    pub layer: Option<String>,
    /// Что сейчас уходит в игру: клавиши и кнопки мыши.
    pub outputs: Vec<String>,
    pub profile: Option<String>,
    pub weapons: Weapons,
    pub engine: EngineInfo,
}

#[derive(Serialize, Clone, Debug)]
pub struct Pad {
    pub model: String,
    /// `usb` | `bluetooth`
    pub bus: String,
    /// 0..1; `None`, пока геймпад не сообщил заряд.
    pub battery: Option<f32>,
    pub charging: bool,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Sticks {
    /// x вправо, y вверх, -1..1
    pub left: [f32; 2],
    pub right: [f32; 2],
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Triggers {
    pub l2: f32,
    pub r2: f32,
}

#[derive(Serialize, Clone, Debug)]
pub struct Touch {
    /// 0..1 от левого верхнего угла тачпада
    pub x: f32,
    pub y: f32,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Weapons {
    pub current: Option<u8>,
    /// Лицевая кнопка, повторное нажатие которой даст чётный слот: cross | circle | square | triangle.
    pub next_double: Option<String>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct EngineInfo {
    /// Окно игры из профиля в фокусе: только тогда движок шлёт ввод.
    pub focused: bool,
    pub paused: bool,
    /// Геймпад скрыт от игр через HidHide.
    pub hidden: bool,
    pub calibrating: bool,
    /// Идёт проверочный поворот на 360°.
    pub turning: bool,
    /// Режим техники: стик и курки уходят в виртуальный Xbox-геймпад.
    pub vehicle: bool,
    /// `hybrid` | `wasd`
    pub movement: Option<String>,
}

impl From<&Frame> for State {
    fn from(f: &Frame) -> Self {
        let mut s = State {
            profile: f.profile.clone(),
            engine: EngineInfo { focused: f.focused, paused: f.paused, hidden: f.hidden, ..Default::default() },
            ..Default::default()
        };
        let bias = f.snapshot.as_ref().map_or([0.0; 3], |snap| snap.gyro_bias);
        if let (Some(info), Some(pad)) = (&f.pad, &f.state) {
            let bus = match info.bus {
                Bus::Usb => "usb",
                Bus::Bluetooth => "bluetooth",
            };
            s.pad = Some(Pad { model: info.model.name().into(), bus: bus.into(), battery: pad.battery, charging: pad.charging });
            s.buttons = pad.buttons.names().map(String::from).collect();
            s.sticks = Sticks { left: pad.left, right: pad.right };
            s.triggers = Triggers { l2: pad.l2, r2: pad.r2 };
            s.gyro_dps = [0, 1, 2].map(|i| pad.gyro[i] - bias[i]);
            let (w, h) = info.model.touch_size();
            s.touch = pad
                .touch
                .iter()
                .filter(|t| t.active)
                .map(|t| Touch { x: t.x as f32 / w as f32, y: t.y as f32 / h as f32 })
                .collect();
        }
        if let Some(snap) = &f.snapshot {
            s.layer = snap.layer.map(|b| b.name().into());
            s.outputs = snap.outputs.iter().map(|o| o.name().into()).collect();
            s.weapons = Weapons {
                current: snap.weapon.and_then(|o| o.name().parse().ok()),
                next_double: snap.next_pair.map(|b| b.name().into()),
            };
            s.engine.calibrating = snap.gyro_calibrating;
            s.engine.turning = snap.turning;
            s.engine.vehicle = snap.vehicle;
            s.engine.movement = Some(format!("{:?}", snap.movement).to_lowercase());
        }
        s
    }
}

/// Что кнопки делают в игре — для подписей на схеме. Событие `bindings`, при каждой смене или
/// перезагрузке профиля.
#[derive(Serialize, Clone, Debug, Default)]
pub struct Bindings {
    pub profile: String,
    /// Файл профиля игры, куда пишутся правки.
    pub file: String,
    /// Как игра пишет клавиши в подсказках: `ru` (S → Ы) или `en`.
    pub prompts: &'static str,
    /// По id кнопки на схеме: `l1`, `cross`, `up`, `pad_left`…
    pub buttons: BTreeMap<String, Entry>,
    /// Слои: `r2` → кнопки слоя.
    pub layers: BTreeMap<String, BTreeMap<String, Entry>>,
    /// Клавиши левого стика в режиме WASD: вперёд, влево, назад, вправо.
    pub move_keys: Vec<String>,
    /// Отсчётов мыши на градус при чувствительности 1 и чувствительность в игре.
    pub real_world: f32,
    pub in_game_sens: f32,
    /// `[controller] hide` и режим стика по профилю: прятать имеет смысл только при WASD.
    pub hide: bool,
    pub wasd: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct Entry {
    pub parts: Vec<Part>,
    /// Комментарий к привязке в профиле.
    pub note: Option<String>,
    /// Задана в файле игры, а не унаследована из базового профиля.
    pub own: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct Part {
    /// `press` | `tap` | `hold` | `double` | `next` | `latch`
    pub how: &'static str,
    /// Имена клавиш движка: `Q`, `LShift`, `MouseLeft`…
    pub keys: Vec<String>,
}

/// Id кнопки на схеме: имя из профиля без `dpad_`.
fn pad_id(b: Btn) -> String {
    b.name().trim_start_matches("dpad_").to_string()
}

fn entry(b: &Binding, note: Option<&String>, own: bool) -> Option<Entry> {
    let names = |v: &[Output]| v.iter().map(|o| o.name().to_string()).collect::<Vec<_>>();
    let parts: Vec<Part> = [
        ("press", &b.press),
        ("tap", &b.tap),
        ("hold", &b.hold),
        ("double", &b.double),
        ("next", &b.next),
        ("latch", &b.latch),
    ]
    .into_iter()
    .filter(|(_, v)| !v.is_empty())
    .map(|(how, v)| Part { how, keys: names(v) })
    .collect();
    (!parts.is_empty()).then(|| Entry { parts, note: note.cloned(), own })
}

/// Привязки, заданные в самом файле игры: `buttons.l1`, `layer.r2.square`. `[weapons]` в файле
/// игры задаёт лицевые кнопки своего слоя.
fn own_keys(path: &Path) -> std::collections::HashSet<String> {
    let mut own = std::collections::HashSet::new();
    let Some(t) = std::fs::read_to_string(path).ok().and_then(|s| s.parse::<toml_edit::DocumentMut>().ok()) else { return own };
    if let Some(b) = t.get("buttons").and_then(|i| i.as_table_like()) {
        own.extend(b.iter().map(|(k, _)| format!("buttons.{k}")));
    }
    if let Some(layers) = t.get("layer").and_then(|i| i.as_table_like()) {
        for (name, layer) in layers.iter() {
            if let Some(l) = layer.as_table_like() {
                own.extend(l.iter().map(|(k, _)| format!("layer.{name}.{k}")));
            }
        }
    }
    if let Some(w) = t.get("weapons") {
        let layer = w.get("layer").and_then(|l| l.as_str()).unwrap_or("r2");
        own.extend(["square", "cross", "circle", "triangle"].map(|b| format!("layer.{layer}.{b}")));
    }
    own
}

impl Bindings {
    pub fn load(path: &Path) -> Option<Self> {
        let p = Profile::load(path).ok()?;
        let notes = binding_notes(path);
        let own = own_keys(path);
        let table = |bindings: &std::collections::HashMap<Btn, Binding>, section: &str| {
            bindings
                .iter()
                .filter_map(|(b, binding)| {
                    {
                    let key = format!("{section}.{}", b.name());
                    entry(binding, notes.get(&key), own.contains(&key)).map(|e| (pad_id(*b), e))
                }
                })
                .collect::<BTreeMap<_, _>>()
        };
        Some(Self {
            profile: p.game.clone(),
            prompts: match p.prompts {
                Prompts::Ru => "ru",
                Prompts::En => "en",
            },
            file: path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            buttons: table(&p.buttons, "buttons"),
            layers: p
                .layers
                .iter()
                .map(|l| (l.button.name().to_string(), table(&l.bindings, &format!("layer.{}", l.button.name()))))
                .collect(),
            move_keys: p.movement.keys.iter().map(|k| k.name().to_string()).collect(),
            real_world: p.calibration.real_world,
            hide: p.hide_controller,
            wasd: p.movement.mode == engine::profile::MoveMode::Wasd,
            in_game_sens: p.calibration.in_game_sens,
        })
    }
}

/// Корень репозитория: app/src-tauri → ../..
pub fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().and_then(|p| p.parent()).map(PathBuf::from).unwrap_or(manifest)
}

/// Журнал движка на диске, `%LOCALAPPDATA%\aikeymapper\engine.log`: по нему потом видно, когда
/// что случилось (например, успело ли скрытие геймпада до запуска игры). Больше мегабайта —
/// уходит в `engine.old.log`.
fn open_log_file() -> Option<File> {
    let dir = agent::data_dir();
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("engine.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 1 << 20) {
        let _ = std::fs::rename(&path, dir.join("engine.old.log"));
    }
    let mut f = OpenOptions::new().create(true).append(true).open(path).ok()?;
    let _ = writeln!(f, "\n{} — окно запущено", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    Some(f)
}

/// Сколько последних строк журнала помнить для окна, открывшегося позже движка.
const LOG_KEEP: usize = 20;

/// Движок в своём потоке; при закрытии окна отпускает клавиши и возвращает геймпад играм.
#[derive(Default)]
pub struct Mapper {
    handle: Mutex<Option<Handle>>,
    log: Arc<Mutex<VecDeque<String>>>,
    bindings: Arc<Mutex<Option<Bindings>>>,
    /// Файл активного профиля.
    profile: Arc<Mutex<Option<PathBuf>>>,
}

impl Mapper {
    pub fn start(&self, app: AppHandle) {
        let root = repo_root();
        let config = Config {
            source: root.join("profiles"),
            dry: std::env::var_os("AIKM_DRY").is_some(),
            hidhide_tool: Some(root.join("target").join("release").join("aikm-hidhide.exe")),
        };
        let frames = app.clone();
        let bindings = self.bindings.clone();
        let profile = self.profile.clone();
        let mut loaded: Option<(PathBuf, u32)> = None;
        let on_frame = Box::new(move |f: &Frame| {
            let current = f.profile_path.clone().map(|p| (p, f.profile_rev));
            if current != loaded {
                loaded = current;
                *profile.lock().unwrap() = loaded.as_ref().map(|(p, _)| p.clone());
                let b = loaded.as_ref().and_then(|(p, _)| Bindings::load(p));
                *bindings.lock().unwrap() = b.clone();
                let _ = frames.emit("bindings", b);
            }
            let _ = frames.emit("state", State::from(f));
        });
        let log = self.log.clone();
        let mut file = open_log_file();
        let on_log = Box::new(move |msg: &str| {
            if let Some(f) = &mut file {
                let _ = writeln!(f, "{} {msg}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"));
            }
            let mut log = log.lock().unwrap();
            if log.len() == LOG_KEEP {
                log.pop_front();
            }
            log.push_back(msg.to_string());
            let _ = app.emit("log", msg.to_string());
        });
        *self.handle.lock().unwrap() = Some(runner::spawn(config, on_frame, on_log));
    }

    pub fn send(&self, cmd: Command) {
        if let Some(h) = self.handle.lock().unwrap().as_ref() {
            h.send(cmd);
        }
    }

    pub fn stop(&self) {
        if let Some(h) = self.handle.lock().unwrap().take() {
            h.stop();
        }
    }
}

#[tauri::command]
pub fn set_paused(paused: bool, mapper: tauri::State<'_, Mapper>) {
    mapper.send(Command::SetPaused(paused));
}

#[tauri::command]
pub fn calibrate(mapper: tauri::State<'_, Mapper>) {
    mapper.send(Command::ToggleCalibration);
}

/// Проверочный поворот на 360°: движок сделает его через секунду после того, как игра получит фокус.
#[tauri::command]
pub fn test_turn(mapper: tauri::State<'_, Mapper>) {
    mapper.send(Command::TestTurn(360.0));
}

/// Новый `real_world` профиля, пересчитанный по проверочному повороту.
#[tauri::command]
pub fn set_real_world(file: String, value: f32) -> Result<(), String> {
    if !(value.is_finite() && value > 0.0) {
        return Err(format!("real_world должен быть больше нуля, а не {value}"));
    }
    edit::set_real_world(&profile_file(&file)?, value)
}

/// Прятать ли геймпад от игры этого профиля (HidHide); подхватит следующий запуск игры.
#[tauri::command]
pub fn set_hide(file: String, hide: bool) -> Result<(), String> {
    edit::set_hide(&profile_file(&file)?, hide)
}

/// Журнал движка с начала работы, старые строки первыми.
#[tauri::command]
pub fn engine_log(mapper: tauri::State<'_, Mapper>) -> Vec<String> {
    mapper.log.lock().unwrap().iter().cloned().collect()
}

/// Привязки активного профиля: окно спрашивает их при открытии, дальше ловит событие `bindings`.
#[tauri::command]
pub fn bindings(mapper: tauri::State<'_, Mapper>) -> Option<Bindings> {
    mapper.bindings.lock().unwrap().clone()
}

/// Профиль из папки `profiles` по имени файла; путей с каталогами не принимает.
fn profile_file(file: &str) -> Result<PathBuf, String> {
    let ok = file.ends_with(".toml") && !file.contains(['/', '\\']) && !file.contains("..");
    let path = repo_root().join("profiles").join(file);
    if ok && path.is_file() {
        Ok(path)
    } else {
        Err(format!("нет профиля {file}"))
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct ProfileInfo {
    pub file: String,
    pub name: String,
    pub exe: Option<String>,
    /// `_base.toml` и прочие файлы на `_`: стандарт, его наследуют игры.
    pub base: bool,
}

/// Все профили папки `profiles`: стандарт первым, игры по имени.
#[tauri::command]
pub fn profiles() -> Vec<ProfileInfo> {
    let Ok(entries) = std::fs::read_dir(repo_root().join("profiles")) else { return Vec::new() };
    let mut list: Vec<ProfileInfo> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .filter_map(|p| {
            let file = p.file_name()?.to_string_lossy().to_string();
            let base = file.starts_with('_');
            let profile = Profile::load(&p).ok()?;
            let name = if file == "_base.toml" { "Стандарт".to_string() } else { profile.game };
            Some(ProfileInfo { file, name, exe: profile.exe, base })
        })
        .collect();
    list.sort_by(|a, b| b.base.cmp(&a.base).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    list
}

/// Привязки любого профиля — смотреть и править без запущенной игры.
#[tauri::command]
pub fn profile_bindings(file: String) -> Result<Bindings, String> {
    let path = profile_file(&file)?;
    Bindings::load(&path).ok_or_else(|| format!("профиль {file} не загружается"))
}

/// Записать привязку кнопки в профиль; если он сейчас активен, движок перечитает его сам.
#[tauri::command]
pub fn set_binding(file: String, button: String, layer: Option<String>, edit: edit::Edit, note: String) -> Result<(), String> {
    edit::set(&profile_file(&file)?, &button, layer.as_deref(), &edit, &note)
}

/// Вернуть кнопке привязку из стандарта.
#[tauri::command]
pub fn reset_binding(file: String, button: String, layer: Option<String>) -> Result<(), String> {
    edit::reset(&profile_file(&file)?, &button, layer.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_list_standard_first() {
        let list = profiles();
        assert_eq!(list[0].file, "_base.toml");
        assert_eq!(list[0].name, "Стандарт");
        assert!(list.iter().any(|p| p.file == "SonsOfTheForest.toml" && !p.base));
        let base = profile_bindings("_base.toml".into()).unwrap();
        assert_eq!(base.buttons["down"].note.as_deref(), Some("фонарик"));
    }

    #[test]
    fn only_files_inside_profiles() {
        assert!(profile_file("../Cargo.toml").is_err());
        assert!(profile_file("..\\x.toml").is_err());
        assert!(profile_file("nope.toml").is_err());
        assert!(profile_file("testrange.toml").is_ok());
    }
}

/// Язык подсказок для профиля: меняет, крупно на кнопках русская буква или латинская.
#[tauri::command]
pub fn set_prompts(file: String, lang: String) -> Result<(), String> {
    if lang != "ru" && lang != "en" {
        return Err(format!("язык подсказок: ru или en, а не {lang}"));
    }
    edit::set_prompts(&profile_file(&file)?, &lang)
}
