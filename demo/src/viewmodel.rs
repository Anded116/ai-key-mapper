use std::f32::consts::PI;

use macroquad::prelude::*;

use crate::render::draw_box_m;
use crate::weapons::{Kind, SWITCH_TIME};
use crate::Game;

const METAL: Color = Color::new(0.34, 0.35, 0.38, 1.0);
const DARK: Color = Color::new(0.15, 0.16, 0.18, 1.0);
const POLY: Color = Color::new(0.22, 0.23, 0.22, 1.0);
const WOOD: Color = Color::new(0.48, 0.31, 0.18, 1.0);
const OD: Color = Color::new(0.34, 0.39, 0.25, 1.0);
const TAN: Color = Color::new(0.63, 0.56, 0.41, 1.0);
const STEEL: Color = Color::new(0.78, 0.8, 0.83, 1.0);
const RED: Color = Color::new(0.72, 0.16, 0.12, 1.0);
const GLASS: Color = Color::new(0.25, 0.55, 0.78, 1.0);
const SKIN: Color = Color::new(0.86, 0.66, 0.52, 1.0);
const SLEEVE: Color = Color::new(0.2, 0.25, 0.32, 1.0);

const HIP: Vec3 = Vec3::new(0.17, -0.17, -0.42);
pub const SWING_TIME: f32 = 0.42;
pub const SWING_WINDUP: f32 = 0.22;
pub const SWING_STRIKE_END: f32 = 0.55;
const THROW_TIME: f32 = 0.45;

/// One box of a weapon model in weapon space: -Z is the barrel, +Y is up.
struct Part {
    pos: Vec3,
    size: Vec3,
    color: Color,
    /// Rotation around X in degrees (grips and magazines lean).
    pitch: f32,
    /// Moves out and back during reload.
    mag: bool,
    /// Optic body: hidden while aiming so the camera does not look into a solid box.
    optic: bool,
}

const fn p(pos: [f32; 3], size: [f32; 3], color: Color) -> Part {
    Part { pos: Vec3::from_array(pos), size: Vec3::from_array(size), color, pitch: 0.0, mag: false, optic: false }
}

const fn tilt(pos: [f32; 3], size: [f32; 3], color: Color, pitch: f32) -> Part {
    Part { pos: Vec3::from_array(pos), size: Vec3::from_array(size), color, pitch, mag: false, optic: false }
}

const fn mag(pos: [f32; 3], size: [f32; 3], color: Color, pitch: f32) -> Part {
    Part { pos: Vec3::from_array(pos), size: Vec3::from_array(size), color, pitch, mag: true, optic: false }
}

const fn optic(pos: [f32; 3], size: [f32; 3], color: Color) -> Part {
    Part { pos: Vec3::from_array(pos), size: Vec3::from_array(size), color, pitch: 0.0, mag: false, optic: true }
}

struct Model {
    parts: &'static [Part],
    muzzle: Vec3,
    /// Where the right hand holds the weapon.
    grip: Vec3,
    /// Sight position: the camera lines up with it when aiming.
    sight: Vec3,
    ads_dist: f32,
}

const KNIFE: Model = Model {
    parts: &[
        p([0.0, 0.0, 0.03], [0.028, 0.035, 0.11], DARK),
        p([0.0, 0.005, -0.03], [0.06, 0.015, 0.012], METAL),
        p([0.0, 0.014, -0.16], [0.012, 0.045, 0.24], STEEL),
        p([0.0, 0.03, -0.13], [0.008, 0.008, 0.17], METAL),
    ],
    muzzle: Vec3::new(0.0, 0.012, -0.26),
    grip: Vec3::new(0.0, 0.0, 0.03),
    sight: Vec3::new(0.0, 0.08, 0.0),
    ads_dist: 0.4,
};

