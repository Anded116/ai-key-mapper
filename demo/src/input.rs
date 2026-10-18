use std::collections::{HashMap, VecDeque};

use macroquad::input::utils::{register_input_subscriber, repeat_all_miniquad_input};
use macroquad::miniquad::{EventHandler, KeyMods};
use macroquad::prelude::*;

/// Default JoyShockMapper HOLD_PRESS_TIME, used to label presses as tap or hold.
pub const HOLD_MS: f64 = 150.0;
const DOUBLE_WINDOW: f64 = 0.3;
const LOG_LEN: usize = 24;
const TURN_IDLE: f64 = 0.12;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Btn {
    Key(KeyCode),
    Mouse(MouseButton),
    WheelUp,
    WheelDown,
    WheelLeft,
    WheelRight,
}

pub fn btn_name(b: Btn) -> String {
    match b {
        Btn::Key(k) => format!("{k:?}"),
        Btn::Mouse(MouseButton::Left) => "LMB".into(),
        Btn::Mouse(MouseButton::Right) => "RMB".into(),
        Btn::Mouse(MouseButton::Middle) => "MMB".into(),
        Btn::Mouse(MouseButton::Unknown) => "Mouse?".into(),
        Btn::WheelUp => "Wheel↑".into(),
        Btn::WheelDown => "Wheel↓".into(),
        Btn::WheelLeft => "Wheel←".into(),
        Btn::WheelRight => "Wheel→".into(),
    }
}

enum Raw {
    Down(Btn),
    Up(Btn),
    Motion(f32, f32),
    Wheel(f32, f32),
}

#[derive(Default)]
struct Collector {
    evs: Vec<Raw>,
}

impl EventHandler for Collector {
    fn update(&mut self) {}
    fn draw(&mut self) {}
    fn mouse_motion_event(&mut self, x: f32, y: f32) {
        self.evs.push(Raw::Motion(x, y));
    }
    fn mouse_wheel_event(&mut self, x: f32, y: f32) {
        self.evs.push(Raw::Wheel(x, y));
    }
    fn mouse_button_down_event(&mut self, b: MouseButton, _x: f32, _y: f32) {
        self.evs.push(Raw::Down(Btn::Mouse(b)));
    }
    fn mouse_button_up_event(&mut self, b: MouseButton, _x: f32, _y: f32) {
        self.evs.push(Raw::Up(Btn::Mouse(b)));
    }
    fn key_down_event(&mut self, k: KeyCode, _m: KeyMods, repeat: bool) {
        if !repeat {
            self.evs.push(Raw::Down(Btn::Key(k)));
        }
    }
    fn key_up_event(&mut self, k: KeyCode, _m: KeyMods) {
        self.evs.push(Raw::Up(Btn::Key(k)));
    }
}

pub struct LogEntry {
    pub t: f64,
    pub text: String,
    pub color: Color,
}

/// Event-driven input: every press is seen even when it is shorter than a frame,
/// which is exactly what a too-short JSM tap looks like.
pub struct Input {
    sub: usize,
    col: Collector,
    last_pos: Option<Vec2>,
    pub delta: Vec2,
    pub down: HashMap<Btn, f64>,
    pub pressed: Vec<Btn>,
    last_press: HashMap<Btn, f64>,
    pub log: VecDeque<LogEntry>,
    pub last_kbm: f64,
    pub counts_total: Vec2,
    last_motion: f64,
    pub turn_counts: f32,
    pub turn_done: bool,
    rate_acc: f32,
    rate_t0: f64,
    pub counts_per_sec: f32,
}

impl Input {
    pub fn new() -> Self {
        Self {
            sub: register_input_subscriber(),
            col: Collector::default(),
            last_pos: None,
            delta: Vec2::ZERO,
            down: HashMap::new(),
            pressed: Vec::new(),
            last_press: HashMap::new(),
            log: VecDeque::new(),
            last_kbm: -1.0,
            counts_total: Vec2::ZERO,
            last_motion: -1.0,
            turn_counts: 0.0,
            turn_done: true,
            rate_acc: 0.0,
            rate_t0: 0.0,
            counts_per_sec: 0.0,
        }
    }

