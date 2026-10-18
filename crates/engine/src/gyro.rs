//! Gyro mouse: bias calibration and the JSM-style sensitivity curve.

use crate::profile::GyroCfg;

#[derive(Default)]
struct Accum {
    sum: [f64; 3],
    n: u32,
}

#[derive(Default)]
pub struct Gyro {
    bias: [f32; 3],
    accum: Option<Accum>,
    started: bool,
    startup_until: Option<f64>,
}

impl Gyro {
    pub fn bias(&self) -> [f32; 3] {
        self.bias
    }

    pub fn calibrating(&self) -> bool {
        self.accum.is_some()
    }

    pub fn begin_calibration(&mut self) {
        self.accum = Some(Accum::default());
    }

    /// Applies the mean collected since `begin_calibration`.
    pub fn end_calibration(&mut self) {
        if let Some(a) = self.accum.take() {
            if a.n > 0 {
                for i in 0..3 {
                    self.bias[i] = (a.sum[i] / a.n as f64) as f32;
                }
            }
        }
    }

    /// Camera turn in degrees for this sample: (right, down). `rates` are deg/s (pitch, yaw, roll).
    pub fn update(&mut self, now: f64, dt: f32, rates: [f32; 3], cfg: &GyroCfg, enabled: bool) -> (f32, f32) {
        if !self.started {
            self.started = true;
            if cfg.startup_calibration > 0.0 {
                self.startup_until = Some(now + cfg.startup_calibration as f64);
                self.begin_calibration();
            }
        }
        if let Some(a) = &mut self.accum {
            for (sum, rate) in a.sum.iter_mut().zip(rates) {
                *sum += rate as f64;
            }
            a.n += 1;
            // Startup calibration ends by itself; a button-held one ends on release.
            if self.startup_until.is_some_and(|t| now >= t) {
                self.startup_until = None;
                self.end_calibration();
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
            invert_x: false,
            invert_y: false,
        }
    }

    #[test]
    fn startup_calibration_removes_bias() {
        let c = cfg();
        let mut g = Gyro::default();
        let mut t = 0.0;
        while t < 0.6 {
            assert_eq!(g.update(t, 0.002, [0.5, -0.4, 0.0], &c, true), (0.0, 0.0));
            t += 0.002;
        }
        assert!(!g.calibrating());
        let (x, y) = g.update(t, 0.002, [0.5, -0.4, 0.0], &c, true);
        assert!(x.abs() < 1e-4 && y.abs() < 1e-4, "bias removed: {x} {y}");
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
