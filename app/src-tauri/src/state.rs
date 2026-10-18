//! Снимок состояния для UI: событие `state` по контракту из docs/architecture.md.
//! Пока движка нет, его шлёт заглушка `spawn_stub`; потом те же структуры будет заполнять engine.

use serde::Serialize;
use std::f32::consts::TAU;
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

#[derive(Serialize, Clone, Debug)]
pub struct State {
    pub pad: Pad,
    /// Нажатые кнопки: L1 R1 L2 R2 L3 R3, Cross Circle Square Triangle, Up Down Left Right,
    /// Create Options PS Touchpad Mic.
    pub buttons: Vec<String>,
    pub sticks: Sticks,
    pub triggers: Triggers,
    pub gyro_dps: [f32; 3],
    pub touch: Vec<Touch>,
    /// Активный слой: `None` — базовый, `Some("r2")` — пока держат R2.
    pub layer: Option<String>,
    /// Что сейчас уходит в игру: клавиши и кнопки мыши.
    pub outputs: Vec<String>,
    pub profile: Option<String>,
    pub weapons: Weapons,
}

#[derive(Serialize, Clone, Debug)]
pub struct Pad {
    pub model: String,
    /// `usb` | `bluetooth`
    pub bus: String,
    pub battery: f32,
}

#[derive(Serialize, Clone, Debug)]
pub struct Sticks {
    /// x вправо, y вверх, -1..1
    pub left: [f32; 2],
    pub right: [f32; 2],
}

#[derive(Serialize, Clone, Debug)]
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

#[derive(Serialize, Clone, Debug)]
pub struct Weapons {
    pub current: Option<u8>,
    /// Лицевая кнопка, повторное нажатие которой даст чётный слот: cross | circle | square | triangle.
    pub next_double: Option<String>,
}

/// Лицевые кнопки слоя R2 и их пары слотов: □ 1/2, ✕ 3/4, ○ 5/6, △ 7/8.
const WEAPON_FACES: [(&str, u8); 4] = [("Square", 1), ("Cross", 3), ("Circle", 5), ("Triangle", 7)];

/// Клавиши базового слоя по стандарту раскладки, как в пресете TestRange.
const BASE_KEYS: [(&str, &str); 13] = [
    ("L1", "Space"),
    ("R1", "MouseLeft"),
    ("L2", "MouseRight"),
    ("L3", "LShift"),
    ("R3", "V"),
    ("Cross", "E"),
    ("Square", "R"),
    ("Triangle", "G"),
    ("Circle", "LCtrl"),
    ("Up", "F"),
    ("Down", "MouseMiddle"),
    ("Options", "Esc"),
    ("Create", "I"),
];

/// Кнопки, которые заглушка жмёт по очереди, по одной на полсекунды.
const CYCLE: [&str; 17] = [
    "Cross", "Circle", "Square", "Triangle", "L1", "R1", "Up", "Right", "Down", "Left", "L3", "R3", "Create",
    "Options", "PS", "Touchpad", "Mic",
];

/// Шлёт `state` ~60 раз в секунду из своего потока.
pub fn spawn_stub(app: AppHandle) {
    thread::spawn(move || {
        let start = Instant::now();
        let frame = Duration::from_micros(16_667);
        let mut stub = Stub::default();
        let mut next = Instant::now();
        loop {
            let state = stub.tick(start.elapsed().as_secs_f32());
            if app.emit("state", &state).is_err() {
                break;
            }
            next += frame;
            let now = Instant::now();
            if next > now {
                thread::sleep(next - now);
            } else {
                next = now;
            }
        }
    });
}

#[derive(Default)]
struct Stub {
    last_face: Option<&'static str>,
    pressed_face: Option<&'static str>,
    current: Option<u8>,
}

