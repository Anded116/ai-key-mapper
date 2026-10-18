use macroquad::prelude::*;
use macroquad::rand::gen_range;

use crate::world::{Hit, World};

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Melee { range: f32 },
    Hitscan,
    Projectile { speed: f32, gravity: f32, splash: f32 },
}

pub struct Def {
    pub name: &'static str,
    pub short: &'static str,
    pub auto: bool,
    pub rpm: f32,
    pub mag: u32,
    pub reload: f64,
    pub dmg: f32,
    pub pellets: u32,
    /// Cone half-angle in degrees, hip and aimed.
    pub spread: f32,
    pub ads_spread: f32,
    pub zoom: f32,
    pub kind: Kind,
}

const BASE: Def = Def {
    name: "",
    short: "",
    auto: false,
    rpm: 600.0,
    mag: 30,
    reload: 2.0,
    dmg: 20.0,
    pellets: 1,
    spread: 1.5,
    ads_spread: 0.3,
    zoom: 1.3,
    kind: Kind::Hitscan,
};

/// Pairs per face button: □ 1/2, ✕ 3/4, ○ 5/6, △ 7/8 (first press / second press).
pub const DEFS: [Def; 8] = [
    Def { name: "Нож", short: "Нож", rpm: 120.0, mag: 0, reload: 0.0, dmg: 55.0, spread: 0.0, ads_spread: 0.0, zoom: 1.0, kind: Kind::Melee { range: 2.3 }, ..BASE },
    Def { name: "Пистолет", short: "Пистолет", rpm: 420.0, mag: 12, reload: 1.2, dmg: 25.0, spread: 1.2, ads_spread: 0.2, ..BASE },
    Def { name: "Дробовик", short: "Дробовик", rpm: 70.0, mag: 6, dmg: 11.0, pellets: 9, spread: 5.0, ads_spread: 3.5, zoom: 1.2, ..BASE },
    Def { name: "Пистолет-пулемёт", short: "ПП", auto: true, rpm: 900.0, reload: 1.6, dmg: 14.0, spread: 2.5, ads_spread: 1.0, ..BASE },
    Def { name: "Автомат", short: "Автомат", auto: true, rpm: 650.0, dmg: 22.0, spread: 1.6, ads_spread: 0.15, zoom: 1.6, ..BASE },
    Def { name: "Снайперская винтовка", short: "Снайперка", rpm: 45.0, mag: 5, reload: 2.6, dmg: 100.0, spread: 6.0, ads_spread: 0.0, zoom: 4.0, ..BASE },
    Def {
        name: "Ракетомёт",
        short: "Ракетомёт",
        rpm: 60.0,
        mag: 1,
        reload: 1.6,
        dmg: 110.0,
        spread: 0.0,
        ads_spread: 0.0,
        zoom: 1.2,
        kind: Kind::Projectile { speed: 35.0, gravity: 0.0, splash: 4.0 },
        ..BASE
    },
    Def { name: "Пулемёт", short: "Пулемёт", auto: true, rpm: 1100.0, mag: 100, reload: 4.0, dmg: 12.0, spread: 3.0, ads_spread: 1.2, ..BASE },
];

pub const SWITCH_TIME: f64 = 0.25;
const GRENADE_COOLDOWN: f64 = 0.8;
const MELEE_COOLDOWN: f64 = 0.5;
/// From the button press to the blade crossing the centre of the screen.
const STRIKE_DELAY: f64 = 0.12;

pub struct Arsenal {
    pub cur: usize,
    pub ammo: [u32; 8],
    pub next_fire: f64,
    pub reload_until: Option<f64>,
    pub switch_until: f64,
    pub ads: f32,
    pub aiming: bool,
    pub grenade_ready: f64,
    pub melee_until: f64,
    pub kick: f32,
    /// Start of the last knife swing and grenade throw, for the viewmodel.
    pub swing_at: f64,
    pub throw_at: f64,
    /// Melee hit waiting for the swing to reach the target: (time, range, damage).
    pub pending_strike: Option<(f64, f32, f32)>,
}

#[derive(Default)]
pub struct Stats {
    pub shots: u32,
    pub hits: u32,
    pub heads: u32,
    pub kills: u32,
    pub grenades: u32,
    pub melee: u32,
    pub reloads: u32,
    pub switches: u32,
}

