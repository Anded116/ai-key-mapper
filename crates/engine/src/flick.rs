//! Flick stick, ported from JoyShockMapper `JoyShock::handleFlickStick`: tilting the stick past
//! the outer deadzone turns the camera to face that direction, rotating the tilted stick turns
//! the camera by the same angle.

use crate::profile::{Deadzone, FlickCfg};

/// JSM keeps flicking while the rescaled length stays above 90% of the start threshold.
const RELEASE_FACTOR: f32 = 0.9;
/// JSM rotation smoothing (radians per sample): everything below `BOTTOM` is smoothed, above
/// `TOP` passes straight through, in between it blends. Window is 64 ms.
const SMOOTH_BOTTOM: f32 = 0.02;
const SMOOTH_TOP: f32 = 0.04;
const SMOOTH_WINDOW_S: f32 = 0.064;
const NUM_SAMPLES: usize = 64;

/// JSM `processDeadZones`: zero inside `inner`, full tilt in the last `outer`, rescaled between.
/// Returns the rescaled stick and its length (1.0 = pegged).
pub fn apply_deadzone(stick: [f32; 2], dz: Deadzone) -> ([f32; 2], f32) {
    let len = (stick[0] * stick[0] + stick[1] * stick[1]).sqrt();
    if len <= dz.inner || len == 0.0 {
        return ([0.0, 0.0], 0.0);
    }
    let scaled = ((len - dz.inner) / (1.0 - dz.inner - dz.outer)).min(1.0);
    ([stick[0] / len * scaled, stick[1] / len * scaled], scaled)
}

/// Stick angle in degrees: 0 = up (forward), positive = right.
pub fn stick_angle(stick: [f32; 2]) -> f32 {
    stick[0].atan2(stick[1]).to_degrees()
}

fn wrap_rad(a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let mut d = (a + PI) % TAU;
    if d < 0.0 {
        d += TAU;
    }
    d - PI
}

pub struct Flick {
    active: bool,
    last: [f32; 2],
    /// Flick in progress (radians, positive = right), time since it started, progress 0..1.
    delta: f32,
    since: f32,
    done: f32,
    samples: [f32; NUM_SAMPLES],
    front: usize,
}

impl Default for Flick {
    fn default() -> Self {
        Self { active: false, last: [0.0; 2], delta: 0.0, since: 0.0, done: 1.0, samples: [0.0; NUM_SAMPLES], front: 0 }
    }
}

impl Flick {
    pub fn active(&self) -> bool {
        self.active
    }

    /// Camera yaw in degrees for this sample, positive = right. `dt` is seconds since the last one.
    pub fn update(&mut self, dt: f32, raw: [f32; 2], cfg: &FlickCfg) -> f32 {
        if !cfg.enabled {
            self.active = false;
            return 0.0;
        }
        let (stick, len) = apply_deadzone(raw, cfg.deadzone);
        let threshold = if self.active { cfg.threshold * RELEASE_FACTOR } else { cfg.threshold };
        let mut out = 0.0f32;
        if len >= threshold {
            let angle = stick[0].atan2(stick[1]);
            if !self.active {
                self.active = true;
                let a = if angle.abs().to_degrees() < cfg.forward_deadzone { 0.0 } else { angle };
                self.delta = a;
                self.since = 0.0;
                self.done = 0.0;
                self.samples = [0.0; NUM_SAMPLES];
            } else {
                let last = self.last[0].atan2(self.last[1]);
                let change = wrap_rad(angle - last);
                let max_samples = ((SMOOTH_WINDOW_S / dt.max(1e-4)).ceil() as usize).clamp(1, NUM_SAMPLES);
                out += self.smoothed(change, max_samples);
            }
        } else {
            self.active = false;
        }
        self.last = stick;

        if self.done < 1.0 {
            self.since += dt;
            let mut pct = if cfg.time <= 0.0 { 1.0 } else { self.since / cfg.time };
            pct = pct.min(1.0);
            let shape = |p: f32| 1.0 - (1.0 - p) * (1.0 - p);
            out += (shape(pct) - shape(self.done)) * self.delta;
            self.done = pct;
        }
        out.to_degrees()
    }

    /// JSM `getSmoothedStickRotation`.
    fn smoothed(&mut self, value: f32, max_samples: usize) -> f32 {
        self.front = if self.front == 0 { NUM_SAMPLES - 1 } else { self.front - 1 };
        let immediate = ((value.abs() - SMOOTH_BOTTOM) / (SMOOTH_TOP - SMOOTH_BOTTOM)).clamp(0.0, 1.0);
        self.samples[self.front] = value * (1.0 - immediate);
        let mut result = 0.0;
        for i in 0..max_samples {
            result += self.samples[(self.front + i) % NUM_SAMPLES] / max_samples as f32;
        }
        result + value * immediate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> FlickCfg {
        FlickCfg {
            enabled: true,
            time: 0.1,
            threshold: 1.0,
            forward_deadzone: 0.0,
            deadzone: Deadzone { inner: 0.15, outer: 0.1 },
        }
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
    fn needs_full_tilt_past_the_outer_deadzone() {
        let mut f = Flick::default();
        assert_eq!(run(&mut f, [0.0, 0.85], 0.2), 0.0, "0.85 rescales to 0.93: no flick");
        assert!(run(&mut f, [0.0, 0.92], 0.2).abs() < 1e-3, "0.92 starts a flick forward (0°)");
        assert!(f.active());
        run(&mut f, [0.0, 0.84], 0.01);
        assert!(f.active(), "stays above 90% of the threshold");
        run(&mut f, [0.0, 0.8], 0.01);
        assert!(!f.active());
    }

    #[test]
    fn rotating_the_stick_rotates_the_camera() {
        let mut f = Flick::default();
        run(&mut f, [0.0, 1.0], 0.2);
        let mut total = 0.0;
        for i in 1..=200 {
            let a = (i as f32 / 200.0 * 90.0).to_radians();
            total += f.update(0.002, [a.sin(), a.cos()], &cfg());
        }
        for _ in 0..100 {
            total += f.update(0.002, [1.0, 0.0], &cfg());
        }
        assert!((total - 90.0).abs() < 0.5, "{total}");
    }

    #[test]
    fn deadzone_rescales_like_jsm() {
        let dz = Deadzone { inner: 0.15, outer: 0.1 };
        assert_eq!(apply_deadzone([0.1, 0.0], dz).1, 0.0);
        assert!((apply_deadzone([0.525, 0.0], dz).1 - 0.5).abs() < 1e-5);
        assert_eq!(apply_deadzone([0.95, 0.0], dz).1, 1.0);
    }
}
