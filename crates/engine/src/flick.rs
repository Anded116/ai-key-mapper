//! Flick stick: tilting the stick turns the camera to face that direction, rotating the tilted
//! stick turns the camera by the same angle. See GyroWiki "Flick Stick".

use crate::profile::FlickCfg;

/// Deflection the stick must drop below to end a flick (hysteresis under the threshold).
const RELEASE_MARGIN: f32 = 0.1;
/// Rotation steps smaller than this (degrees) are averaged: 8-bit sticks jitter by ~0.5°.
const SMOOTH_BELOW: f32 = 1.0;
const SMOOTH_SAMPLES: usize = 4;

#[derive(Default)]
pub struct Flick {
    active: bool,
    prev_angle: f32,
    /// Flick in progress: total angle, time spent, angle already sent.
    target: f32,
    elapsed: f32,
    sent: f32,
    smooth: [f32; SMOOTH_SAMPLES],
    smooth_i: usize,
}

/// Stick angle in degrees: 0 = up (forward), positive = right.
pub fn stick_angle(stick: [f32; 2]) -> f32 {
    stick[0].atan2(stick[1]).to_degrees()
}

fn wrap(deg: f32) -> f32 {
    let mut d = deg % 360.0;
    if d > 180.0 {
        d -= 360.0;
    } else if d < -180.0 {
        d += 360.0;
    }
    d
}

impl Flick {
    pub fn active(&self) -> bool {
        self.active
    }

    /// Camera yaw in degrees for this sample, positive = right.
    pub fn update(&mut self, dt: f32, stick: [f32; 2], cfg: &FlickCfg) -> f32 {
        if !cfg.enabled {
            self.active = false;
            return 0.0;
        }
        let len = (stick[0] * stick[0] + stick[1] * stick[1]).sqrt();
        let angle = stick_angle(stick);
        let mut out = 0.0;
        if !self.active {
            if len >= cfg.threshold {
                self.active = true;
                self.prev_angle = angle;
                self.target = if angle.abs() <= cfg.forward_deadzone { 0.0 } else { angle };
                self.elapsed = 0.0;
                self.sent = 0.0;
                self.smooth = [0.0; SMOOTH_SAMPLES];
            }
        } else if len < cfg.threshold - RELEASE_MARGIN {
            self.active = false;
        } else {
            let step = wrap(angle - self.prev_angle);
            self.prev_angle = angle;
            out += self.smoothed(step);
        }
        if self.sent != self.target {
            self.elapsed += dt;
            let k = if cfg.time <= 0.0 { 1.0 } else { (self.elapsed / cfg.time).min(1.0) };
            // Ease out: fast start, gentle stop.
            let want = self.target * (1.0 - (1.0 - k) * (1.0 - k));
            out += want - self.sent;
            self.sent = if k >= 1.0 { self.target } else { want };
        }
        out
    }

    fn smoothed(&mut self, step: f32) -> f32 {
        if step.abs() >= SMOOTH_BELOW {
            self.smooth = [0.0; SMOOTH_SAMPLES];
            return step;
        }
        self.smooth[self.smooth_i] = step;
        self.smooth_i = (self.smooth_i + 1) % SMOOTH_SAMPLES;
        self.smooth.iter().sum::<f32>() / SMOOTH_SAMPLES as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> FlickCfg {
        FlickCfg { enabled: true, time: 0.1, threshold: 0.9, forward_deadzone: 0.0 }
    }

    fn run(f: &mut Flick, stick: [f32; 2], seconds: f32) -> f32 {
        let mut total = 0.0;
        let steps = (seconds / 0.002) as usize;
        for _ in 0..steps {
            total += f.update(0.002, stick, &cfg());
        }
        total
    }

    #[test]
    fn flick_turns_to_stick_direction() {
        let mut f = Flick::default();
        assert!((run(&mut f, [1.0, 0.0], 0.2) - 90.0).abs() < 0.01, "right = +90°");
        run(&mut f, [0.0, 0.0], 0.05);
        assert!((run(&mut f, [0.0, -1.0], 0.2).abs() - 180.0).abs() < 0.01, "back = 180°");
        run(&mut f, [0.0, 0.0], 0.05);
        let d = std::f32::consts::FRAC_1_SQRT_2;
        assert!((run(&mut f, [-d, d], 0.2) + 45.0).abs() < 0.05, "up-left = -45°");
    }

    #[test]
    fn small_tilt_does_nothing() {
        let mut f = Flick::default();
        assert_eq!(run(&mut f, [0.5, 0.5], 0.2), 0.0);
    }

    #[test]
    fn rotating_the_stick_rotates_the_camera() {
        let mut f = Flick::default();
        run(&mut f, [0.0, 1.0], 0.2);
        let mut total = 0.0;
        // Sweep a quarter circle clockwise from up to right in 200 steps.
        for i in 1..=200 {
            let a = (i as f32 / 200.0 * 90.0).to_radians();
            total += f.update(0.002, [a.sin(), a.cos()], &cfg());
        }
        for _ in 0..10 {
            total += f.update(0.002, [1.0, 0.0], &cfg());
        }
        assert!((total - 90.0).abs() < 0.5, "{total}");
    }

    #[test]
    fn angle_wraps_across_the_back() {
        assert!((wrap(350.0 - 10.0) + 20.0).abs() < 1e-4);
        assert!((wrap(-170.0 - 170.0) - 20.0).abs() < 1e-4);
    }
}