const PISTOL: Model = Model {
    parts: &[
        p([0.0, 0.035, -0.06], [0.034, 0.04, 0.2], DARK),
        p([0.0, 0.005, -0.05], [0.032, 0.025, 0.17], POLY),
        tilt([0.0, -0.055, 0.025], [0.032, 0.1, 0.042], POLY, -15.0),
        mag([0.0, -0.105, 0.035], [0.028, 0.02, 0.036], METAL, -15.0),
        p([0.0, -0.015, -0.02], [0.012, 0.02, 0.04], POLY),
        p([0.0, 0.06, -0.15], [0.006, 0.01, 0.008], DARK),
        p([0.0, 0.06, 0.03], [0.02, 0.01, 0.008], DARK),
    ],
    muzzle: Vec3::new(0.0, 0.035, -0.17),
    grip: Vec3::new(0.0, -0.05, 0.03),
    sight: Vec3::new(0.0, 0.065, 0.0),
    ads_dist: 0.38,
};

const SHOTGUN: Model = Model {
    parts: &[
        p([0.0, 0.0, 0.0], [0.05, 0.065, 0.24], DARK),
        p([0.0, 0.018, -0.36], [0.03, 0.03, 0.5], METAL),
        p([0.0, -0.02, -0.3], [0.028, 0.028, 0.4], METAL),
        mag([0.0, -0.022, -0.25], [0.05, 0.045, 0.16], WOOD, 0.0),
        tilt([0.0, -0.035, 0.25], [0.045, 0.08, 0.28], WOOD, 8.0),
        tilt([0.0, -0.06, 0.1], [0.034, 0.08, 0.04], WOOD, -18.0),
        p([0.0, 0.04, -0.58], [0.006, 0.01, 0.01], STEEL),
    ],
    muzzle: Vec3::new(0.0, 0.018, -0.62),
    grip: Vec3::new(0.0, -0.06, 0.1),
    sight: Vec3::new(0.0, 0.045, 0.0),
    ads_dist: 0.36,
};

const SMG: Model = Model {
    parts: &[
        p([0.0, 0.0, -0.02], [0.045, 0.06, 0.26], POLY),
        p([0.0, 0.01, -0.2], [0.022, 0.022, 0.12], DARK),
        mag([0.0, -0.1, -0.06], [0.028, 0.14, 0.04], DARK, 0.0),
        tilt([0.0, -0.07, 0.07], [0.034, 0.09, 0.038], POLY, -12.0),
        p([0.0, -0.005, 0.2], [0.02, 0.04, 0.16], DARK),
        optic([0.0, 0.045, -0.02], [0.03, 0.03, 0.06], DARK),
        optic([0.0, 0.045, -0.05], [0.024, 0.022, 0.004], GLASS),
    ],
    muzzle: Vec3::new(0.0, 0.01, -0.27),
    grip: Vec3::new(0.0, -0.065, 0.07),
    sight: Vec3::new(0.0, 0.05, 0.0),
    ads_dist: 0.34,
};

const RIFLE: Model = Model {
    parts: &[
        p([0.0, 0.0, 0.0], [0.05, 0.07, 0.34], OD),
        p([0.0, 0.003, -0.27], [0.055, 0.055, 0.22], DARK),
        p([0.0, 0.005, -0.45], [0.02, 0.02, 0.16], METAL),
        mag([0.0, -0.1, -0.07], [0.034, 0.15, 0.065], DARK, 12.0),
        tilt([0.0, -0.075, 0.08], [0.034, 0.1, 0.04], POLY, -15.0),
        p([0.0, -0.015, 0.28], [0.045, 0.08, 0.22], OD),
        p([0.0, 0.045, -0.02], [0.03, 0.015, 0.1], DARK),
        optic([0.0, 0.07, -0.02], [0.045, 0.04, 0.11], DARK),
        optic([0.0, 0.07, -0.077], [0.035, 0.03, 0.004], GLASS),
    ],
    muzzle: Vec3::new(0.0, 0.005, -0.54),
    grip: Vec3::new(0.0, -0.07, 0.08),
    sight: Vec3::new(0.0, 0.072, 0.0),
    ads_dist: 0.36,
};

