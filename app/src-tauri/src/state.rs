//! Снимок состояния для UI: событие `state` по контракту из docs/architecture.md, плюс строки
//! журнала движка событием `log`. Кадры приходят из `engine::runner` ~60 раз в секунду.

use std::path::PathBuf;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use dualsense::Bus;
use engine::runner::{self, Command, Config, Frame, Handle};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

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
            s.pad = Some(Pad { model: info.model.name().into(), bus: bus.into(), battery: pad.battery });
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
            s.engine.movement = Some(format!("{:?}", snap.movement).to_lowercase());
        }
        s
    }
}

/// Корень репозитория: app/src-tauri → ../..
pub fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().and_then(|p| p.parent()).map(PathBuf::from).unwrap_or(manifest)
}

/// Сколько последних строк журнала помнить для окна, открывшегося позже движка.
const LOG_KEEP: usize = 20;

/// Движок в своём потоке; при закрытии окна отпускает клавиши и возвращает геймпад играм.
#[derive(Default)]
pub struct Mapper {
    handle: Mutex<Option<Handle>>,
    log: Arc<Mutex<VecDeque<String>>>,
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
        let on_frame = Box::new(move |f: &Frame| {
            let _ = frames.emit("state", State::from(f));
        });
        let log = self.log.clone();
        let on_log = Box::new(move |msg: &str| {
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

/// Журнал движка с начала работы, старые строки первыми.
#[tauri::command]
pub fn engine_log(mapper: tauri::State<'_, Mapper>) -> Vec<String> {
    mapper.log.lock().unwrap().iter().cloned().collect()
}
