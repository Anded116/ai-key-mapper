//! Gyro mouse: bias calibration and the JSM-style sensitivity curve.

use crate::profile::GyroCfg;

/// Seconds dropped after the calibration starts and before it ends: pressing and releasing a
/// button jolts the controller.
pub const TRIM_S: f64 = 0.3;
/// Samples further than this (deg/s) from the median are motion, not bias.
const OUTLIER_DPS: f32 = 1.5;
const MIN_SAMPLES: usize = 50;
const MAX_SAMPLES: usize = 50_000;
/// Rest calibration: the pad counts as lying still after this long without motion.
const REST_S: f64 = 1.5;
/// Still means every gyro axis stays within this of its mean (deg/s): hands shake more than this,
/// a table does not.
const REST_SPREAD_DPS: f32 = 0.6;
/// ... and gravity does not move by more than this (g): a slow, smooth turn tilts it, rest does not.
const REST_SPREAD_G: f32 = 0.012;
/// A bias this far off is not drift but a pad moving steadily; leave it alone.
const REST_MAX_BIAS_DPS: f32 = 6.0;
/// Rest results closer than this to the current bias are not worth a log line.
const REST_REPORT_DPS: f32 = 0.08;

/// A sample kept for rest calibration: time, rates, acceleration.
type RestSample = (f64, [f32; 3], [f32; 3]);

struct Accum {
    started: f64,
    samples: Vec<(f64, [f32; 3])>,
}

/// Outcome of the last finished calibration, for the UI.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CalibrationResult {
    /// New bias, or `None` when there were too few still samples and the old bias stays.
    pub bias: Option<[f32; 3]>,
    pub samples: usize,
    /// Found by itself while the pad lay still, not by a button or at startup.
    pub auto: bool,
}

#[derive(Default)]
pub struct Gyro {
    bias: [f32; 3],
    accum: Option<Accum>,
    started: bool,
    startup_until: Option<f64>,
    last: Option<CalibrationResult>,
    /// Recent samples for rest calibration: time, rates, acceleration.
    rest: Vec<RestSample>,
    rest_checked: f64,
}

impl Gyro {
    pub fn bias(&self) -> [f32; 3] {
        self.bias
    }

    pub fn calibrating(&self) -> bool {
        self.accum.is_some()
    }

    /// Result of the last calibration, once.
    pub fn take_result(&mut self) -> Option<CalibrationResult> {
        self.last.take()
    }

    pub fn begin_calibration(&mut self, now: f64) {
        self.accum = Some(Accum { started: now, samples: Vec::new() });
    }

    /// Bias = mean of the still samples between `started + trim_start` and `now - trim_end`.
    pub fn end_calibration(&mut self, now: f64, trim_start: f64, trim_end: f64) {
        let Some(a) = self.accum.take() else { return };
        let (from, to) = (a.started + trim_start, now - trim_end);
        let kept: Vec<[f32; 3]> = a.samples.iter().filter(|(t, _)| *t >= from && *t <= to).map(|(_, r)| *r).collect();
        let mut bias = [0.0f32; 3];
        let mut used = usize::MAX;
        for axis in 0..3 {
            let mut v: Vec<f32> = kept.iter().map(|r| r[axis]).collect();
            if v.len() < MIN_SAMPLES {
                used = 0;
                break;
            }
            v.sort_by(|a, b| a.total_cmp(b));
            let median = v[v.len() / 2];
            let near: Vec<f32> = v.into_iter().filter(|x| (x - median).abs() <= OUTLIER_DPS).collect();
            used = used.min(near.len());
            bias[axis] = near.iter().sum::<f32>() / near.len() as f32;
        }
        let ok = used >= MIN_SAMPLES;
        if ok {
            self.bias = bias;
        }
        self.last = Some(CalibrationResult { bias: ok.then_some(bias), samples: used, auto: false });
    }

