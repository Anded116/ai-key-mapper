use std::collections::BTreeSet;

use dualsense::{Buttons as PadButtons, State};

use crate::buttons::Buttons;
use crate::flick::{apply_deadzone, Flick};
use crate::gyro::{CalibrationResult, Gyro, TRIM_S};
use crate::keys::Output;
use crate::output::{Held, Sink};
use crate::profile::{Btn, MoveMode, Profile};

/// Digital buttons. L2/R2 are left out: like JSM they come from the analog position.
const PAD_BITS: [(PadButtons, Btn); 17] = [
    (PadButtons::L1, Btn::L1),
    (PadButtons::R1, Btn::R1),
    (PadButtons::L3, Btn::L3),
    (PadButtons::R3, Btn::R3),
    (PadButtons::CROSS, Btn::Cross),
    (PadButtons::CIRCLE, Btn::Circle),
    (PadButtons::SQUARE, Btn::Square),
    (PadButtons::TRIANGLE, Btn::Triangle),
    (PadButtons::UP, Btn::Up),
    (PadButtons::DOWN, Btn::Down),
    (PadButtons::LEFT, Btn::Left),
    (PadButtons::RIGHT, Btn::Right),
    (PadButtons::CREATE, Btn::Create),
    (PadButtons::OPTIONS, Btn::Options),
    (PadButtons::PS, Btn::Ps),
    (PadButtons::TOUCHPAD, Btn::Touchpad),
    (PadButtons::MIC, Btn::Mic),
];

/// Half of the touchpad width in touch units (DualSense and DS4 are both 1920 wide).
const PAD_HALF_X: u16 = 960;
/// A calibrate press shorter than this toggles calibration instead of holding it.
const CAL_TAP_S: f64 = 0.3;

/// What the UI shows about the engine.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub inputs: Vec<Btn>,
    pub outputs: Vec<Output>,
    pub layer: Option<Btn>,
    pub gyro_calibrating: bool,
    pub gyro_bias: [f32; 3],
    pub flick_active: bool,
    /// Paired button (in the weapon layer) whose second slot the next press gives.
    pub next_pair: Option<Btn>,
}

pub struct Engine {
    pub profile: Profile,
    buttons: Buttons,
    held: Held,
    gyro: Gyro,
    flick: Flick,
    pressed: BTreeSet<Btn>,
    move_held: [bool; 4],
    walk_held: bool,
    sprint_held: bool,
    frac: (f32, f32),
    last_now: Option<f64>,
    last_ts: Option<u32>,
    cal_press: Option<f64>,
    cal_toggle: bool,
    cal_ending_press: bool,
}

impl Engine {
    pub fn new(profile: Profile) -> Self {
        Self {
            profile,
            buttons: Buttons::default(),
            held: Held::default(),
            gyro: Gyro::default(),
            flick: Flick::default(),
            pressed: BTreeSet::new(),
            move_held: [false; 4],
            walk_held: false,
            sprint_held: false,
            frac: (0.0, 0.0),
            last_now: None,
            last_ts: None,
            cal_press: None,
            cal_toggle: false,
            cal_ending_press: false,
        }
    }

    /// Swaps the profile; everything held is released first. Gyro calibration is kept.
    pub fn set_profile(&mut self, profile: Profile, sink: &mut dyn Sink) {
        self.release_all(sink);
        self.profile = profile;
    }

