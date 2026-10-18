use std::collections::BTreeSet;

use dualsense::{Buttons as PadButtons, State};

use crate::buttons::Buttons;
use crate::flick::{apply_deadzone, Flick};
use crate::gyro::{CalibrationResult, Gyro, TRIM_S};
use crate::keys::{Action, Output};
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
/// Half of the DualSense touchpad height (1080; the DS4 pad is 942 tall, its middle a little higher).
const PAD_HALF_Y: u16 = 540;
/// A calibrate press shorter than this toggles calibration instead of holding it.
const CAL_TAP_S: f64 = 0.3;
/// Calibration turn speed, deg/s: a full circle in half a second, quick but not a flick.
const TURN_SPEED: f32 = 720.0;

/// Virtual Xbox pad report in vehicle mode: stick -1..1 with +Y up, triggers 0..1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VirtualPad {
    pub left: [f32; 2],
    pub left_trigger: f32,
    pub right_trigger: f32,
}

/// What the UI shows about the engine.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub inputs: Vec<Btn>,
    pub outputs: Vec<Output>,
    pub layer: Option<Btn>,
    pub gyro_calibrating: bool,
    pub gyro_bias: [f32; 3],
    pub flick_active: bool,
    /// A calibration turn is under way.
    pub turning: bool,
    /// Vehicle mode is on.
    pub vehicle: bool,
    /// Paired button (in the weapon layer) whose second slot the next press gives.
    pub next_pair: Option<Btn>,
    pub movement: MoveMode,
    /// Key of the last weapon chosen through an R2 pair.
    pub weapon: Option<Output>,
}


/// Passes keys on and catches engine commands, which never reach the system.
struct ActionTap<'a> {
    inner: &'a mut dyn Sink,
    actions: Vec<Action>,
}