    pub fn push_log(&mut self, now: f64, text: String, color: Color) {
        self.log.push_front(LogEntry { t: now, text, color });
        self.log.truncate(LOG_LEN);
    }

    pub fn update(&mut self, now: f64, grabbed: bool, action_of: impl Fn(Btn) -> Option<&'static str>) {
        self.delta = Vec2::ZERO;
        self.pressed.clear();
        self.col.evs.clear();
        repeat_all_miniquad_input(&mut self.col, self.sub);
        let evs = std::mem::take(&mut self.col.evs);
        for ev in evs {
            match ev {
                Raw::Motion(x, y) => {
                    let p = vec2(x, y);
                    if grabbed {
                        if let Some(last) = self.last_pos {
                            self.delta += p - last;
                        }
                        self.last_pos = Some(p);
                    } else {
                        self.last_pos = None;
                    }
                }
                Raw::Down(b) => {
                    if self.down.contains_key(&b) {
                        continue;
                    }
                    self.down.insert(b, now);
                    self.pressed.push(b);
                    self.last_kbm = now;
                    let dbl = self.last_press.get(&b).map(|t| now - t).filter(|d| *d < DOUBLE_WINDOW);
                    self.last_press.insert(b, now);
                    let action = action_of(b).map(|a| format!(" → {a}")).unwrap_or_default();
                    let dbl = dbl.map(|d| format!("  ×2 через {:.0} мс", d * 1000.0)).unwrap_or_default();
                    let color = if action.is_empty() { GRAY } else { WHITE };
                    self.push_log(now, format!("+{}{action}{dbl}", btn_name(b)), color);
                }
                Raw::Up(b) => {
                    let Some(t0) = self.down.remove(&b) else { continue };
                    let ms = (now - t0) * 1000.0;
                    let (kind, color) = if ms < 0.5 {
                        ("короче кадра!", ORANGE)
                    } else if ms < HOLD_MS {
                        ("tap", Color::new(0.55, 0.85, 1.0, 1.0))
                    } else {
                        ("hold", Color::new(0.75, 1.0, 0.55, 1.0))
                    };
                    self.push_log(now, format!("−{}  {ms:.0} мс  {kind}", btn_name(b)), color);
                }
                Raw::Wheel(x, y) => {
                    let b = if y > 0.0 {
                        Btn::WheelUp
                    } else if y < 0.0 {
                        Btn::WheelDown
                    } else if x > 0.0 {
                        Btn::WheelRight
                    } else {
                        Btn::WheelLeft
                    };
                    self.pressed.push(b);
                    self.last_kbm = now;
                    let action = action_of(b).map(|a| format!(" → {a}")).unwrap_or_default();
                    self.push_log(now, format!("•{}{action}", btn_name(b)), YELLOW);
                }
            }
        }
        self.track_motion(now);
    }

    fn track_motion(&mut self, now: f64) {
        if self.delta != Vec2::ZERO {
            self.counts_total += self.delta;
            self.last_kbm = now;
            if now - self.last_motion > TURN_IDLE {
                self.turn_counts = 0.0;
            }
            self.turn_counts += self.delta.x;
            self.turn_done = false;
            self.last_motion = now;
        } else if now - self.last_motion > TURN_IDLE {
            self.turn_done = true;
        }
        self.rate_acc += self.delta.length();
        if now - self.rate_t0 >= 0.25 {
            self.counts_per_sec = self.rate_acc / (now - self.rate_t0) as f32;
            self.rate_acc = 0.0;
            self.rate_t0 = now;
        }
    }

    pub fn held(&self, b: Btn) -> bool {
        self.down.contains_key(&b)
    }

    pub fn key(&self, k: KeyCode) -> bool {
        self.held(Btn::Key(k))
    }

    pub fn hit(&self, b: Btn) -> bool {
        self.pressed.contains(&b)
    }

    pub fn key_hit(&self, k: KeyCode) -> bool {
        self.hit(Btn::Key(k))
    }

    pub fn reset_counters(&mut self) {
        self.counts_total = Vec2::ZERO;
        self.turn_counts = 0.0;
    }
}