    pub fn release_all(&mut self, sink: &mut dyn Sink) {
        self.held.release_all(sink);
        self.buttons.reset();
        self.pressed.clear();
        self.move_held = [false; 4];
        self.walk_held = false;
        self.sprint_held = false;
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            inputs: self.pressed.iter().copied().collect(),
            outputs: self.held.held().collect(),
            layer: self.buttons.active_layer(),
            gyro_calibrating: self.gyro.calibrating(),
            gyro_bias: self.gyro.bias(),
            flick_active: self.flick.active(),
            next_pair: self.profile.layers.iter().find_map(|l| self.buttons.next_pair(Some(l.button))),
        }
    }

    fn inputs(s: &State, trigger_threshold: f32) -> BTreeSet<Btn> {
        let mut set: BTreeSet<Btn> = PAD_BITS.iter().filter(|(b, _)| s.buttons.contains(*b)).map(|(_, x)| *x).collect();
        if s.l2 > trigger_threshold {
            set.insert(Btn::L2);
        }
        if s.r2 > trigger_threshold {
            set.insert(Btn::R2);
        }
        let finger = s.touch.iter().find(|t| t.active);
        if finger.is_some() {
            set.insert(Btn::Touch);
        }
        if s.buttons.contains(PadButtons::TOUCHPAD) {
            let right = finger.is_some_and(|t| t.x >= PAD_HALF_X);
            set.insert(if right { Btn::PadRight } else { Btn::PadLeft });
        }
        set
    }

    /// Calibration button as in JSM: a tap starts calibrating and the next tap finishes it;
    /// a hold calibrates while held. Either way the jolts of pressing and releasing are trimmed.
    fn calibrate_pressed(&mut self, now: f64) {
        if self.gyro.calibrating() && self.cal_toggle {
            self.gyro.end_calibration(now, TRIM_S, TRIM_S);
            self.cal_toggle = false;
            self.cal_ending_press = true;
        } else {
            self.gyro.begin_calibration(now);
            self.cal_press = Some(now);
        }
    }

    fn calibrate_released(&mut self, now: f64) {
        if std::mem::take(&mut self.cal_ending_press) {
            return;
        }
        if let Some(t) = self.cal_press.take() {
            if now - t < CAL_TAP_S {
                self.cal_toggle = true;
            } else {
                self.gyro.end_calibration(now, TRIM_S, TRIM_S);
            }
        }
    }

    /// Result of the last finished gyro calibration, once.
    pub fn take_calibration(&mut self) -> Option<CalibrationResult> {
        self.gyro.take_result()
    }

    /// Time without a new report: only timers run (tap releases, holds, pending taps).
    pub fn tick(&mut self, now: f64, sink: &mut dyn Sink) {
        self.buttons.tick(now, &self.profile, &mut self.held, sink);
    }

    pub fn update(&mut self, now: f64, s: &State, sink: &mut dyn Sink) {
        let dt = self.dt(now, s.timestamp_us);
        self.buttons.tick(now, &self.profile, &mut self.held, sink);

        let current = Self::inputs(s, self.profile.trigger_threshold);
        let released: Vec<Btn> = self.pressed.difference(&current).copied().collect();
        let mut pressed: Vec<Btn> = current.difference(&self.pressed).copied().collect();
        // Layer buttons first, so R2 and □ landing in one report still select the weapon.
        pressed.sort_by_key(|b| !self.profile.layers.iter().any(|l| l.button == *b));
        let calibrate = self.profile.gyro.calibrate;
        for b in released {
            self.buttons.release(b, now, &self.profile, &mut self.held, sink);
            if Some(b) == calibrate {
                self.calibrate_released(now);
            }
        }
        for b in pressed {
            self.buttons.press(b, now, &self.profile, &mut self.held, sink);
            if Some(b) == calibrate {
                self.calibrate_pressed(now);
            }
        }
        self.pressed = current;

        self.left_stick(s.left, sink);

        let gyro_on = self.profile.gyro.off.is_none_or(|b| !self.pressed.contains(&b));
        let (gx, gy) = self.gyro.update(now, dt, s.gyro, &self.profile.gyro, gyro_on);
        let fx = self.flick.update(dt, s.right, &self.profile.flick);
        let k = self.profile.calibration.counts_per_degree();
        self.frac.0 += (gx + fx) * k;
        self.frac.1 += gy * k;
        let (dx, dy) = (self.frac.0.trunc(), self.frac.1.trunc());
        self.frac.0 -= dx;
        self.frac.1 -= dy;
        sink.mouse_move(dx as i32, dy as i32);
    }

    /// Prefers the controller clock: host timing jitters with USB/Bluetooth batching.
    fn dt(&mut self, now: f64, ts: u32) -> f32 {
        let host = self.last_now.map_or(0.0, |t| (now - t) as f32);
        let pad = self.last_ts.map(|t| ts.wrapping_sub(t) as f32 / 1_000_000.0);
        self.last_now = Some(now);
        self.last_ts = Some(ts);
        match pad {
            Some(d) if d > 0.0 && d < 0.05 => d,
            _ => host.clamp(0.0, 0.05),
        }
    }

    /// JSM `NO_MOUSE` stick: a direction is held when its axis exceeds half of the other axis
    /// (±63.4° sectors, diagonals press two keys); rings split at a rescaled length of `ring`.
    fn left_stick(&mut self, raw: [f32; 2], sink: &mut dyn Sink) {
        let m = &self.profile.movement;
        if m.mode != MoveMode::Wasd {
            return;
        }
        let ([x, y], len) = apply_deadzone(raw, m.deadzone);
        let want = [y > 0.5 * x.abs(), x < -0.5 * y.abs(), y < -0.5 * x.abs(), x > 0.5 * y.abs()];
        let (keys, walk, sprint, ring) = (m.keys, m.walk, m.sprint, m.ring);
        for i in 0..4 {
            set(&mut self.held, &mut self.move_held[i], want[i], keys[i], sink);
        }
        if let Some(k) = walk {
            set(&mut self.held, &mut self.walk_held, len > 0.0 && len < ring, k, sink);
        }
        if let Some(k) = sprint {
            set(&mut self.held, &mut self.sprint_held, len > ring, k, sink);
        }
    }
}

