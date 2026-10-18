//! Button bindings: press, tap/hold, double press and held layers.

use std::collections::HashMap;

use crate::keys::Output;
use crate::output::{Held, Sink};
use crate::profile::{Binding, Btn, Profile};

/// A press in progress. The binding is resolved once, at press time (as JSM does): releasing
/// the layer button first must not change what this button lets go of.
struct Active {
    binding: Binding,
    layer: Option<Btn>,
    started: f64,
    /// Keys that went down on press and go up on release (press, next or double).
    immediate: Vec<Output>,
    hold_fired: bool,
}

#[derive(Default)]
pub struct Buttons {
    active: HashMap<Btn, Active>,
    /// Held layer buttons, most recent last.
    layers: Vec<Btn>,
    /// First-press time per (button, layer) for `press` + `double`.
    first_press: HashMap<(Btn, Option<Btn>), f64>,
    /// Key of the last weapon picked through a pair (press or next).
    last_pair: Option<Output>,
    /// Per layer: the paired button whose `next` the following press gives.
    toggle: HashMap<Option<Btn>, Btn>,
    /// Taps waiting out the double-press window: fire time and keys.
    pending_tap: HashMap<(Btn, Option<Btn>), (f64, Vec<Output>)>,
    /// Key-ups of taps, due at the given time.
    scheduled_up: Vec<(f64, Output)>,
}

impl Buttons {
    pub fn active_layer(&self) -> Option<Btn> {
        self.layers.last().copied()
    }

    /// The paired button whose second weapon the next press in `layer` gives.
    pub fn last_weapon(&self) -> Option<Output> {
        self.last_pair
    }

    pub fn next_pair(&self, layer: Option<Btn>) -> Option<Btn> {
        self.toggle.get(&layer).copied()
    }

    fn resolve(&self, b: Btn, p: &Profile) -> (Option<Btn>, Option<Binding>) {
        for layer_btn in self.layers.iter().rev().filter(|l| **l != b) {
            if let Some(layer) = p.layers.iter().find(|l| l.button == *layer_btn) {
                if let Some(binding) = layer.bindings.get(&b) {
                    return (Some(*layer_btn), Some(binding.clone()));
                }
            }
        }
        (None, p.buttons.get(&b).cloned())
    }

    pub fn press(&mut self, b: Btn, now: f64, p: &Profile, held: &mut Held, sink: &mut dyn Sink) {
        if p.layers.iter().any(|l| l.button == b) {
            self.layers.push(b);
        }
        let (layer, Some(binding)) = self.resolve(b, p) else { return };
        let key = (b, layer);
        let window = p.timing.double_ms as f64 / 1000.0;
        let mut immediate = binding.press.clone();
        if !binding.next.is_empty() {
            if self.toggle.get(&layer) == Some(&b) {
                immediate = binding.next.clone();
                self.toggle.remove(&layer);
            } else {
                self.toggle.insert(layer, b);
            }
        } else if !binding.double.is_empty() {
            if !binding.press.is_empty() {
                match self.first_press.get(&key) {
                    Some(t) if now - t <= window => {
                        immediate = binding.double.clone();
                        self.first_press.remove(&key);
                    }
                    _ => {
                        self.first_press.insert(key, now);
                    }
                }
            } else if self.pending_tap.remove(&key).is_some() {
                immediate = binding.double.clone();
            }
        }
        for o in &immediate {
            held.down(*o, sink);
        }
        if !binding.next.is_empty() {
            self.last_pair = immediate.first().copied();
        }
        self.active.insert(b, Active { binding, layer, started: now, immediate, hold_fired: false });
    }

    pub fn release(&mut self, b: Btn, now: f64, p: &Profile, held: &mut Held, sink: &mut dyn Sink) {
        self.layers.retain(|l| *l != b);
        let Some(a) = self.active.remove(&b) else { return };
        if !a.immediate.is_empty() {
            for o in &a.immediate {
                held.up(*o, sink);
            }
            return;
        }
        if a.hold_fired {
            for o in &a.binding.hold {
                held.up(*o, sink);
            }
            return;
        }
        if a.binding.tap.is_empty() {
            return;
        }
        if a.binding.double.is_empty() {
            self.pulse(&a.binding.tap, now, p, held, sink);
        } else {
            let fire_at = a.started + p.timing.double_ms as f64 / 1000.0;
            self.pending_tap.insert((b, a.layer), (fire_at.max(now), a.binding.tap.clone()));
        }
    }

    fn pulse(&mut self, outs: &[Output], now: f64, p: &Profile, held: &mut Held, sink: &mut dyn Sink) {
        for o in outs {
            held.down(*o, sink);
            self.scheduled_up.push((now + p.timing.tap_ms as f64 / 1000.0, *o));
        }
    }

    pub fn tick(&mut self, now: f64, p: &Profile, held: &mut Held, sink: &mut dyn Sink) {
        let hold_after = p.timing.hold_ms as f64 / 1000.0;
        for a in self.active.values_mut() {
            let wants_hold = a.immediate.is_empty() && !a.binding.hold.is_empty();
            if wants_hold && !a.hold_fired && now - a.started >= hold_after {
                a.hold_fired = true;
                for o in &a.binding.hold {
                    held.down(*o, sink);
                }
            }
        }
        let due: Vec<_> = self.pending_tap.iter().filter(|(_, (t, _))| *t <= now).map(|(k, _)| *k).collect();
        for k in due {
            if let Some((_, outs)) = self.pending_tap.remove(&k) {
                self.pulse(&outs, now, p, held, sink);
            }
        }
        let mut i = 0;
        while i < self.scheduled_up.len() {
            if self.scheduled_up[i].0 <= now {
                let (_, o) = self.scheduled_up.swap_remove(i);
                held.up(o, sink);
            } else {
                i += 1;
            }
        }
        let window = p.timing.double_ms as f64 / 1000.0;
        self.first_press.retain(|_, t| now - *t <= window);
    }