    /// Rest calibration: a pad lying still for `REST_S` gets its bias from those samples, without a
    /// button — drift goes away whenever the pad is put down.
    pub fn watch_rest(&mut self, now: f64, rates: [f32; 3], accel: [f32; 3], cfg: &GyroCfg) {
        if !cfg.auto_calibrate || self.accum.is_some() {
            self.rest.clear();
            return;
        }
        self.rest.push((now, rates, accel));
        self.rest.retain(|(t, _, _)| now - *t <= REST_S);
        if now - self.rest_checked < 0.25 || self.rest.len() < MIN_SAMPLES || now - self.rest[0].0 < REST_S * 0.95 {
            return;
        }
        self.rest_checked = now;
        let n = self.rest.len() as f32;
        let spread = |pick: &dyn Fn(&RestSample) -> f32| {
            let (lo, hi) = self.rest.iter().map(pick).fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
            hi - lo
        };
        for axis in 0..3 {
            if spread(&|s| s.1[axis]) > REST_SPREAD_DPS * 2.0 || spread(&|s| s.2[axis]) > REST_SPREAD_G * 2.0 {
                return;
            }
        }
        let mut bias = [0.0f32; 3];
        for (axis, b) in bias.iter_mut().enumerate() {
            *b = self.rest.iter().map(|s| s.1[axis]).sum::<f32>() / n;
        }
        if bias.iter().any(|b| b.abs() > REST_MAX_BIAS_DPS) {
            return;
        }
        let change = (0..3).map(|a| (bias[a] - self.bias[a]).abs()).fold(0.0, f32::max);
        self.bias = bias;
        if change > REST_REPORT_DPS {
            self.last = Some(CalibrationResult { bias: Some(bias), samples: self.rest.len(), auto: true });
        }
    }