const SNIPER: Model = Model {
    parts: &[
        p([0.0, 0.0, 0.0], [0.05, 0.06, 0.3], TAN),
        p([0.0, 0.01, -0.45], [0.025, 0.025, 0.6], DARK),
        p([0.0, -0.02, 0.3], [0.05, 0.09, 0.3], TAN),
        tilt([0.0, -0.07, 0.09], [0.035, 0.09, 0.045], TAN, -20.0),
        optic([0.0, 0.075, -0.03], [0.045, 0.045, 0.32], DARK),
        optic([0.0, 0.075, -0.192], [0.038, 0.038, 0.004], GLASS),
        p([0.0, 0.045, -0.09], [0.03, 0.02, 0.02], METAL),
        p([0.0, 0.045, 0.04], [0.03, 0.02, 0.02], METAL),
        p([0.04, 0.01, 0.06], [0.04, 0.012, 0.012], METAL),
        mag([0.0, -0.05, -0.03], [0.03, 0.05, 0.07], DARK, 0.0),
    ],
    muzzle: Vec3::new(0.0, 0.01, -0.75),
    grip: Vec3::new(0.0, -0.065, 0.09),
    sight: Vec3::new(0.0, 0.075, 0.0),
    ads_dist: 0.3,
};

const ROCKET: Model = Model {
    parts: &[
        p([0.0, 0.0, -0.1], [0.13, 0.13, 0.95], OD),
        p([0.0, 0.0, -0.57], [0.15, 0.15, 0.04], DARK),
        p([0.0, 0.0, 0.37], [0.15, 0.15, 0.04], DARK),
        tilt([0.0, -0.11, -0.05], [0.035, 0.1, 0.045], DARK, -10.0),
        p([0.0, -0.1, -0.3], [0.03, 0.08, 0.03], DARK),
        p([-0.085, 0.06, -0.15], [0.03, 0.05, 0.06], DARK),
        mag([0.0, 0.0, -0.6], [0.07, 0.07, 0.06], RED, 0.0),
    ],
    muzzle: Vec3::new(0.0, 0.0, -0.62),
    grip: Vec3::new(0.0, -0.11, -0.05),
    sight: Vec3::new(-0.085, 0.09, 0.0),
    ads_dist: 0.38,
};

const MACHINE_GUN: Model = Model {
    parts: &[
        p([0.0, 0.0, 0.0], [0.07, 0.09, 0.36], DARK),
        p([0.0, 0.01, -0.35], [0.05, 0.05, 0.34], METAL),
        p([0.0, 0.01, -0.56], [0.022, 0.022, 0.12], DARK),
        mag([-0.06, -0.06, -0.03], [0.05, 0.11, 0.12], OD, 0.0),
        p([-0.03, 0.0, -0.03], [0.02, 0.03, 0.06], TAN),
        tilt([0.0, -0.08, 0.1], [0.036, 0.1, 0.04], POLY, -15.0),
        p([0.0, -0.01, 0.3], [0.05, 0.09, 0.22], DARK),
        p([0.0, 0.07, -0.08], [0.02, 0.03, 0.12], DARK),
        p([0.0, -0.035, -0.45], [0.04, 0.015, 0.2], METAL),
    ],
    muzzle: Vec3::new(0.0, 0.01, -0.63),
    grip: Vec3::new(0.0, -0.075, 0.1),
    sight: Vec3::new(0.0, 0.06, 0.0),
    ads_dist: 0.36,
};

