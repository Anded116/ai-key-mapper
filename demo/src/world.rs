use macroquad::prelude::*;

#[derive(Clone, Copy)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    pub fn centered(c: Vec3, size: Vec3) -> Self {
        Self { min: c - size / 2.0, max: c + size / 2.0 }
    }

    pub fn center(&self) -> Vec3 {
        (self.min + self.max) / 2.0
    }

    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }

    pub fn overlaps(&self, o: &Aabb) -> bool {
        self.min.x < o.max.x
            && self.max.x > o.min.x
            && self.min.y < o.max.y
            && self.max.y > o.min.y
            && self.min.z < o.max.z
            && self.max.z > o.min.z
    }

    pub fn contains(&self, p: Vec3) -> bool {
        p.cmpgt(self.min).all() && p.cmplt(self.max).all()
    }

    /// Slab test, returns distance along `dir` (unit) to the entry point.
    pub fn ray(&self, origin: Vec3, dir: Vec3) -> Option<f32> {
        let inv = dir.recip();
        let t1 = (self.min - origin) * inv;
        let t2 = (self.max - origin) * inv;
        let tmin = t1.min(t2).max_element();
        let tmax = t1.max(t2).min_element();
        (tmax >= tmin.max(0.0)).then_some(tmin.max(0.0))
    }

    pub fn draw(&self, color: Color) {
        crate::render::draw_box(self.center(), self.size(), color);
        draw_cube_wires(self.center(), self.size(), Color::new(0.0, 0.0, 0.0, 0.12));
    }
}

pub struct Block {
    pub b: Aabb,
    pub color: Color,
}

pub struct Target {
    pub origin: Vec3,
    pub feet: Vec3,
    pub hp: f32,
    pub dead_until: f64,
    pub flash_until: f64,
    pub mover: Option<(f32, f32)>,
}

pub const TARGET_HP: f32 = 100.0;
const RESPAWN: f64 = 2.0;

impl Target {
    fn new(feet: Vec3, mover: Option<(f32, f32)>) -> Self {
        Self { origin: feet, feet, hp: TARGET_HP, dead_until: 0.0, flash_until: 0.0, mover }
    }

    pub fn alive(&self, now: f64) -> bool {
        now >= self.dead_until
    }

    pub fn body(&self) -> Aabb {
        Aabb::centered(self.feet + vec3(0.0, 0.75, 0.0), vec3(0.6, 1.5, 0.35))
    }

    pub fn head(&self) -> Aabb {
        Aabb::centered(self.feet + vec3(0.0, 1.73, 0.0), vec3(0.32, 0.36, 0.32))
    }

    /// Returns true when this hit killed the target.
    pub fn damage(&mut self, now: f64, dmg: f32) -> bool {
        self.flash_until = now + 0.08;
        self.hp -= dmg;
        if self.hp <= 0.0 {
            self.hp = TARGET_HP;
            self.dead_until = now + RESPAWN;
            return true;
        }
        false
    }
}

pub enum Hit {
    Block,
    Body(usize),
    Head(usize),
}

pub struct World {
    pub blocks: Vec<Block>,
    pub targets: Vec<Target>,
    pub button: Aabb,
    pub button_on: bool,
    pub pillars: Vec<(Vec3, Color)>,
}

pub const SPAWN: Vec3 = Vec3::new(0.0, 0.0, 0.0);
pub const FLOOR_MIN: Vec2 = Vec2::new(-60.0, -200.0);
pub const FLOOR_MAX: Vec2 = Vec2::new(60.0, 40.0);

