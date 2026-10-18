//! Output names used in profiles: keyboard keys (sent as scan codes), mouse buttons, wheel.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Output {
    /// Set-1 scan code; `extended` adds the E0 prefix (arrows, right Ctrl, ...).
    Key { scan: u16, extended: bool },
    Mouse(MouseButton),
    WheelUp,
    WheelDown,
    /// An engine command instead of a key: fires once on press.
    Action(Action),
}

/// Engine commands a binding can trigger.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    /// Switch the left stick between hybrid (game reads it) and WASD.
    ToggleMovement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

const fn k(scan: u16) -> Output {
    Output::Key { scan, extended: false }
}

const fn e(scan: u16) -> Output {
    Output::Key { scan, extended: true }
}

/// Names are matched case-insensitively. Aliases follow JoyShockMapper where it helps import.
const NAMES: &[(&str, Output)] = &[
    ("Esc", k(0x01)),
    ("Escape", k(0x01)),
    ("1", k(0x02)),
    ("2", k(0x03)),
    ("3", k(0x04)),
    ("4", k(0x05)),
    ("5", k(0x06)),
    ("6", k(0x07)),
    ("7", k(0x08)),
    ("8", k(0x09)),
    ("9", k(0x0A)),
    ("0", k(0x0B)),
    ("Minus", k(0x0C)),
    ("Equals", k(0x0D)),
    ("Backspace", k(0x0E)),
    ("Tab", k(0x0F)),
    ("Q", k(0x10)),
    ("W", k(0x11)),
    ("E", k(0x12)),
    ("R", k(0x13)),
    ("T", k(0x14)),
    ("Y", k(0x15)),
    ("U", k(0x16)),
    ("I", k(0x17)),
    ("O", k(0x18)),
    ("P", k(0x19)),
    ("LeftBracket", k(0x1A)),
    ("RightBracket", k(0x1B)),
    ("Enter", k(0x1C)),
    ("LCtrl", k(0x1D)),
    ("LControl", k(0x1D)),
    ("Ctrl", k(0x1D)),
    ("A", k(0x1E)),
    ("S", k(0x1F)),
    ("D", k(0x20)),
    ("F", k(0x21)),
    ("G", k(0x22)),
    ("H", k(0x23)),
    ("J", k(0x24)),
    ("K", k(0x25)),
    ("L", k(0x26)),
    ("Semicolon", k(0x27)),
    ("Quote", k(0x28)),
    ("Backquote", k(0x29)),
    ("LShift", k(0x2A)),
    ("Shift", k(0x2A)),
    ("Backslash", k(0x2B)),
    ("Z", k(0x2C)),
    ("X", k(0x2D)),
    ("C", k(0x2E)),
    ("V", k(0x2F)),
    ("B", k(0x30)),
    ("N", k(0x31)),
    ("M", k(0x32)),
    ("Comma", k(0x33)),
    ("Period", k(0x34)),
    ("Slash", k(0x35)),
    ("RShift", k(0x36)),
    ("LAlt", k(0x38)),
    ("Alt", k(0x38)),
    ("Space", k(0x39)),
    ("CapsLock", k(0x3A)),
    ("F1", k(0x3B)),
    ("F2", k(0x3C)),
    ("F3", k(0x3D)),
    ("F4", k(0x3E)),
    ("F5", k(0x3F)),
    ("F6", k(0x40)),
    ("F7", k(0x41)),
    ("F8", k(0x42)),
    ("F9", k(0x43)),
    ("F10", k(0x44)),
    ("F11", k(0x57)),
    ("F12", k(0x58)),
    ("RCtrl", e(0x1D)),
    ("RAlt", e(0x38)),
    ("Home", e(0x47)),
    ("Up", e(0x48)),
    ("PageUp", e(0x49)),
    ("Left", e(0x4B)),
    ("Right", e(0x4D)),
    ("End", e(0x4F)),
    ("Down", e(0x50)),
    ("PageDown", e(0x51)),
    ("Insert", e(0x52)),
    ("Delete", e(0x53)),
    ("MouseLeft", Output::Mouse(MouseButton::Left)),
    ("LMouse", Output::Mouse(MouseButton::Left)),
    ("MouseRight", Output::Mouse(MouseButton::Right)),
    ("RMouse", Output::Mouse(MouseButton::Right)),
    ("MouseMiddle", Output::Mouse(MouseButton::Middle)),
    ("MMouse", Output::Mouse(MouseButton::Middle)),
    ("MouseBack", Output::Mouse(MouseButton::Back)),
    ("BMouse", Output::Mouse(MouseButton::Back)),
    ("MouseForward", Output::Mouse(MouseButton::Forward)),
    ("FMouse", Output::Mouse(MouseButton::Forward)),
    ("WheelUp", Output::WheelUp),
    ("ScrollUp", Output::WheelUp),
    ("WheelDown", Output::WheelDown),
    ("ScrollDown", Output::WheelDown),
    ("ToggleMovement", Output::Action(Action::ToggleMovement)),
];

impl Output {
    pub fn parse(name: &str) -> Option<Self> {
        NAMES.iter().find(|(n, _)| n.eq_ignore_ascii_case(name.trim())).map(|(_, o)| *o)
    }

    /// Canonical name (first listed alias).
    pub fn name(self) -> &'static str {
        NAMES.iter().find(|(_, o)| *o == self).map_or("?", |(n, _)| n)
    }

    /// Wheel notches have no "up" event: they fire once on press.
    pub fn is_pulse(self) -> bool {
        matches!(self, Output::WheelUp | Output::WheelDown | Output::Action(_))
    }
}

impl fmt::Display for Output {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_and_aliases() {
        assert_eq!(Output::parse("lctrl"), Output::parse("LControl"));
        assert_eq!(Output::parse("Space"), Some(Output::Key { scan: 0x39, extended: false }));
        assert_eq!(Output::parse("Up"), Some(Output::Key { scan: 0x48, extended: true }));
        assert_eq!(Output::parse("LMOUSE"), Some(Output::Mouse(MouseButton::Left)));
        assert_eq!(Output::parse("nope"), None);
        for (n, o) in NAMES {
            assert_eq!(Output::parse(n), Some(*o));
        }
    }
}
