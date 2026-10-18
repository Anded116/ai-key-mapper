use gilrs::{Axis, Event, EventType, Gilrs};
use macroquad::prelude::*;

use crate::ds::{PadHid, Sticks};
use crate::input::Input;

const DEADZONE: f32 = 0.15;
/// Sticks rarely reach a full 1.0 on diagonals; the last 10% counts as full tilt.
const DEADZONE_OUTER: f32 = 0.1;
const ACTIVITY: f32 = 0.25;

pub struct PadInfo {
    pub name: String,
    pub left: Vec2,
    pub right: Vec2,
}

/// Gamepads as the game sees them. If HidHide hides the DualSense from this
/// process, neither gilrs nor HID sees it while JoyShockMapper still reads the pad.
pub struct Pads {
    gilrs: Option<Gilrs>,
    hid: PadHid,
    pub error: Option<String>,
    pub list: Vec<PadInfo>,
    /// Sticks read straight from HID, when a PlayStation pad is visible that way.
    pub hid_sticks: Option<Sticks>,
    pub last_activity: f64,
}

impl Pads {
    pub fn new() -> Self {
        let (gilrs, error) = match Gilrs::new() {
            Ok(g) => (Some(g), None),
            Err(e) => (None, Some(e.to_string())),
        };
        Self { gilrs, hid: PadHid::start(), error, list: Vec::new(), hid_sticks: None, last_activity: -1.0 }
    }

    pub fn update(&mut self, now: f64, input: &mut Input) {
        self.hid_sticks = self.hid.sticks();
        if let Some(g) = self.gilrs.as_mut() {
            let pad_color = Color::new(1.0, 0.6, 0.9, 1.0);
            while let Some(Event { id, event, .. }) = g.next_event() {
                let n = usize::from(id);
                match event {
                    EventType::ButtonPressed(b, _) => {
                        self.last_activity = now;
                        input.push_log(now, format!("PAD{n} +{b:?}"), pad_color);
                    }
                    EventType::ButtonReleased(b, _) => {
                        input.push_log(now, format!("PAD{n} −{b:?}"), pad_color);
                    }
                    EventType::Connected => input.push_log(now, format!("PAD{n} подключён"), pad_color),
                    EventType::Disconnected => input.push_log(now, format!("PAD{n} отключён"), pad_color),
                    _ => {}
                }
            }
            self.list = g
                .gamepads()
                .map(|(_, gp)| PadInfo {
                    name: gp.name().to_string(),
                    left: vec2(gp.value(Axis::LeftStickX), gp.value(Axis::LeftStickY)),
                    right: vec2(gp.value(Axis::RightStickX), gp.value(Axis::RightStickY)),
                })
                .collect();
        }
        if self.raw_left().length() > ACTIVITY || self.raw_right().length() > ACTIVITY {
            self.last_activity = now;
        }
    }

    pub fn visible(&self) -> bool {
        self.hid_sticks.is_some() || !self.list.is_empty()
    }

    pub fn name(&self) -> Option<String> {
        match (&self.hid_sticks, self.list.first()) {
            (Some(s), _) => Some(s.name.clone()),
            (None, Some(p)) => Some(p.name.clone()),
            _ => None,
        }
    }

    fn raw_left(&self) -> Vec2 {
        match (&self.hid_sticks, self.list.first()) {
            (Some(s), _) => s.left,
            (None, Some(p)) => p.left,
            _ => Vec2::ZERO,
        }
    }

    fn raw_right(&self) -> Vec2 {
        match (&self.hid_sticks, self.list.first()) {
            (Some(s), _) => s.right,
            (None, Some(p)) => p.right,
            _ => Vec2::ZERO,
        }
    }

    /// Left stick, radial deadzone rescaled to 0..1, +Y = forward.
    pub fn move_stick(&self) -> Vec2 {
        let left = self.raw_left();
        let len = left.length();
        if len < DEADZONE {
            return Vec2::ZERO;
        }
        left / len * ((len - DEADZONE) / (1.0 - DEADZONE - DEADZONE_OUTER)).min(1.0)
    }
}