impl Sink for ActionTap<'_> {
    fn key(&mut self, out: Output, down: bool) {
        match out {
            Output::Action(a) if down => self.actions.push(a),
            Output::Action(_) => {}
            _ => self.inner.key(out, down),
        }
    }

    fn mouse_move(&mut self, dx: i32, dy: i32) {
        self.inner.mouse_move(dx, dy);
    }
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
    /// Movement mode switched at runtime (ToggleMovement); `None` = the profile's.
    movement_override: Option<MoveMode>,
    /// Degrees of the calibration turn still to send.
    turn_left: f32,
    /// Vehicle mode (ToggleVehicle): left stick and triggers go to the virtual pad.
    vehicle: bool,
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
            movement_override: None,
            turn_left: 0.0,
            vehicle: false,
        }
    }

    /// Swaps the profile; everything held is released first. Gyro calibration is kept.
    pub fn set_profile(&mut self, profile: Profile, sink: &mut dyn Sink) {
        self.release_all(sink);
        self.profile = profile;
        self.movement_override = None;
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
            turning: self.turn_left > 0.0,
            vehicle: self.vehicle,
            next_pair: self.next_pair(),
            movement: self.movement_mode(),
            weapon: self.buttons.last_weapon(),
        }
    }

    fn inputs(&self, s: &State) -> BTreeSet<Btn> {
        let trigger_threshold = self.profile.trigger_threshold;
        let mut set: BTreeSet<Btn> = PAD_BITS.iter().filter(|(b, _)| s.buttons.contains(*b)).map(|(_, x)| *x).collect();
        // in a vehicle L2 is the brake, not aim; R2 stays: it is the layer key that leaves the mode
        if s.l2 > trigger_threshold && !self.vehicle {
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
            let lower = finger.is_some_and(|t| t.y >= PAD_HALF_Y);
            let quarter = match (lower, right) {
                (false, false) => Btn::PadUpLeft,
                (false, true) => Btn::PadUpRight,
                (true, false) => Btn::PadDownLeft,
                (true, true) => Btn::PadDownRight,
            };
            let p = &self.profile;
            let bound = p.buttons.contains_key(&quarter) || p.layers.iter().any(|l| l.bindings.contains_key(&quarter));
            set.insert(if bound { quarter } else if right { Btn::PadRight } else { Btn::PadLeft });
        }
        set
    }

    pub fn movement_mode(&self) -> MoveMode {
        self.movement_override.unwrap_or(self.profile.movement.mode)
    }

    /// The controller should be hidden from games: the profile asks for it and the left stick is
    /// mapped to WASD. In hybrid mode the game has to read the stick, so it must see the pad.
    pub fn wants_hidden(&self) -> bool {
        self.profile.hide_controller && self.movement_mode() == MoveMode::Wasd
    }

    fn next_pair(&self) -> Option<Btn> {
        self.profile.layers.iter().find_map(|l| self.buttons.next_pair(Some(l.button)))
    }

    /// Light bar colour the profile asks for right now, if it drives the light bar at all.
    pub fn lightbar(&self) -> Option<[u8; 3]> {
        let lb = &self.profile.lightbar;
        if !lb.enabled {
            return None;
        }
        let pair = self.next_pair().and_then(|b| lb.pairs.iter().find(|(p, _)| *p == b).map(|(_, c)| *c));
        Some(pair.unwrap_or(lb.idle))
    }

    fn apply(&mut self, actions: Vec<Action>, sink: &mut dyn Sink) {
        for a in actions {
            match a {
                Action::ToggleMovement => {
                    let next = match self.movement_mode() {
                        MoveMode::Hybrid => MoveMode::Wasd,
                        MoveMode::Wasd => MoveMode::Hybrid,
                    };
                    self.movement_override = Some(next);
                    self.release_movement(sink);
                }
                Action::ToggleVehicle => {
                    self.vehicle = !self.vehicle;
                    self.release_movement(sink);
                }
            }
        }
    }

    /// Leaving WASD (or entering a vehicle) must not leave a movement key held.
    fn release_movement(&mut self, sink: &mut dyn Sink) {
        let keys = self.profile.movement.keys;
        for (i, key) in keys.iter().enumerate() {
            set(&mut self.held, &mut self.move_held[i], false, *key, sink);
        }
        if let Some(k) = self.profile.movement.walk {
            set(&mut self.held, &mut self.walk_held, false, k, sink);
        }
        if let Some(k) = self.profile.movement.sprint {
            set(&mut self.held, &mut self.sprint_held, false, k, sink);
        }
    }

    /// What the virtual Xbox pad should report: `None` outside vehicle mode (no virtual pad at all).
    pub fn virtual_pad(&self, s: &State) -> Option<VirtualPad> {
        if !self.vehicle {
            return None;
        }
        let ([x, y], _) = apply_deadzone(s.left, self.profile.movement.deadzone);
        Some(VirtualPad { left: [x, y], left_trigger: s.l2, right_trigger: s.r2 })
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

    /// Same as a tap of the calibrate button: starts calibrating, or finishes a running one.
/// Turn the camera by exactly `degrees` as the profile's calibration sees it, over a short moment:
    /// if the game turns more or less, `real_world` is off by that ratio.
    pub fn test_turn(&mut self, degrees: f32) {
        self.turn_left = degrees.max(0.0);
    }

    pub fn toggle_calibration(&mut self, now: f64) {
        if self.gyro.calibrating() {
            self.gyro.end_calibration(now, TRIM_S, TRIM_S);
            self.cal_toggle = false;
        } else {
            self.gyro.begin_calibration(now);
            self.cal_toggle = true;
        }
    }

    /// Result of the last finished gyro calibration, once.
    pub fn take_calibration(&mut self) -> Option<CalibrationResult> {
        self.gyro.take_result()
    }

    /// The game is not in focus: nothing is sent, only gyro calibration keeps collecting, so the
    /// startup calibration happens while the pad lies still after launch, not on first focus.
    pub fn idle(&mut self, now: f64, s: &State) {
        let dt = self.dt(now, s.timestamp_us);
        self.gyro.update(now, dt, s.gyro, &self.profile.gyro, false);
    }

    /// Time without a new report: only timers run (tap releases, holds, pending taps).
    pub fn tick(&mut self, now: f64, sink: &mut dyn Sink) {
        let mut tap = ActionTap { inner: sink, actions: Vec::new() };
        self.buttons.tick(now, &self.profile, &mut self.held, &mut tap);
        let actions = std::mem::take(&mut tap.actions);
        self.apply(actions, sink);
    }

    pub fn update(&mut self, now: f64, s: &State, sink: &mut dyn Sink) {
        let dt = self.dt(now, s.timestamp_us);
        let mut tap = ActionTap { inner: sink, actions: Vec::new() };
        self.buttons.tick(now, &self.profile, &mut self.held, &mut tap);
        let actions = std::mem::take(&mut tap.actions);
        self.apply(actions, sink);

        let current = self.inputs(s);
        let released: Vec<Btn> = self.pressed.difference(&current).copied().collect();
        let mut pressed: Vec<Btn> = current.difference(&self.pressed).copied().collect();
        // Layer buttons first, so R2 and □ landing in one report still select the weapon.
        pressed.sort_by_key(|b| !self.profile.layers.iter().any(|l| l.button == *b));
        let calibrate = self.profile.gyro.calibrate;
        for b in released {
            let mut tap = ActionTap { inner: sink, actions: Vec::new() };
            self.buttons.release(b, now, &self.profile, &mut self.held, &mut tap);
            let actions = std::mem::take(&mut tap.actions);
            self.apply(actions, sink);
            if Some(b) == calibrate {
                self.calibrate_released(now);
            }
        }
        for b in pressed {
            let mut tap = ActionTap { inner: sink, actions: Vec::new() };
            self.buttons.press(b, now, &self.profile, &mut self.held, &mut tap);
            let actions = std::mem::take(&mut tap.actions);
            self.apply(actions, sink);
            if Some(b) == calibrate {
                self.calibrate_pressed(now);
            }
        }
        self.pressed = current;

        self.left_stick(s.left, sink);
        let (_, tilt) = apply_deadzone(s.left, self.profile.movement.deadzone);
        self.buttons.left_stick(tilt > 0.0, &mut self.held, sink);

        let gyro_on = self.profile.gyro.off.is_none_or(|b| !self.pressed.contains(&b));
        let (gx, gy) = self.gyro.update(now, dt, s.gyro, &self.profile.gyro, gyro_on);
        let fx = self.flick.update(dt, s.right, &self.profile.flick);
        let turn = (TURN_SPEED * dt).min(self.turn_left);
        self.turn_left -= turn;
        let k = self.profile.calibration.counts_per_degree();
        self.frac.0 += (gx + fx + turn) * k;
        if turn > 0.0 && self.turn_left <= 0.0 {
            // the turn ends on a whole count: a test circle must not come out one count short
            self.frac.0 = self.frac.0.round();
        }
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
        if self.movement_mode() != MoveMode::Wasd || self.vehicle {
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
    fn test_turn_sends_exactly_a_circle() {
        let p = Profile::parse("[calibration]\nreal_world = 10\nin_game_sens = 2\n[gyro]\nenabled = false\n[flick]\nenabled = false").unwrap();
        let mut e = Engine::new(p);
        let mut sink = RecordSink::default();
        e.update(0.0, &state(PadButtons::NONE, 0), &mut sink);
        e.test_turn(360.0);
        assert!(e.snapshot().turning);
        for i in 1..=200 {
            e.update(i as f64 * 0.005, &state(PadButtons::NONE, i * 5000), &mut sink);
        }
        assert!(!e.snapshot().turning);
        assert_eq!(sink.moved.0, 1800, "360° × 10 / 2 counts");
    }

    #[test]
    fn vehicle_mode_hands_stick_and_triggers_to_the_virtual_pad() {
        let p = Profile::parse("[movement]\nmode = \"wasd\"\n[buttons]\nl2 = \"MouseRight\"\nmic = \"ToggleVehicle\"").unwrap();
        let mut e = Engine::new(p);
        let mut sink = RecordSink::default();
        let mut s = state(PadButtons::NONE, 0);
        s.left = [0.0, 1.0];
        e.update(0.0, &s, &mut sink);
        assert_eq!(names(&mut sink), ["+W"]);
        assert!(e.virtual_pad(&s).is_none());
        let mut s = state(PadButtons::MIC, 5000);
        s.left = [0.0, 1.0];
        e.update(0.005, &s, &mut sink);
        assert_eq!(names(&mut sink), ["-W"], "entering the vehicle lets go of WASD");
        let mut s = state(PadButtons::NONE, 10_000);
        s.left = [0.0, 1.0];
        s.l2 = 0.6;
        s.r2 = 0.8;
        e.update(0.01, &s, &mut sink);
        assert_eq!(names(&mut sink), Vec::<String>::new(), "no WASD and no aim click while driving");
        let pad = e.virtual_pad(&s).unwrap();
        assert!(pad.left[1] > 0.9 && pad.left_trigger == 0.6 && pad.right_trigger == 0.8);
        assert!(e.snapshot().vehicle);
    }

    #[test]
    fn touchpad_quarters_take_their_half() {
        let p = Profile::parse("[buttons]
pad_left = \"F1\"
pad_down_left = \"Z\"").unwrap();
        let mut e = Engine::new(p);
        let mut sink = RecordSink::default();
        let click = |e: &mut Engine, sink: &mut RecordSink, t: f64, x: u16, y: u16| {
            let mut s = state(PadButtons::TOUCHPAD, (t * 1e6) as u32);
            s.touch[0] = Touch { active: true, id: 1, x, y };
            e.update(t, &s, sink);
            e.update(t + 0.01, &state(PadButtons::NONE, ((t + 0.01) * 1e6) as u32), sink);
        };
        click(&mut e, &mut sink, 0.0, 300, 900);
        assert_eq!(names(&mut sink), ["+Z", "-Z"], "bound lower-left quarter");
        click(&mut e, &mut sink, 1.0, 300, 200);
        assert_eq!(names(&mut sink), ["+F1", "-F1"], "upper-left falls back to the left half");
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
    fn toggle_movement_switches_and_releases_keys() {
        let p = Profile::parse("[movement]\nmode = \"wasd\"\n[layer.r2]\nl3 = \"ToggleMovement\"").unwrap();
        let mut e = Engine::new(p);
        let mut sink = RecordSink::default();
        let mut s = state(PadButtons::NONE, 0);
        s.left = [0.0, 1.0];
        e.update(0.0, &s, &mut sink);
        assert_eq!(names(&mut sink), ["+W"]);
        s.r2 = 1.0;
        s.buttons = PadButtons::L3;
        e.update(0.01, &s, &mut sink);
        assert_eq!(e.movement_mode(), MoveMode::Hybrid);
        assert_eq!(names(&mut sink), ["-W"], "stick no longer drives WASD; the command never reaches the system");
        s.buttons = PadButtons::NONE;
        s.r2 = 0.0;
        e.update(0.02, &s, &mut sink);
        assert!(names(&mut sink).is_empty());
    }

    #[test]
    fn lightbar_shows_the_pending_pair() {
        assert_eq!(Engine::new(Profile::parse("").unwrap()).lightbar(), None, "off unless the profile asks");
        let p = Profile::parse("[lightbar]\nenabled = true\n[weapons]\nslots = 8\nkeys = [\"1\",\"2\",\"3\",\"4\",\"5\",\"6\",\"7\",\"8\"]")
            .unwrap();
        let mut e = Engine::new(p);
        let mut sink = RecordSink::default();
        assert_eq!(e.lightbar(), Some([0x30, 0x30, 0x30]));
        let mut s = state(PadButtons::CIRCLE, 0);
        s.r2 = 1.0;
        e.update(0.0, &s, &mut sink);
        assert_eq!(e.lightbar(), Some([0xFF, 0x28, 0x28]), "circle's second weapon is next");
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