fn set(held: &mut Held, state: &mut bool, want: bool, key: Output, sink: &mut dyn Sink) {
    if want != *state {
        *state = want;
        if want {
            held.down(key, sink);
        } else {
            held.up(key, sink);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::RecordSink;
    use dualsense::Touch;

    fn state(buttons: PadButtons, ts: u32) -> State {
        State { buttons, timestamp_us: ts, ..State::default() }
    }

    fn names(sink: &mut RecordSink) -> Vec<String> {
        sink.events.drain(..).map(|(o, d)| format!("{}{}", if d { "+" } else { "-" }, o)).collect()
    }

    #[test]
    fn layer_and_face_in_one_report() {
        let p = Profile::parse("[weapons]\nslots = 4\nkeys = [\"1\",\"2\",\"3\",\"4\"]").unwrap();
        let mut e = Engine::new(p);
        let mut sink = RecordSink::default();
        let mut s = state(PadButtons::CIRCLE, 0);
        s.r2 = 0.3;
        e.update(0.0, &s, &mut sink);
        assert_eq!(names(&mut sink), ["+3"]);
    }

    #[test]
    fn touchpad_halves() {
        let p = Profile::parse("[buttons]\npad_left = \"F1\"\npad_right = \"F2\"").unwrap();
        let mut e = Engine::new(p);
        let mut sink = RecordSink::default();
        let mut s = state(PadButtons::TOUCHPAD, 0);
        s.touch[0] = Touch { active: true, id: 1, x: 1500, y: 300 };
        e.update(0.0, &s, &mut sink);
        e.update(0.01, &state(PadButtons::NONE, 10_000), &mut sink);
        assert_eq!(names(&mut sink), ["+F2", "-F2"]);
    }

    #[test]
    fn wasd_with_walk_ring() {
        let p = Profile::parse("[movement]\nmode = \"wasd\"\nwalk = \"LAlt\"").unwrap();
        let mut e = Engine::new(p);
        let mut sink = RecordSink::default();
        let mut s = state(PadButtons::NONE, 0);
        s.left = [0.0, 0.5];
        e.update(0.0, &s, &mut sink);
        assert_eq!(names(&mut sink), ["+W", "+LAlt"]);
        s.left = [0.7, 0.7];
        e.update(0.01, &s, &mut sink);
        assert_eq!(names(&mut sink), ["+D", "-LAlt"]);
        s.left = [0.0, 0.0];
        e.update(0.02, &s, &mut sink);
        assert_eq!(names(&mut sink), ["-W", "-D"]);
    }

    #[test]
    fn gyro_turn_becomes_mouse_counts() {
        let p = Profile::parse(
            "[calibration]\nreal_world = 45.4545\nin_game_sens = 1.0\n[gyro]\nmin_sens = 1.0\nmax_sens = 1.0\nstartup_calibration = 0.0",
        )
        .unwrap();
        let mut e = Engine::new(p);
        let mut sink = RecordSink::default();
        // Turn right at 90 deg/s (yaw negative) for one second of 2 ms reports.
        for i in 0..=500u32 {
            let mut s = state(PadButtons::NONE, i * 2000);
            s.gyro = [0.0, -90.0, 0.0];
            e.update(i as f64 * 0.002, &s, &mut sink);
        }
        let expected = 90.0 * 45.4545;
        assert!((sink.moved.0 as f32 - expected).abs() < 5.0, "{:?}", sink.moved);
        assert_eq!(sink.moved.1, 0);
    }

    fn feed(e: &mut Engine, sink: &mut RecordSink, from: f64, to: f64, buttons: PadButtons, gyro: [f32; 3]) {
        let mut t = from;
        while t < to {
            let mut s = state(buttons, (t * 1_000_000.0) as u32);
            s.gyro = gyro;
            e.update(t, &s, sink);
            t += 0.002;
        }
    }

    #[test]
    fn calibrate_tap_toggles_and_hold_holds() {
        let p = Profile::parse("[gyro]\nstartup_calibration = 0.0").unwrap();
        let mut e = Engine::new(p);
        let mut sink = RecordSink::default();
        let bias = [0.3, -0.5, 0.1];
        // Tap PS: calibration keeps running after the release.
        feed(&mut e, &mut sink, 0.0, 0.1, PadButtons::PS, bias);
        feed(&mut e, &mut sink, 0.1, 2.0, PadButtons::NONE, bias);
        assert!(e.snapshot().gyro_calibrating);
        // Second tap finishes it.
        feed(&mut e, &mut sink, 2.0, 2.1, PadButtons::PS, bias);
        assert!(!e.snapshot().gyro_calibrating);
        feed(&mut e, &mut sink, 2.1, 2.2, PadButtons::NONE, bias);
        assert!(!e.snapshot().gyro_calibrating, "the finishing tap's release starts nothing");
        let r = e.take_calibration().unwrap();
        let b = r.bias.unwrap();
        assert!((b[0] - 0.3).abs() < 1e-4 && (b[1] + 0.5).abs() < 1e-4, "{b:?}");
        // Hold PS for two seconds: calibrates until release.
        feed(&mut e, &mut sink, 3.0, 5.0, PadButtons::PS, [0.0, 0.2, 0.0]);
        assert!(e.snapshot().gyro_calibrating);
        feed(&mut e, &mut sink, 5.0, 5.1, PadButtons::NONE, [0.0, 0.2, 0.0]);
        assert!(!e.snapshot().gyro_calibrating);
        assert!((e.snapshot().gyro_bias[1] - 0.2).abs() < 1e-4);
    }

    #[test]
    fn touch_turns_gyro_off() {
        let p = Profile::parse("[gyro]\nstartup_calibration = 0.0").unwrap();
        let mut e = Engine::new(p);
        let mut sink = RecordSink::default();
        for i in 0..100u32 {
            let mut s = state(PadButtons::NONE, i * 2000);
            s.gyro = [0.0, 50.0, 0.0];
            s.touch[0] = Touch { active: true, id: 0, x: 100, y: 100 };
            e.update(i as f64 * 0.002, &s, &mut sink);
        }
        assert_eq!(sink.moved, (0, 0));
    }
}