const MODELS: [&Model; 8] = [&KNIFE, &PISTOL, &SHOTGUN, &SMG, &RIFLE, &SNIPER, &ROCKET, &MACHINE_GUN];

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn draw_model(model: &Model, m: Mat4, mag_drop: f32, hand: bool, ads: f32) {
    for part in model.parts {
        if part.optic && ads > 0.6 {
            continue;
        }
        let mut pos = part.pos;
        if part.mag {
            pos.y -= mag_drop;
        }
        let pm = m * Mat4::from_translation(pos) * Mat4::from_rotation_x(part.pitch.to_radians());
        draw_box_m(&pm, part.size, part.color);
    }
    if hand {
        let g = model.grip;
        draw_box_m(&(m * Mat4::from_translation(g)), vec3(0.06, 0.075, 0.07), SKIN);
        let sleeve = m * Mat4::from_translation(g + vec3(0.03, -0.06, 0.16)) * Mat4::from_rotation_x(0.5);
        draw_box_m(&sleeve, vec3(0.075, 0.075, 0.26), SLEEVE);
    }
}

impl Game {
    /// Draws the first-person weapon in camera space (camera at origin looking down -Z).
    pub fn draw_viewmodel(&self, now: f64) {
        let a = &self.arms;
        let idx = a.cur;
        let model = MODELS[idx];
        let ads = smooth(a.ads);

        let ads_pos = vec3(-model.sight.x, -model.sight.y, -model.ads_dist);
        let mut pos = HIP.lerp(ads_pos, ads);
        let (mut yaw, mut pitch, mut roll) = (0.0f32, 0.0f32, 0.0f32);

        let speed = self.p.horizontal_speed();
        if self.p.on_ground && speed > 0.3 {
            let t = now as f32 * (4.0 + speed * 1.3);
            let amp = (speed / 5.0).min(1.5) * (1.0 - ads * 0.85);
            pos.x += t.sin() * 0.012 * amp;
            pos.y -= t.cos().abs() * 0.012 * amp;
        }
        if !self.p.on_ground {
            pos.y -= 0.015;
        }

        yaw += self.sway.x * (1.0 - ads * 0.7);
        pitch += self.sway.y * (1.0 - ads * 0.7);
        pos.x += self.sway.x * 0.04;

        let sk = self.sprint_k * (1.0 - ads);
        pos += vec3(0.02, -0.06, 0.04) * sk;
        yaw += 0.55 * sk;
        pitch -= 0.25 * sk;

        let heavy = if matches!(idx, 2 | 5 | 6) { 1.8 } else { 1.0 };
        pos.z += 0.04 * a.kick * heavy;
        pitch += 0.08 * a.kick * heavy;

        let mut mag_drop = 0.0;
        if let Some(t) = a.reload_until {
            let d = self.arms.def();
            let q = (1.0 - (t - now) / d.reload).clamp(0.0, 1.0) as f32;
            let k = (q * PI).sin();
            roll -= 0.55 * k;
            pitch -= 0.3 * k;
            pos.y -= 0.05 * k;
            // Magazine out during the first half, back in during the second.
            let out = ((q - 0.15) / 0.25).clamp(0.0, 1.0) - ((q - 0.6) / 0.25).clamp(0.0, 1.0);
            mag_drop = 0.22 * out;
        }

        if now < a.switch_until {
            let k = ((a.switch_until - now) / SWITCH_TIME) as f32;
            pos.y -= 0.3 * k;
            pitch -= 0.9 * k;
        }

        let swing_q = ((now - a.swing_at) as f32 / SWING_TIME).clamp(0.0, 1.0);
        let swinging = swing_q < 1.0;
        let throw_q = ((now - a.throw_at) as f32 / THROW_TIME).clamp(0.0, 1.0);
        let throwing = throw_q < 1.0;
        let is_knife = matches!(self.arms.def().kind, Kind::Melee { .. });
        if swinging && !is_knife {
            // The gun drops out of view so the knife is the only thing moving.
            let k = smooth(swing_q / 0.15) * (1.0 - smooth((swing_q - 0.75) / 0.25));
            pos.y -= 0.4 * k;
            pitch -= 0.7 * k;
        }
        if throwing {
            let k = (throw_q * PI).sin();
            pos.y -= 0.14 * k;
            pos.x += 0.06 * k;
            roll += 0.35 * k;
        }

        if !(is_knife && swinging) {
            let m = Mat4::from_translation(pos) * Mat4::from_euler(EulerRot::YXZ, yaw, pitch, roll);
            draw_model(model, m, mag_drop, true, ads);
            if now < self.fx.muzzle_until {
                let mz = m.transform_point3(model.muzzle);
                draw_sphere(mz, 0.045, None, Color::new(1.0, 0.85, 0.4, 0.95));
                draw_sphere(mz, 0.09, None, Color::new(1.0, 0.6, 0.2, 0.35));
            }
        }
        if swinging {
            draw_swing(swing_q);
        }
        if throwing && throw_q < 0.6 {
            let k = smooth(throw_q / 0.6);
            let gp = vec3(-0.18, -0.24, -0.32).lerp(vec3(-0.04, 0.02, -0.75), k);
            draw_box_m(&Mat4::from_translation(gp), vec3(0.06, 0.08, 0.06), Color::new(0.25, 0.35, 0.2, 1.0));
        }
    }
}

