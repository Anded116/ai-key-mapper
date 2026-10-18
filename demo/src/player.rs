use macroquad::prelude::*;

use crate::world::{Aabb, World, SPAWN};

const RADIUS: f32 = 0.35;
const STAND_H: f32 = 1.8;
const CROUCH_H: f32 = 1.15;
const EYE_BELOW_TOP: f32 = 0.12;
const GRAVITY: f32 = 16.0;
const JUMP_VEL: f32 = 6.0;
const EPS: f32 = 1e-4;

pub const SPEED_WALK: f32 = 2.2;
pub const SPEED_RUN: f32 = 4.8;
pub const SPEED_SPRINT: f32 = 7.5;
pub const SPEED_CROUCH: f32 = 2.2;

#[derive(Clone, Copy, PartialEq)]
pub enum Stance {
    Run,
    Walk,
    Sprint,
    Crouch,
}

pub struct Player {
    pub pos: Vec3,
    pub vel: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
    pub crouching: bool,
    pub height: f32,
    pub stance: Stance,
    pub jumps: u32,
}

pub struct MoveIntent {
    /// x = strafe right, y = forward; length 0..1 (analog when it comes from a stick).
    pub wish: Vec2,
    pub jump: bool,
    pub crouch: bool,
    pub sprint: bool,
    pub walk: bool,
}

impl Player {
    pub fn new() -> Self {
        Self {
            pos: SPAWN,
            vel: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            on_ground: true,
            crouching: false,
            height: STAND_H,
            stance: Stance::Run,
            jumps: 0,
        }
    }

    pub fn eye(&self) -> Vec3 {
        self.pos + vec3(0.0, self.height - EYE_BELOW_TOP, 0.0)
    }

    pub fn look_dir(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        vec3(sy * cp, sp, -cy * cp)
    }

    pub fn flat_forward(&self) -> Vec3 {
        vec3(self.yaw.sin(), 0.0, -self.yaw.cos())
    }

    pub fn right(&self) -> Vec3 {
        vec3(self.yaw.cos(), 0.0, self.yaw.sin())
    }

    pub fn horizontal_speed(&self) -> f32 {
        vec2(self.vel.x, self.vel.z).length()
    }

    /// `deg_per_count` already includes sensitivity and ADS scaling.
    pub fn look(&mut self, counts: Vec2, deg_per_count: f32, invert_y: bool) {
        let k = deg_per_count.to_radians();
        self.yaw += counts.x * k;
        let dy = if invert_y { counts.y } else { -counts.y };
        self.pitch = (self.pitch + dy * k).clamp(-89f32.to_radians(), 89f32.to_radians());
    }

    fn aabb_at(pos: Vec3, h: f32) -> Aabb {
        Aabb::new(pos - vec3(RADIUS, 0.0, RADIUS), pos + vec3(RADIUS, h, RADIUS))
    }

    pub fn step(&mut self, dt: f32, m: &MoveIntent, world: &World) {
        if m.crouch {
            self.crouching = true;
        } else if self.crouching && !world.solid(&Self::aabb_at(self.pos, STAND_H)) {
            self.crouching = false;
        }
        let target_h = if self.crouching { CROUCH_H } else { STAND_H };
        if target_h < self.height {
            // Shrink immediately so the collision box fits the tunnel; keep the eye smooth in the air.
            self.height = target_h;
        } else {
            self.height = (self.height + 6.0 * dt).min(target_h);
        }

        let wish = m.wish.clamp_length_max(1.0);
        self.stance = if self.crouching {
            Stance::Crouch
        } else if m.sprint && wish.y > 0.5 {
            Stance::Sprint
        } else if m.walk {
            Stance::Walk
        } else {
            Stance::Run
        };
        let speed = match self.stance {
            Stance::Crouch => SPEED_CROUCH,
            Stance::Sprint => SPEED_SPRINT,
            Stance::Walk => SPEED_WALK,
            Stance::Run => SPEED_RUN,
        };
        let target = (self.right() * wish.x + self.flat_forward() * wish.y) * speed;
        let accel = if self.on_ground { 45.0 } else { 10.0 };
        let cur = vec2(self.vel.x, self.vel.z);
        let goal = vec2(target.x, target.z);
        let diff = goal - cur;
        let next = cur + diff.clamp_length_max(accel * dt);
        self.vel.x = next.x;
        self.vel.z = next.y;

        if m.jump && self.on_ground {
            self.vel.y = JUMP_VEL;
            self.on_ground = false;
            self.jumps += 1;
        }
        self.vel.y -= GRAVITY * dt;

        self.move_axis(0, self.vel.x * dt, world);
        self.move_axis(2, self.vel.z * dt, world);
        self.on_ground = false;
        self.move_axis(1, self.vel.y * dt, world);
        // At very high frame rates one frame of gravity is smaller than EPS and never touches
        // the floor, so probe below the feet instead of relying on the collision alone.
        if self.vel.y <= 0.0 && world.solid(&Self::aabb_at(self.pos - vec3(0.0, 0.02, 0.0), self.height)) {
            self.on_ground = true;
            self.vel.y = 0.0;
        }
    }

    fn move_axis(&mut self, axis: usize, delta: f32, world: &World) {
        if delta == 0.0 {
            return;
        }
        self.pos[axis] += delta;
        for bl in &world.blocks {
            let me = Self::aabb_at(self.pos, self.height);
            if !me.overlaps(&bl.b) {
                continue;
            }
            match axis {
                1 if delta < 0.0 => {
                    self.pos.y = bl.b.max.y + EPS;
                    self.on_ground = true;
                    self.vel.y = 0.0;
                }
                1 => {
                    self.pos.y = bl.b.min.y - self.height - EPS;
                    self.vel.y = 0.0;
                }
                _ if delta > 0.0 => {
                    self.pos[axis] = bl.b.min[axis] - RADIUS - EPS;
                    self.vel[axis] = 0.0;
                }
                _ => {
                    self.pos[axis] = bl.b.max[axis] + RADIUS + EPS;
                    self.vel[axis] = 0.0;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idle() -> MoveIntent {
        MoveIntent { wish: Vec2::ZERO, jump: false, crouch: false, sprint: false, walk: false }
    }

    #[test]
    fn stays_grounded_at_high_fps() {
        let world = World::new();
        let mut p = Player::new();
        for _ in 0..2000 {
            p.step(1.0 / 2000.0, &idle(), &world);
            assert!(p.on_ground);
        }
    }

    #[test]
    fn jump_reaches_one_meter_step() {
        let world = World::new();
        let mut p = Player::new();
        p.step(0.01, &MoveIntent { jump: true, ..idle() }, &world);
        let mut apex: f32 = 0.0;
        for _ in 0..200 {
            p.step(0.005, &idle(), &world);
            apex = apex.max(p.pos.y);
        }
        assert!(apex > 1.0 && apex < 1.3, "apex {apex}");
        assert!(p.on_ground);
    }

    #[test]
    fn crouch_fits_tunnel_and_standing_does_not() {
        let world = World::new();
        let inside = vec3(19.75, 0.0, -16.0);
        assert!(world.solid(&Player::aabb_at(inside, STAND_H)));
        assert!(!world.solid(&Player::aabb_at(inside, CROUCH_H)));
    }

    #[test]
    fn mouse_counts_map_to_degrees() {
        let mut p = Player::new();
        p.look(vec2(360.0 / 0.022, 0.0), 0.022, false);
        assert!((p.yaw.to_degrees() - 360.0).abs() < 0.01);
    }
}