impl World {
    pub fn new() -> Self {
        let mut blocks = Vec::new();
        let mut add = |min: Vec3, max: Vec3, color: Color| blocks.push(Block { b: Aabb::new(min, max), color });

        let wall = Color::new(0.42, 0.45, 0.5, 1.0);
        add(vec3(FLOOR_MIN.x, -1.0, FLOOR_MIN.y), vec3(FLOOR_MAX.x, 0.0, FLOOR_MAX.y), Color::new(0.3, 0.32, 0.34, 1.0));
        add(vec3(FLOOR_MIN.x - 1.0, 0.0, FLOOR_MIN.y), vec3(FLOOR_MIN.x, 6.0, FLOOR_MAX.y), wall);
        add(vec3(FLOOR_MAX.x, 0.0, FLOOR_MIN.y), vec3(FLOOR_MAX.x + 1.0, 6.0, FLOOR_MAX.y), wall);
        add(vec3(FLOOR_MIN.x, 0.0, FLOOR_MIN.y - 1.0), vec3(FLOOR_MAX.x, 12.0, FLOOR_MIN.y), wall);
        add(vec3(FLOOR_MIN.x, 0.0, FLOOR_MAX.y), vec3(FLOOR_MAX.x, 6.0, FLOOR_MAX.y + 1.0), wall);

        // Lane separators and cover in the shooting range.
        let cover = Color::new(0.55, 0.5, 0.42, 1.0);
        for z in [-18.0, -40.0, -75.0] {
            add(vec3(-11.0, 0.0, z), vec3(-9.0, 1.0, z + 0.6), cover);
            add(vec3(9.0, 0.0, z), vec3(11.0, 1.0, z + 0.6), cover);
        }
        add(vec3(-14.0, 0.0, -108.0), vec3(14.0, 8.0, -106.0), wall);

        // Parkour: steps for jump height, a stack, and a crouch tunnel.
        let parkour = Color::new(0.35, 0.55, 0.75, 1.0);
        add(vec3(18.0, 0.0, -6.0), vec3(21.0, 0.5, -3.0), parkour);
        add(vec3(22.0, 0.0, -6.0), vec3(25.0, 1.0, -3.0), parkour);
        add(vec3(26.0, 0.0, -6.0), vec3(29.0, 1.6, -3.0), Color::new(0.6, 0.35, 0.35, 1.0));
        add(vec3(30.0, 0.0, -6.0), vec3(33.0, 1.0, -3.0), parkour);
        add(vec3(30.0, 1.0, -6.0), vec3(33.0, 2.0, -4.5), parkour);
        add(vec3(30.0, 2.0, -6.0), vec3(33.0, 3.0, -5.2), parkour);
        let tunnel = Color::new(0.5, 0.42, 0.62, 1.0);
        add(vec3(18.0, 0.0, -20.0), vec3(18.5, 1.3, -12.0), tunnel);
        add(vec3(20.5, 0.0, -20.0), vec3(21.0, 1.3, -12.0), tunnel);
        add(vec3(18.0, 1.3, -20.0), vec3(21.0, 1.8, -12.0), tunnel);

        // Use-button on a post behind-left of spawn.
        add(vec3(-5.3, 0.0, 3.7), vec3(-4.7, 1.0, 4.3), wall);
        let button = Aabb::new(vec3(-5.2, 1.0, 3.8), vec3(-4.8, 1.25, 4.2));

        let mut targets = Vec::new();
        for z in [-10.0, -25.0, -50.0, -100.0] {
            for x in [-8.0, -4.0, 0.0, 4.0, 8.0] {
                targets.push(Target::new(vec3(x, 0.0, z), None));
            }
        }
        for (z, speed) in [(-30.0, 0.5), (-33.0, 1.0), (-36.0, 1.8)] {
            targets.push(Target::new(vec3(0.0, 0.0, z), Some((7.0, speed))));
        }
        // Flick targets around spawn every 45° (0° is the range itself).
        for i in 1..8 {
            let a = (i as f32 * 45.0).to_radians();
            targets.push(Target::new(vec3(a.sin() * 12.0, 0.0, -a.cos() * 12.0), None));
        }

        let colors = [RED, ORANGE, YELLOW, GREEN, SKYBLUE, BLUE, VIOLET, PINK];
        let pillars = (0..8)
            .map(|i| {
                let a = (i as f32 * 45.0).to_radians();
                (vec3(a.sin() * 35.0, 0.0, -a.cos() * 35.0), colors[i])
            })
            .collect();

        Self { blocks, targets, button, button_on: false, pillars }
    }