pub struct Projectile {
    pub pos: Vec3,
    pub vel: Vec3,
    pub gravity: f32,
    pub splash: f32,
    pub dmg: f32,
    pub fuse: Option<f64>,
}

pub struct Fx {
    pub tracers: Vec<(Vec3, Vec3, f64)>,
    pub impacts: Vec<(Vec3, f64)>,
    pub explosions: Vec<(Vec3, f32, f64)>,
    pub popups: Vec<(Vec3, String, Color, f64)>,
    pub pings: Vec<(Vec3, f64)>,
    pub projectiles: Vec<Projectile>,
    pub hitmarker_until: f64,
    pub hitmarker_head: bool,
    pub muzzle_until: f64,
}

impl Fx {
    pub fn new() -> Self {
        Self {
            tracers: Vec::new(),
            impacts: Vec::new(),
            explosions: Vec::new(),
            popups: Vec::new(),
            pings: Vec::new(),
            projectiles: Vec::new(),
            hitmarker_until: 0.0,
            hitmarker_head: false,
            muzzle_until: 0.0,
        }
    }
}

impl Arsenal {
    pub fn new() -> Self {
        let mut ammo = [0; 8];
        for (i, d) in DEFS.iter().enumerate() {
            ammo[i] = d.mag;
        }
        Self {
            cur: 1,
            ammo,
            next_fire: 0.0,
            reload_until: None,
            switch_until: 0.0,
            ads: 0.0,
            aiming: false,
            grenade_ready: 0.0,
            melee_until: 0.0,
            kick: 0.0,
            swing_at: -10.0,
            throw_at: -10.0,
            pending_strike: None,
        }
    }

    pub fn def(&self) -> &'static Def {
        &DEFS[self.cur]
    }

    pub fn select(&mut self, now: f64, slot: usize, stats: &mut Stats) -> bool {
        if slot == self.cur || slot >= DEFS.len() {
            return false;
        }
        self.cur = slot;
        self.reload_until = None;
        self.switch_until = now + SWITCH_TIME;
        stats.switches += 1;
        true
    }

    pub fn start_reload(&mut self, now: f64, stats: &mut Stats) -> bool {
        let d = self.def();
        if d.mag == 0 || self.ammo[self.cur] == d.mag || self.reload_until.is_some() {
            return false;
        }
        self.reload_until = Some(now + d.reload);
        stats.reloads += 1;
        true
    }

    pub fn update(&mut self, now: f64, dt: f32) {
        if let Some(t) = self.reload_until {
            if now >= t {
                self.ammo[self.cur] = self.def().mag;
                self.reload_until = None;
            }
        }
        let target = if self.aiming && self.reload_until.is_none() { 1.0 } else { 0.0 };
        let step = 7.0 * dt;
        self.ads += (target - self.ads).clamp(-step, step);
        self.kick = (self.kick - dt * 6.0).max(0.0);
    }

    pub fn busy(&self, now: f64) -> bool {
        now < self.switch_until || self.reload_until.is_some() || now < self.melee_until
    }

    pub fn current_spread(&self, moving: bool, airborne: bool) -> f32 {
        let d = self.def();
        let mut s = d.spread + (d.ads_spread - d.spread) * self.ads;
        if moving {
            s *= 1.5;
        }
        if airborne {
            s *= 2.0;
        }
        s
    }

    /// Fires the current weapon. Returns false when nothing was fired.
    pub fn fire(&mut self, now: f64, eye: Vec3, dir: Vec3, spread: f32, world: &mut World, fx: &mut Fx, stats: &mut Stats) -> bool {
        if self.busy(now) || now < self.next_fire {
            return false;
        }
        let d = self.def();
        if d.mag > 0 && self.ammo[self.cur] == 0 {
            self.start_reload(now, stats);
            return false;
        }
        self.next_fire = now + 60.0 / d.rpm as f64;
        if d.mag > 0 {
            self.ammo[self.cur] -= 1;
        }
        match d.kind {
            Kind::Melee { range } => {
                stats.melee += 1;
                self.swing_at = now;
                self.pending_strike = Some((now + STRIKE_DELAY, range, d.dmg));
            }
            Kind::Hitscan => {
                stats.shots += 1;
                fx.muzzle_until = now + 0.04;
                self.kick = 1.0;
                let mut any = false;
                for _ in 0..d.pellets {
                    let pdir = jitter(dir, spread);
                    any |= hitscan(now, eye, pdir, d.dmg, world, fx, stats);
                }
                if any {
                    stats.hits += 1;
                }
            }
            Kind::Projectile { speed, gravity, splash } => {
                stats.shots += 1;
                fx.muzzle_until = now + 0.06;
                self.kick = 1.0;
                fx.projectiles.push(Projectile { pos: eye + dir * 0.6, vel: dir * speed, gravity, splash, dmg: d.dmg, fuse: None });
            }
        }
        if d.mag > 0 && self.ammo[self.cur] == 0 {
            self.start_reload(now, stats);
        }
        true
    }

    pub fn quick_melee(&mut self, now: f64, stats: &mut Stats) -> bool {
        if now < self.melee_until || now < self.switch_until {
            return false;
        }
        self.melee_until = now + MELEE_COOLDOWN;
        self.swing_at = now;
        self.pending_strike = Some((now + STRIKE_DELAY, 2.3, 55.0));
        self.reload_until = None;
        stats.melee += 1;
        true
    }

    /// Melee damage lands when the blade crosses the screen, not on the button press.
    pub fn resolve_strike(&mut self, now: f64, eye: Vec3, dir: Vec3, world: &mut World, fx: &mut Fx, stats: &mut Stats) {
        if let Some((at, range, dmg)) = self.pending_strike {
            if now >= at {
                self.pending_strike = None;
                strike(now, eye, dir, range, dmg, world, fx, stats);
            }
        }
    }

    pub fn throw_grenade(&mut self, now: f64, eye: Vec3, dir: Vec3, fx: &mut Fx, stats: &mut Stats) -> bool {
        if now < self.grenade_ready {
            return false;
        }
        self.grenade_ready = now + GRENADE_COOLDOWN;
        self.throw_at = now;
        stats.grenades += 1;
        let v = (dir + vec3(0.0, 0.25, 0.0)).normalize() * 18.0;
        fx.projectiles.push(Projectile { pos: eye + dir * 0.5, vel: v, gravity: 14.0, splash: 5.0, dmg: 120.0, fuse: Some(now + 2.0) });
        true
    }
}