/// Swing keyframes: (phase, position, yaw/pitch/roll). Wind-up raises the blade at the
/// upper right, the strike cuts through the centre to the lower left, then it leaves the frame.
const SWING_KEYS: [(f32, Vec3, Vec3); 4] = [
    (0.0, Vec3::new(0.3, -0.4, -0.4), Vec3::new(0.4, 0.3, -0.8)),
    (SWING_WINDUP, Vec3::new(0.26, 0.06, -0.4), Vec3::new(0.5, 1.0, -0.9)),
    (SWING_STRIKE_END, Vec3::new(-0.2, -0.22, -0.46), Vec3::new(-0.6, -0.4, -0.2)),
    (1.0, Vec3::new(-0.26, -0.55, -0.42), Vec3::new(-0.7, -0.6, 0.0)),
];

fn swing_matrix(q: f32) -> Mat4 {
    let i = SWING_KEYS.iter().rposition(|k| k.0 <= q).unwrap_or(0).min(SWING_KEYS.len() - 2);
    let (a, b) = (SWING_KEYS[i], SWING_KEYS[i + 1]);
    let t = ((q - a.0) / (b.0 - a.0)).clamp(0.0, 1.0);
    // The strike is fast out of the wind-up and slows into the follow-through.
    let e = if i == 1 { 1.0 - (1.0 - t).powi(3) } else { smooth(t) };
    let pos = a.1.lerp(b.1, e);
    let rot = a.2.lerp(b.2, e);
    Mat4::from_translation(pos) * Mat4::from_euler(EulerRot::YXZ, rot.x, rot.y, rot.z)
}

fn draw_swing(q: f32) {
    draw_model(&KNIFE, swing_matrix(q), 0.0, false, 0.0);
    if !(SWING_WINDUP..SWING_STRIKE_END + 0.2).contains(&q) {
        return;
    }
    // Motion smear: fading copies of the blade along the recent path.
    let blade = &KNIFE.parts[2];
    for i in 1..=5 {
        let past = q - i as f32 * 0.03;
        if past < SWING_WINDUP + 0.04 {
            break;
        }
        let m = swing_matrix(past) * Mat4::from_translation(blade.pos);
        let a = 0.32 * (1.0 - i as f32 / 6.0);
        draw_box_m(&m, blade.size * vec3(1.5, 1.1, 1.0), Color::new(0.92, 0.95, 1.0, a));
    }
}

/// View punch during the strike: (yaw, roll) in radians, sells the impact.
pub fn swing_punch(since_swing: f32) -> (f32, f32) {
    let q = since_swing / SWING_TIME;
    let s = ((q - SWING_WINDUP) / (SWING_STRIKE_END - SWING_WINDUP + 0.15)).clamp(0.0, 1.0);
    let bump = (s * PI).sin();
    (-0.03 * bump, 0.035 * bump)
}
