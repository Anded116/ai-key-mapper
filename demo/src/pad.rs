use gilrs::{Axis, Event, EventType, Gilrs};
use macroquad::prelude::*;

use crate::input::Input;

const DEADZONE: f32 = 0.15;
/// Sticks rarely reach a full 1.0 on diagonals; the last 10% counts as full tilt.
const DEADZONE_OUTER: f32 = 0.1;
const ACTIVITY: f32 = 0.25;

pub struct PadInfo {
    pub name: String,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub left: Vec2,
    pub right: Vec2,
}

/// Gamepads as the game sees them. If HidHide hides the DualSense from this
/// process, the list is empty while JoyShockMapper still reads the pad.
pub struct Pads {
    gilrs: Option<Gilrs>,
    pub error: Option<String>,
    pub list: Vec<PadInfo>,
    pub last_activity: f64,
}

impl Pads {
    pub fn new() -> Self {
        match Gilrs::new() {
            Ok(g) => Self { gilrs: Some(g), error: None, list: Vec::new(), last_activity: -1.0 },
            Err(e) => Self { gilrs: None, error: Some(e.to_string()), list: Vec::new(), last_activity: -1.0 },
        }
    }

    pub fn update(&mut self, now: f64, input: &mut Input) {
        let Some(g) = self.gilrs.as_mut() else { return };
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
                vid: gp.vendor_id(),
                pid: gp.product_id(),
                left: vec2(gp.value(Axis::LeftStickX), gp.value(Axis::LeftStickY)),
                right: vec2(gp.value(Axis::RightStickX), gp.value(Axis::RightStickY)),
            })
            .collect();
        if self.list.iter().any(|p| p.left.length() > ACTIVITY || p.right.length() > ACTIVITY) {
            self.last_activity = now;
        }
    }

    /// Left stick of the first pad, radial deadzone rescaled to 0..1, +Y = forward.
    pub fn move_stick(&self) -> Vec2 {
        let Some(p) = self.list.first() else { return Vec2::ZERO };
        let len = p.left.length();
        if len < DEADZONE {
            return Vec2::ZERO;
        }
        p.left / len * ((len - DEADZONE) / (1.0 - DEADZONE - DEADZONE_OUTER)).min(1.0)
    }
}