    /// Forget all presses (profile switch). Keys are released by the caller through `Held`.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::RecordSink;

    struct Rig {
        p: Profile,
        b: Buttons,
        held: Held,
        sink: RecordSink,
    }

    impl Rig {
        fn new(toml: &str) -> Self {
            Self { p: Profile::parse(toml).unwrap(), b: Buttons::default(), held: Held::default(), sink: RecordSink::default() }
        }
        fn press(&mut self, b: Btn, t: f64) {
            self.b.tick(t, &self.p, &mut self.held, &mut self.sink);
            self.b.press(b, t, &self.p, &mut self.held, &mut self.sink);
        }
        fn release(&mut self, b: Btn, t: f64) {
            self.b.tick(t, &self.p, &mut self.held, &mut self.sink);
            self.b.release(b, t, &self.p, &mut self.held, &mut self.sink);
        }
        fn tick(&mut self, t: f64) {
            self.b.tick(t, &self.p, &mut self.held, &mut self.sink);
        }
        fn events(&mut self) -> Vec<String> {
            self.sink.events.drain(..).map(|(o, d)| format!("{}{}", if d { "+" } else { "-" }, o)).collect()
        }
    }

    #[test]
    fn press_follows_button() {
        let mut r = Rig::new("[buttons]\nl1 = \"Space\"");
        r.press(Btn::L1, 0.0);
        r.release(Btn::L1, 0.5);
        assert_eq!(r.events(), ["+Space", "-Space"]);
    }

    #[test]
    fn tap_and_hold() {
        let mut r = Rig::new("[buttons]\noptions = { tap = \"Esc\", hold = \"M\" }");
        r.press(Btn::Options, 0.0);
        r.release(Btn::Options, 0.05);
        r.tick(0.2);
        assert_eq!(r.events(), ["+Esc", "-Esc"]);
        r.press(Btn::Options, 1.0);
        r.tick(1.2);
        assert_eq!(r.events(), ["+M"]);
        r.release(Btn::Options, 2.0);
        assert_eq!(r.events(), ["-M"]);
    }

    #[test]
    fn weapon_pairs_toggle_like_jsm_state_files() {
        let mut r = Rig::new("[weapons]\nslots = 8\nkeys = [\"1\",\"2\",\"3\",\"4\",\"5\",\"6\",\"7\",\"8\"]");
        r.press(Btn::R2, 0.0);
        r.press(Btn::Square, 0.1);
        assert_eq!(r.events(), ["+1"], "first press acts at once");
        r.release(Btn::Square, 0.15);
        r.press(Btn::Square, 3.0);
        r.release(Btn::Square, 3.05);
        assert_eq!(r.events(), ["-1", "+2", "-2"], "same button again gives the pair's second slot, any time later");
        r.press(Btn::Square, 4.0);
        r.release(Btn::Square, 4.05);
        assert_eq!(r.events(), ["+1", "-1"], "and then the first again");
        r.press(Btn::Circle, 5.0);
        r.release(Btn::Circle, 5.05);
        r.press(Btn::Square, 6.0);
        r.release(Btn::Square, 6.05);
        assert_eq!(r.events(), ["+5", "-5", "+1", "-1"], "another button starts its pair over");
        r.release(Btn::R2, 7.0);
        r.press(Btn::Square, 7.1);
        assert_eq!(r.events(), Vec::<String>::new(), "without R2 the square has no binding here");
    }

    #[test]
    fn layer_binding_is_kept_until_release() {
        let mut r = Rig::new("[buttons]\nsquare = \"R\"\n[layer.r2]\nsquare = \"1\"");
        r.press(Btn::R2, 0.0);
        r.press(Btn::Square, 0.1);
        r.release(Btn::R2, 0.2);
        r.release(Btn::Square, 0.3);
        r.press(Btn::Square, 0.4);
        assert_eq!(r.events(), ["+1", "-1", "+R"]);
    }

    #[test]
    fn tap_with_double_waits_for_the_window() {
        let mut r = Rig::new("[buttons]\ncircle = { tap = \"C\", double = \"Z\" }");
        r.press(Btn::Circle, 0.0);
        r.release(Btn::Circle, 0.05);
        r.tick(0.1);
        assert!(r.events().is_empty(), "tap is held back while a double press is possible");
        r.tick(0.3);
        assert_eq!(r.events(), ["+C"]);
        r.tick(0.4);
        r.events();
        r.press(Btn::Circle, 1.0);
        r.release(Btn::Circle, 1.05);
        r.press(Btn::Circle, 1.1);
        r.release(Btn::Circle, 1.15);
        r.tick(2.0);
        assert_eq!(r.events(), ["+Z", "-Z"], "double replaces the tap");
    }

    #[test]
    fn shared_key_released_by_last_source() {
        let mut r = Rig::new("[buttons]\ndpad_up = \"W\"\nl1 = \"W\"");
        r.press(Btn::Up, 0.0);
        r.press(Btn::L1, 0.1);
        r.release(Btn::Up, 0.2);
        assert_eq!(r.events(), ["+W"]);
        r.release(Btn::L1, 0.3);
        assert_eq!(r.events(), ["-W"]);
    }
}
