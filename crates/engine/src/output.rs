//! Keyboard and mouse output. `SendInput` with scan codes, so games on DirectInput see the keys.

use std::collections::HashMap;

use crate::keys::{MouseButton, Output};

pub trait Sink {
    fn key(&mut self, out: Output, down: bool);
    /// Relative mouse movement in counts.
    fn mouse_move(&mut self, dx: i32, dy: i32);
}

/// Several sources may hold the same key (left stick and d-pad both on W): the key goes up only
/// when the last source releases it.
#[derive(Default)]
pub struct Held {
    count: HashMap<Output, u32>,
}

impl Held {
    pub fn down(&mut self, out: Output, sink: &mut dyn Sink) {
        if out.is_pulse() {
            sink.key(out, true);
            return;
        }
        let c = self.count.entry(out).or_insert(0);
        *c += 1;
        if *c == 1 {
            sink.key(out, true);
        }
    }

    pub fn up(&mut self, out: Output, sink: &mut dyn Sink) {
        if out.is_pulse() {
            return;
        }
        if let Some(c) = self.count.get_mut(&out) {
            *c -= 1;
            if *c == 0 {
                self.count.remove(&out);
                sink.key(out, false);
            }
        }
    }

    pub fn release_all(&mut self, sink: &mut dyn Sink) {
        for (out, _) in self.count.drain() {
            sink.key(out, false);
        }
    }

    pub fn held(&self) -> impl Iterator<Item = Output> + '_ {
        self.count.keys().copied()
    }
}

/// Sends input to the focused window.
#[derive(Default)]
pub struct WinSink;

impl Sink for WinSink {
    fn key(&mut self, out: Output, down: bool) {
        use windows::Win32::UI::Input::KeyboardAndMouse::*;
        let input = match out {
            Output::Key { scan, extended } => {
                let mut flags = KEYEVENTF_SCANCODE;
                if extended {
                    flags |= KEYEVENTF_EXTENDEDKEY;
                }
                if !down {
                    flags |= KEYEVENTF_KEYUP;
                }
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT { wVk: VIRTUAL_KEY(0), wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: 0 },
                    },
                }
            }
            Output::Mouse(b) => {
                let (flags, data) = match (b, down) {
                    (MouseButton::Left, true) => (MOUSEEVENTF_LEFTDOWN, 0),
                    (MouseButton::Left, false) => (MOUSEEVENTF_LEFTUP, 0),
                    (MouseButton::Right, true) => (MOUSEEVENTF_RIGHTDOWN, 0),
                    (MouseButton::Right, false) => (MOUSEEVENTF_RIGHTUP, 0),
                    (MouseButton::Middle, true) => (MOUSEEVENTF_MIDDLEDOWN, 0),
                    (MouseButton::Middle, false) => (MOUSEEVENTF_MIDDLEUP, 0),
                    (MouseButton::Back, true) => (MOUSEEVENTF_XDOWN, 1),
                    (MouseButton::Back, false) => (MOUSEEVENTF_XUP, 1),
                    (MouseButton::Forward, true) => (MOUSEEVENTF_XDOWN, 2),
                    (MouseButton::Forward, false) => (MOUSEEVENTF_XUP, 2),
                };
                mouse_input(flags, 0, 0, data)
            }
            Output::WheelUp | Output::WheelDown if down => {
                let delta: i32 = if out == Output::WheelUp { 120 } else { -120 };
                mouse_input(MOUSEEVENTF_WHEEL, 0, 0, delta as u32)
            }
            Output::WheelUp | Output::WheelDown => return,
        };
        send(&[input]);
    }

    fn mouse_move(&mut self, dx: i32, dy: i32) {
        use windows::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_MOVE;
        if dx != 0 || dy != 0 {
            send(&[mouse_input(MOUSEEVENTF_MOVE, dx, dy, 0)]);
        }
    }
}

fn mouse_input(
    flags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS,
    dx: i32,
    dy: i32,
    data: u32,
) -> windows::Win32::UI::Input::KeyboardAndMouse::INPUT {
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 { mi: MOUSEINPUT { dx, dy, mouseData: data, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    }
}

fn send(inputs: &[windows::Win32::UI::Input::KeyboardAndMouse::INPUT]) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT};
    // SAFETY: the slice holds fully initialised INPUT structs of the size passed along.
    unsafe {
        SendInput(inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

/// Records events instead of sending them (tests, dry runs).
#[derive(Default, Debug)]
pub struct RecordSink {
    pub events: Vec<(Output, bool)>,
    pub moved: (i64, i64),
}

impl Sink for RecordSink {
    fn key(&mut self, out: Output, down: bool) {
        self.events.push((out, down));
    }

    fn mouse_move(&mut self, dx: i32, dy: i32) {
        self.moved.0 += dx as i64;
        self.moved.1 += dy as i64;
    }
}