    /// Camera turn in degrees for this sample: (right, down). `rates` are deg/s (pitch, yaw, roll).
    pub fn update(&mut self, now: f64, dt: f32, rates: [f32; 3], cfg: &GyroCfg, enabled: bool) -> (f32, f32) {
        if !self.started {
            self.started = true;
            if cfg.startup_calibration > 0.0 {
                self.startup_until = Some(now + cfg.startup_calibration as f64);
                self.begin_calibration(now);
            }
        }
        if let Some(a) = &mut self.accum {
            if a.samples.len() < MAX_SAMPLES {
                a.samples.push((now, rates));
            }
            // Startup calibration ends by itself; a button one ends from the engine.
            if self.startup_until.is_some_and(|t| now >= t) {
                self.startup_until = None;
                self.end_calibration(now, 0.2, 0.0);
            }
            return (0.0, 0.0);
        }
        if !cfg.enabled || !enabled {
            return (0.0, 0.0);
        }
        let pitch = rates[0] - self.bias[0];
        let yaw = rates[1] - self.bias[1];
        let speed = (pitch * pitch + yaw * yaw).sqrt();
        let span = cfg.max_threshold - cfg.min_threshold;
        let t = if span <= 0.0 { 1.0 } else { ((speed - cfg.min_threshold) / span).clamp(0.0, 1.0) };
        let sens = cfg.min_sens + (cfg.max_sens - cfg.min_sens) * t;
        // Counter-clockwise yaw (turning left) is positive: camera right is its negative.
        let mut x = -yaw * sens * dt;
        let mut y = -pitch * sens * dt;
        if cfg.invert_x {
            x = -x;
        }
        if cfg.invert_y {
            y = -y;
        }
        (x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> GyroCfg {
        GyroCfg {
            enabled: true,
            min_sens: 2.0,
            max_sens: 4.0,
            min_threshold: 0.0,
            max_threshold: 100.0,
            off: None,
            calibrate: None,
            startup_calibration: 0.5,
            auto_calibrate: true,
            invert_x: false,
            invert_y: false,
        }
    }

    /// Rates and acceleration of a pad on a table: bias plus sensor noise.
    fn table(i: usize, bias: [f32; 3]) -> ([f32; 3], [f32; 3]) {
        let n = ((i * 7919) % 13) as f32 / 13.0 - 0.5;
        ([bias[0] + n * 0.3, bias[1] - n * 0.2, bias[2] + n * 0.1], [0.0 + n * 0.004, -1.0, 0.01])
    }

    #[test]
    fn a_pad_put_down_calibrates_itself() {
        let c = cfg();
        let mut g = Gyro::default();
        for i in 0..1000 {
            let (r, a) = table(i, [0.7, -0.5, 0.2]);
            g.watch_rest(i as f64 * 0.002, r, a, &c);
        }
        let b = g.bias();
        assert!((b[0] - 0.7).abs() < 0.05 && (b[1] + 0.5).abs() < 0.05, "{b:?}");
        assert!(g.take_result().is_some_and(|r| r.auto));
    }

    #[test]
    fn hands_and_slow_turns_are_not_rest() {
        let c = cfg();
        let mut g = Gyro::default();
        // In hands: tremor of a few deg/s.
        for i in 0..1500 {
            let shake = ((i as f32) * 0.3).sin() * 2.5;
            g.watch_rest(i as f64 * 0.002, [0.7 + shake, -0.5, 0.2], [0.0, -1.0, 0.0], &c);
        }
        assert_eq!(g.bias(), [0.0; 3], "tremor is motion");
        // A slow, perfectly smooth turn: steady 3 deg/s, gravity swings with it.
        let mut g = Gyro::default();
        for i in 0..1500 {
            let angle = (i as f32 * 0.002 * 3.0).to_radians();
            g.watch_rest(i as f64 * 0.002, [3.0, 0.0, 0.0], [0.0, -angle.cos(), angle.sin()], &c);
        }
        assert_eq!(g.bias(), [0.0; 3], "a steady turn tilts gravity");
    }

    #[test]
    fn startup_calibration_removes_bias() {
        let c = cfg();
        let mut g = Gyro::default();
        let mut t = 0.0;
        while t < 0.499 {
            assert_eq!(g.update(t, 0.002, [0.5, -0.4, 0.0], &c, true), (0.0, 0.0));
            t += 0.002;
        }
        g.update(t + 0.002, 0.002, [0.5, -0.4, 0.0], &c, true);
        t += 0.004;
        assert!(!g.calibrating());
        let (x, y) = g.update(t, 0.002, [0.5, -0.4, 0.0], &c, true);
        assert!(x.abs() < 1e-4 && y.abs() < 1e-4, "bias removed: {x} {y}");
    }

    #[test]
    fn button_jolts_and_spikes_do_not_reach_the_bias() {
        let mut c = cfg();
        c.startup_calibration = 0.0;
        let mut g = Gyro::default();
        g.begin_calibration(0.0);
        let mut t = 0.0;
        while t < 2.0 {
            // True bias 0.4 deg/s; the press (first 0.1 s) and release (last 0.1 s) shake the pad.
            let jolt = if !(0.1..=1.9).contains(&t) { 40.0 } else { 0.0 };
            let spike = if (1.0..1.01).contains(&t) { 25.0 } else { 0.0 };
            g.update(t, 0.002, [0.4 + jolt + spike, -0.2 - jolt, 0.0], &c, true);
            t += 0.002;
        }
        g.end_calibration(t, TRIM_S, TRIM_S);
        let b = g.bias();
        assert!((b[0] - 0.4).abs() < 0.01 && (b[1] + 0.2).abs() < 0.01, "{b:?}");
        let r = g.take_result().unwrap();
        assert!(r.bias.is_some() && r.samples > 500);
    }

    #[test]
    fn too_short_calibration_keeps_the_old_bias() {
        let mut c = cfg();
        c.startup_calibration = 0.0;
        let mut g = Gyro::default();
        g.begin_calibration(0.0);
        for i in 0..100 {
            g.update(i as f64 * 0.002, 0.002, [5.0, 5.0, 5.0], &c, true);
        }
        g.end_calibration(0.2, TRIM_S, TRIM_S);
        assert_eq!(g.bias(), [0.0; 3]);
        assert_eq!(g.take_result().unwrap().bias, None);
    }

    #[test]
    fn sensitivity_curve_and_direction() {
        let mut c = cfg();
        c.startup_calibration = 0.0;
        let mut g = Gyro::default();
        // Slow left turn: yaw +10 deg/s for 1 s at the low end of the curve (2.2x).
        let (x, _) = g.update(0.0, 1.0, [0.0, 10.0, 0.0], &c, true);
        assert!((x + 22.0).abs() < 1e-3, "{x}");
        // Fast turn above the max threshold uses max_sens.
        let (x, _) = g.update(1.0, 0.1, [0.0, -200.0, 0.0], &c, true);
        assert!((x - 80.0).abs() < 1e-3, "{x}");
        assert_eq!(g.update(2.0, 0.1, [0.0, 50.0, 0.0], &c, false), (0.0, 0.0), "gyro off button");
    }
}