impl Stub {
    fn tick(&mut self, t: f32) -> State {
        let left = [(t * 0.9).sin() * 0.9, (t * 0.6).cos() * 0.9];
        let right = [(t * 1.7).sin() * 0.7, (t * 1.1).sin() * 0.5];
        let l2 = ((t * 0.8).sin() * 0.5 + 0.5).powi(2);
        // R2 держат 3 секунды из каждых 7: в это время лицевые кнопки выбирают оружие.
        let r2_phase = t % 7.0;
        let r2 = if r2_phase < 3.0 { (r2_phase * 4.0).min(1.0) } else { (1.0 - (r2_phase - 3.0) * 4.0).max(0.0) };
        let layer_r2 = r2 > 0.5;

        let mut buttons: Vec<String> = Vec::new();
        let step = (t * 2.0) as usize;
        let held = (t * 2.0).fract() < 0.6;
        let cycled = if layer_r2 {
            // В слое R2 жмём лицевые кнопки, иногда дважды подряд, чтобы видно было чётный слот.
            ["Square", "Square", "Circle", "Triangle", "Triangle", "Cross"][step % 6]
        } else {
            CYCLE[step % CYCLE.len()]
        };
        if held {
            buttons.push(cycled.to_string());
        }
        if l2 > 0.3 {
            buttons.push("L2".into());
        }
        if r2 > 0.3 {
            buttons.push("R2".into());
        }

        let face_now = if held && layer_r2 { WEAPON_FACES.iter().find(|(b, _)| *b == cycled) } else { None };
        match face_now {
            Some(&(face, odd)) if self.pressed_face != Some(face) => {
                let double = self.last_face == Some(face) && self.current == Some(odd);
                self.current = Some(if double { odd + 1 } else { odd });
                self.last_face = if double { None } else { Some(face) };
                self.pressed_face = Some(face);
            }
            Some(_) => {}
            None => self.pressed_face = None,
        }
        if !layer_r2 {
            self.last_face = None;
        }

        let mut outputs: Vec<String> = Vec::new();
        for b in &buttons {
            if layer_r2 {
                if let Some(&(_, odd)) = WEAPON_FACES.iter().find(|(f, _)| f == b) {
                    let slot = if self.current == Some(odd + 1) { odd + 1 } else { odd };
                    outputs.push(slot.to_string());
                    continue;
                }
            }
            if let Some(&(_, key)) = BASE_KEYS.iter().find(|(f, _)| f == b) {
                outputs.push(key.into());
            }
        }
        let [lx, ly] = left;
        if ly > 0.5 {
            outputs.push("W".into());
        }
        if ly < -0.5 {
            outputs.push("S".into());
        }
        if lx < -0.5 {
            outputs.push("A".into());
        }
        if lx > 0.5 {
            outputs.push("D".into());
        }

        // Палец на тачпаде 2 секунды из каждых 5, бегает по кругу; гироскоп в это время выключен.
        let touching = t % 5.0 < 2.0;
        let touch = if touching {
            let a = t * TAU * 0.4;
            vec![Touch { x: 0.5 + a.cos() * 0.3, y: 0.5 + a.sin() * 0.3 }]
        } else {
            Vec::new()
        };
        let gyro_dps = [(t * 2.3).sin() * 40.0, (t * 1.3).sin() * 120.0, (t * 0.7).cos() * 15.0];
        if !touching && gyro_dps[1].abs() > 5.0 {
            outputs.push("Mouse".into());
        }

        State {
            pad: Pad { model: "DualSense".into(), bus: "bluetooth".into(), battery: 0.8 - (t / 3600.0).min(0.7) },
            buttons,
            sticks: Sticks { left, right },
            triggers: Triggers { l2, r2 },
            gyro_dps,
            touch,
            layer: layer_r2.then(|| "r2".to_string()),
            outputs,
            profile: Some("testrange".into()),
            weapons: Weapons {
                current: self.current,
                next_double: self.last_face.map(|f| f.to_lowercase()),
            },
        }
    }
}