fn jitter(dir: Vec3, deg: f32) -> Vec3 {
    if deg <= 0.0 {
        return dir;
    }
    let right = dir.cross(Vec3::Y).normalize_or(Vec3::X);
    let up = right.cross(dir);
    let r = deg.to_radians().tan() * gen_range(0.0f32, 1.0).sqrt();
    let a = gen_range(0.0, std::f32::consts::TAU);
    (dir + right * r * a.cos() + up * r * a.sin()).normalize()
}

fn register_hit(now: f64, world: &mut World, fx: &mut Fx, stats: &mut Stats, idx: usize, head: bool, dmg: f32, at: Vec3) {
    let dmg = if head { dmg * 2.0 } else { dmg };
    if head {
        stats.heads += 1;
    }
    if world.targets[idx].damage(now, dmg) {
        stats.kills += 1;
    }
    fx.hitmarker_until = now + 0.15;
    fx.hitmarker_head = head;
    let color = if head { Color::new(1.0, 0.85, 0.2, 1.0) } else { WHITE };
    fx.popups.push((at, format!("{dmg:.0}"), color, now + 0.7));
}

fn hitscan(now: f64, eye: Vec3, dir: Vec3, dmg: f32, world: &mut World, fx: &mut Fx, stats: &mut Stats) -> bool {
    let muzzle = eye + dir * 0.4 + vec3(0.0, -0.12, 0.0);
    let Some((t, hit)) = world.raycast(now, eye, dir, 500.0) else {
        fx.tracers.push((muzzle, eye + dir * 500.0, now + 0.05));
        return false;
    };
    let at = eye + dir * t;
    fx.tracers.push((muzzle, at, now + 0.05));
    match hit {
        Hit::Block => {
            fx.impacts.push((at, now + 1.5));
            false
        }
        Hit::Body(i) => {
            register_hit(now, world, fx, stats, i, false, dmg, at);
            true
        }
        Hit::Head(i) => {
            register_hit(now, world, fx, stats, i, true, dmg, at);
            true
        }
    }
}