    pub fn update(&mut self, now: f64) {
        for t in &mut self.targets {
            if let Some((amp, speed)) = t.mover {
                t.feet.x = t.origin.x + amp * (now as f32 * speed).sin();
            }
        }
    }

    pub fn solid(&self, b: &Aabb) -> bool {
        self.blocks.iter().any(|bl| bl.b.overlaps(b))
    }

    pub fn point_solid(&self, p: Vec3) -> bool {
        self.blocks.iter().any(|bl| bl.b.contains(p))
    }

    /// Nearest hit along a ray within `max` meters.
    pub fn raycast(&self, now: f64, origin: Vec3, dir: Vec3, max: f32) -> Option<(f32, Hit)> {
        let mut best: Option<(f32, Hit)> = None;
        let mut consider = |t: f32, h: Hit| {
            if t <= max && best.as_ref().is_none_or(|(bt, _)| t < *bt) {
                best = Some((t, h));
            }
        };
        for bl in &self.blocks {
            if let Some(t) = bl.b.ray(origin, dir) {
                consider(t, Hit::Block);
            }
        }
        for (i, tg) in self.targets.iter().enumerate() {
            if !tg.alive(now) {
                continue;
            }
            if let Some(t) = tg.head().ray(origin, dir) {
                consider(t, Hit::Head(i));
            }
            if let Some(t) = tg.body().ray(origin, dir) {
                consider(t, Hit::Body(i));
            }
        }
        best
    }

    pub fn draw(&self, now: f64) {
        for bl in &self.blocks {
            bl.b.draw(bl.color);
        }
        draw_grid_at_floor();
        for (p, c) in &self.pillars {
            crate::render::draw_box(*p + vec3(0.0, 5.0, 0.0), vec3(0.8, 10.0, 0.8), *c);
        }
        let bc = if self.button_on { GREEN } else { RED };
        self.button.draw(bc);
        for t in &self.targets {
            if !t.alive(now) {
                draw_cube_wires(t.body().center(), t.body().size(), Color::new(1.0, 1.0, 1.0, 0.15));
                continue;
            }
            let flash = now < t.flash_until;
            let k = t.hp / TARGET_HP;
            let body = if flash { WHITE } else { Color::new(0.9, 0.35 + 0.25 * k, 0.15, 1.0) };
            let head = if flash { WHITE } else { Color::new(0.95, 0.8, 0.6, 1.0) };
            t.body().draw(body);
            t.head().draw(head);
        }
    }
}

fn draw_grid_at_floor() {
    let c = Color::new(1.0, 1.0, 1.0, 0.07);
    let y = 0.005;
    let mut x = FLOOR_MIN.x;
    while x <= FLOOR_MAX.x {
        draw_line_3d(vec3(x, y, FLOOR_MIN.y), vec3(x, y, FLOOR_MAX.y), c);
        x += 2.0;
    }
    let mut z = FLOOR_MIN.y;
    while z <= FLOOR_MAX.y {
        draw_line_3d(vec3(FLOOR_MIN.x, y, z), vec3(FLOOR_MAX.x, y, z), c);
        z += 2.0;
    }
    for d in [10.0, 25.0, 50.0, 100.0] {
        draw_line_3d(vec3(-12.0, y, -d), vec3(12.0, y, -d), Color::new(1.0, 1.0, 0.3, 0.5));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_hits_first_range_target_head_first() {
        let w = World::new();
        let eye = vec3(0.0, 1.73, 0.0);
        let (t, hit) = w.raycast(0.0, eye, vec3(0.0, 0.0, -1.0), 500.0).unwrap();
        assert!(matches!(hit, Hit::Head(_)));
        assert!((t - 9.84).abs() < 0.05, "t {t}");
    }

    #[test]
    fn ray_misses_box_behind_origin() {
        let b = Aabb::new(vec3(-1.0, -1.0, 2.0), vec3(1.0, 1.0, 3.0));
        assert!(b.ray(Vec3::ZERO, vec3(0.0, 0.0, -1.0)).is_none());
        assert_eq!(b.ray(Vec3::ZERO, vec3(0.0, 0.0, 1.0)), Some(2.0));
    }
}