fn strike(now: f64, eye: Vec3, dir: Vec3, range: f32, dmg: f32, world: &mut World, fx: &mut Fx, stats: &mut Stats) {
    match world.raycast(now, eye, dir, range) {
        Some((t, Hit::Body(i))) => register_hit(now, world, fx, stats, i, false, dmg, eye + dir * t),
        Some((t, Hit::Head(i))) => register_hit(now, world, fx, stats, i, true, dmg, eye + dir * t),
        Some((t, Hit::Block)) => fx.impacts.push((eye + dir * t, now + 1.0)),
        None => {}
    }
}

fn explode(now: f64, at: Vec3, splash: f32, dmg: f32, world: &mut World, fx: &mut Fx, stats: &mut Stats) {
    fx.explosions.push((at, splash, now + 0.35));
    let mut any = false;
    for i in 0..world.targets.len() {
        if !world.targets[i].alive(now) {
            continue;
        }
        let c = world.targets[i].body().center();
        let dist = c.distance(at);
        if dist < splash {
            any = true;
            let d = dmg * (1.0 - dist / splash).max(0.2);
            if world.targets[i].damage(now, d) {
                stats.kills += 1;
            }
            fx.popups.push((c + vec3(0.0, 1.2, 0.0), format!("{d:.0}"), ORANGE, now + 0.7));
        }
    }
    if any {
        stats.hits += 1;
        fx.hitmarker_until = now + 0.15;
        fx.hitmarker_head = false;
    }
}

pub fn update_fx(now: f64, dt: f32, world: &mut World, fx: &mut Fx, stats: &mut Stats) {
    fx.tracers.retain(|t| now < t.2);
    fx.impacts.retain(|t| now < t.1);
    fx.explosions.retain(|t| now < t.2);
    fx.popups.retain(|t| now < t.3);
    fx.pings.retain(|t| now < t.1);
    for p in &mut fx.popups {
        p.0.y += dt * 0.8;
    }

    let mut boom = Vec::new();
    let mut i = 0;
    while i < fx.projectiles.len() {
        let p = &mut fx.projectiles[i];
        p.vel.y -= p.gravity * dt;
        let next = p.pos + p.vel * dt;
        let hits_target = world
            .targets
            .iter()
            .any(|t| t.alive(now) && (t.body().contains(next) || t.head().contains(next)));
        let hits_block = world.point_solid(next);
        let expired = p.fuse.is_some_and(|f| now >= f) || next.length() > 400.0;
        match p.fuse {
            // Grenades stop on the ground and wait for the fuse, unless they hit a target.
            Some(_) if hits_block && !hits_target => {
                p.vel = Vec3::ZERO;
                p.gravity = 0.0;
            }
            _ => p.pos = next,
        }
        let detonate = hits_target || expired || (p.fuse.is_none() && hits_block);
        if detonate {
            let p = fx.projectiles.swap_remove(i);
            boom.push((p.pos, p.splash, p.dmg));
        } else {
            i += 1;
        }
    }
    for (at, splash, dmg) in boom {
        explode(now, at, splash, dmg, world, fx, stats);
    }
}

pub fn draw_fx(now: f64, fx: &Fx) {
    for (a, b, _) in &fx.tracers {
        draw_line_3d(*a, *b, Color::new(1.0, 0.9, 0.5, 0.8));
    }
    for (p, _) in &fx.impacts {
        draw_cube(*p, vec3(0.06, 0.06, 0.06), None, Color::new(0.1, 0.1, 0.1, 0.9));
    }
    for p in &fx.projectiles {
        let c = if p.fuse.is_some() { DARKGREEN } else { ORANGE };
        draw_sphere(p.pos, 0.12, None, c);
    }
    for (p, r, until) in &fx.explosions {
        let k = 1.0 - ((until - now) / 0.35) as f32;
        draw_sphere(*p, r * (0.3 + 0.7 * k), None, Color::new(1.0, 0.55, 0.1, 0.55 * (1.0 - k)));
    }
    for (p, _) in &fx.pings {
        draw_cube(*p + vec3(0.0, 1.5, 0.0), vec3(0.1, 3.0, 0.1), None, Color::new(0.2, 1.0, 1.0, 0.7));
    }
}
