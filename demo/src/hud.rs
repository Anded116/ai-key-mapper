use macroquad::camera::Camera;
use macroquad::prelude::*;

use crate::input::btn_name;
use crate::player::Stance;
use crate::settings::{MixedInput, MENU_LEN, YAW_DEG_PER_COUNT};
use crate::weapons::{Kind, DEFS};
use crate::world::{FLOOR_MAX, FLOOR_MIN, TARGET_HP};
use crate::{Game, BINDINGS};

const PANEL: Color = Color::new(0.05, 0.06, 0.08, 0.72);
const MODAL: Color = Color::new(0.05, 0.06, 0.08, 0.94);
const DIM: Color = Color::new(0.72, 0.75, 0.8, 1.0);
const ACCENT: Color = Color::new(0.4, 0.85, 1.0, 1.0);
const GOOD: Color = Color::new(0.55, 1.0, 0.55, 1.0);
const WARN: Color = Color::new(1.0, 0.65, 0.25, 1.0);
const STICK: Color = Color::new(1.0, 0.45, 0.85, 1.0);

fn modal(x: f32, y: f32, w: f32, h: f32) {
    draw_rectangle(x, y, w, h, MODAL);
}

fn panel(x: f32, y: f32, w: f32, h: f32) {
    draw_rectangle(x, y, w, h, PANEL);
}

const SWITCH_POPUP: f32 = 1.2;

#[derive(Clone, Copy)]
enum Glyph {
    Square,
    Cross,
    Circle,
    Triangle,
}

/// Face buttons in weapon-pair order (□ 1/2, ✕ 3/4, ○ 5/6, △ 7/8) with PlayStation colors.
const FACE_ARMS: [(Glyph, Color, Vec2); 4] = [
    (Glyph::Square, Color::new(0.95, 0.5, 0.8, 1.0), Vec2::new(-1.0, 0.0)),
    (Glyph::Cross, Color::new(0.45, 0.62, 1.0, 1.0), Vec2::new(0.0, 1.0)),
    (Glyph::Circle, Color::new(1.0, 0.38, 0.42, 1.0), Vec2::new(1.0, 0.0)),
    (Glyph::Triangle, Color::new(0.3, 0.85, 0.65, 1.0), Vec2::new(0.0, -1.0)),
];

fn draw_glyph(g: Glyph, x: f32, y: f32, r: f32, c: Color) {
    let t = (r * 0.3).max(1.5);
    match g {
        Glyph::Square => draw_rectangle_lines(x - r, y - r, r * 2.0, r * 2.0, t, c),
        Glyph::Circle => draw_circle_lines(x, y, r, t, c),
        Glyph::Cross => {
            draw_line(x - r, y - r, x + r, y + r, t, c);
            draw_line(x - r, y + r, x + r, y - r, t, c);
        }
        Glyph::Triangle => draw_triangle_lines(vec2(x, y - r), vec2(x - r, y + r * 0.8), vec2(x + r, y + r * 0.8), t, c),
    }
}

/// Compass angle of a 2D move vector: 0° = forward, positive = right.
fn angle_deg(v: Vec2) -> f32 {
    v.x.atan2(v.y).to_degrees()
}

impl Game {
    pub fn draw_hud(&self, now: f64) {
        let (w, h) = (screen_width(), screen_height());
        if self.flashlight {
            for i in 0..6 {
                let r = h * (0.18 + i as f32 * 0.06);
                draw_circle(w / 2.0, h / 2.0, r, Color::new(1.0, 0.95, 0.7, 0.035));
            }
        }
        self.draw_popups();
        let scoped = self.arms.cur == 5 && self.arms.ads > 0.85;
        if scoped {
            self.draw_scope(w, h);
        }
        self.draw_crosshair(now, w, h, scoped);
        self.draw_weapon_panel(now, w, h);
        let since_switch = (now - self.switched_at) as f32;
        if !self.show_inv && since_switch < SWITCH_POPUP {
            let alpha = ((SWITCH_POPUP - since_switch) / 0.3).min(1.0);
            self.draw_cross(w / 2.0, h * 0.7, 1.0, alpha);
        } else if !self.show_inv {
            self.draw_cross(14.0 + 250.0, h - 14.0 - 108.0 - 110.0, 0.72, 0.9);
        }
        self.draw_status_panel(now, h);
        self.draw_toasts(now, w);
        if self.s.show_debug {
            self.draw_debug(now, w);
        }
        if self.input.key(KeyCode::Tab) {
            self.draw_scoreboard(w, h);
        }
        if self.show_map {
            self.draw_map(w, h);
        }
        if self.show_inv {
            self.draw_inventory(w, h);
        }
        if self.show_help {
            self.draw_help(w, h);
        }
        if self.paused {
            self.draw_pause(w, h);
        } else if !self.grabbed && !self.show_help && !self.show_map && !self.show_inv {
            panel(w / 2.0 - 330.0, h / 2.0 + 60.0, 660.0, 70.0);
            self.txt.centered("Кликни ЛКМ, чтобы захватить мышь", w / 2.0, h / 2.0 + 92.0, 26.0, WHITE);
            self.txt.centered("Esc — пауза и настройки · F1 — раскладка", w / 2.0, h / 2.0 + 118.0, 18.0, DIM);
        }
    }

    fn project(&self, p: Vec3) -> Option<Vec2> {
        let c = self.cam.matrix() * p.extend(1.0);
        if c.w <= 0.0 {
            return None;
        }
        let n = c.truncate() / c.w;
        Some(vec2((n.x + 1.0) / 2.0 * screen_width(), (1.0 - n.y) / 2.0 * screen_height()))
    }

    fn draw_popups(&self) {
        for (p, text, color, _) in &self.fx.popups {
            if let Some(s) = self.project(*p) {
                self.txt.centered(text, s.x, s.y, 22.0, *color);
            }
        }
    }

    fn draw_crosshair(&self, now: f64, w: f32, h: f32, scoped: bool) {
        let (cx, cy) = (w / 2.0, h / 2.0);
        let d = self.arms.def();
        if !scoped {
            if matches!(d.kind, Kind::Melee { .. }) || self.arms.ads > 0.8 {
                draw_circle(cx, cy, 2.5, WHITE);
            } else {
                let moving = self.p.horizontal_speed() > 1.0;
                let spread = self.arms.current_spread(moving, !self.p.on_ground);
                let gap = spread.to_radians().tan() / (self.vfov / 2.0).tan() * h / 2.0 + 4.0 + self.arms.kick * 6.0;
                let len = 10.0;
                let c = Color::new(1.0, 1.0, 1.0, 0.9);
                draw_line(cx - gap - len, cy, cx - gap, cy, 2.0, c);
                draw_line(cx + gap, cy, cx + gap + len, cy, 2.0, c);
                draw_line(cx, cy - gap - len, cx, cy - gap, 2.0, c);
                draw_line(cx, cy + gap, cx, cy + gap + len, 2.0, c);
            }
        }
        if now < self.fx.hitmarker_until {
            let c = if self.fx.hitmarker_head { YELLOW } else { WHITE };
            for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                draw_line(cx + sx * 7.0, cy + sy * 7.0, cx + sx * 15.0, cy + sy * 15.0, 2.5, c);
            }
        }
    }

    fn draw_scope(&self, w: f32, h: f32) {
        let (cx, cy) = (w / 2.0, h / 2.0);
        let r = h * 0.42;
        let thick = w;
        draw_poly_lines(cx, cy, 96, r + thick / 2.0, 0.0, thick, BLACK);
        draw_line(cx - r, cy, cx + r, cy, 1.5, BLACK);
        draw_line(cx, cy - r, cx, cy + r, 1.5, BLACK);
        draw_circle_lines(cx, cy, r, 3.0, BLACK);
    }

    fn draw_weapon_panel(&self, now: f64, w: f32, h: f32) {
        let d = self.arms.def();
        let (pw, ph) = (360.0, 84.0);
        let (x, y) = (w - pw - 14.0, h - ph - 14.0);
        panel(x, y, pw, ph);
        self.txt.shadow(&format!("{}  {}", self.arms.cur + 1, d.name), x + 14.0, y + 30.0, 24.0, WHITE);
        let ammo = if d.mag == 0 { "∞".to_string() } else { format!("{} / {}", self.arms.ammo[self.arms.cur], d.mag) };
        self.txt.shadow(&ammo, x + 14.0, y + 66.0, 30.0, ACCENT);
        let mode = if d.auto { "авто" } else { "одиночный" };
        self.txt.draw(mode, x + 200.0, y + 66.0, 18.0, DIM);
        if let Some(t) = self.arms.reload_until {
            let k = 1.0 - ((t - now) / d.reload).clamp(0.0, 1.0) as f32;
            draw_rectangle(x + 14.0, y + 74.0, (pw - 28.0) * k, 5.0, WARN);
            self.txt.draw("перезарядка", x + 200.0, y + 40.0, 16.0, WARN);
        }
    }

    /// Weapon layout as it sits under R2: each face button holds two weapons,
    /// the first press gives the inner one, the second press the outer one.
    fn draw_cross(&self, cx: f32, cy: f32, s: f32, alpha: f32) {
        let (cw, ch, gap, pad) = (150.0 * s, 46.0 * s, 36.0 * s, 6.0 * s);
        let fade = |c: Color| Color::new(c.r, c.g, c.b, c.a * alpha);
        draw_circle(cx, cy, 24.0 * s, fade(PANEL));
        self.txt.centered("R2", cx, cy + 7.0 * s, 19.0 * s, fade(DIM));
        for (arm, (glyph, color, dir)) in FACE_ARMS.iter().enumerate() {
            let horizontal = dir.x != 0.0;
            for k in 0..2 {
                let slot = arm * 2 + k;
                let (x, y) = if horizontal {
                    (cx + dir.x * (gap + cw / 2.0 + k as f32 * (cw + pad)), cy)
                } else {
                    (cx, cy + dir.y * (gap + ch / 2.0 + k as f32 * (ch + pad)))
                };
                let active = slot == self.arms.cur;
                let bg = if active { Color::new(0.4, 0.85, 1.0, 0.9) } else { PANEL };
                draw_rectangle(x - cw / 2.0, y - ch / 2.0, cw, ch, fade(bg));
                draw_rectangle_lines(x - cw / 2.0, y - ch / 2.0, cw, ch, 1.5 * s, fade(Color::new(color.r, color.g, color.b, 0.6)));
                let text_c = if active { BLACK } else { WHITE };
                let d = &DEFS[slot];
                self.txt.draw(&format!("{} {}", slot + 1, d.short), x - cw / 2.0 + 10.0 * s, y + 6.0 * s, 18.0 * s, fade(text_c));
                for dot in 0..=k {
                    let dx = x + cw / 2.0 - 10.0 * s - dot as f32 * 9.0 * s;
                    draw_circle(dx, y - ch / 2.0 + 9.0 * s, 3.0 * s, fade(*color));
                }
            }
            let g = vec2(cx, cy) + *dir * (gap * 0.62);
            draw_glyph(*glyph, g.x, g.y, 7.0 * s, fade(*color));
        }
    }

    fn draw_status_panel(&self, now: f64, h: f32) {
        let (pw, ph) = (330.0, 108.0);
        let (x, y) = (14.0, h - ph - 14.0);
        panel(x, y, pw, ph);
        let stance = match self.p.stance {
            Stance::Run => "бег трусцой",
            Stance::Walk => "шаг",
            Stance::Sprint => "спринт",
            Stance::Crouch => "присед",
        };
        let air = if self.p.on_ground { "на земле" } else { "в воздухе" };
        self.txt.shadow(&format!("{stance} · {air}"), x + 14.0, y + 28.0, 22.0, WHITE);
        self.txt.draw(&format!("скорость {:.1} м/с", self.p.horizontal_speed()), x + 14.0, y + 52.0, 18.0, DIM);
        let gren = if now >= self.arms.grenade_ready { "граната готова" } else { "граната…" };
        let light = if self.flashlight { "фонарик ВКЛ" } else { "фонарик выкл" };
        self.txt.draw(&format!("{gren} · {light}"), x + 14.0, y + 74.0, 18.0, DIM);
        let (dev, c) = if self.pad_active { ("ГЕЙМПАД", STICK) } else { ("КЛАВИАТУРА+МЫШЬ", ACCENT) };
        self.txt.draw(&format!("подсказки: {dev}"), x + 14.0, y + 96.0, 18.0, c);
    }

    fn draw_toasts(&self, now: f64, w: f32) {
        for (i, (s, until)) in self.toasts.iter().enumerate() {
            let a = ((until - now) / 0.4).clamp(0.0, 1.0) as f32;
            if a <= 0.0 {
                continue;
            }
            self.txt.centered(s, w / 2.0, 120.0 + i as f32 * 30.0, 24.0, Color::new(1.0, 1.0, 1.0, a));
        }
    }

    fn draw_debug(&self, now: f64, w: f32) {
        let inp = &self.input;
        let (x, mut y) = (12.0, 12.0);
        let pw = 560.0;
        panel(x, y, pw, 426.0);
        y += 22.0;
        let line = |y: &mut f32, s: &str, c: Color| {
            self.txt.draw(s, x + 12.0, *y, 17.0, c);
            *y += 21.0;
        };
        line(&mut y, &format!("FPS {:.0}  ({:.2} мс)", self.fps, 1000.0 / self.fps.max(1.0)), DIM);
        let k = YAW_DEG_PER_COUNT * self.s.sens;
        let yaw = self.p.yaw.to_degrees();
        line(&mut y, &format!("Мышь: Δ {:+.0} / {:+.0} · {:.0} отсч/с", inp.delta.x, inp.delta.y, inp.counts_per_sec), WHITE);
        line(&mut y, &format!("Всего: X {:.0} · Y {:.0} отсчётов", inp.counts_total.x, inp.counts_total.y), DIM);
        line(
            &mut y,
            &format!("Yaw {:.1}° (всего {:+.1}°) · Pitch {:+.1}°", yaw.rem_euclid(360.0), yaw, self.p.pitch.to_degrees()),
            WHITE,
        );
        let turn = inp.turn_counts * k;
        let state = if inp.turn_done { "" } else { " …" };
        line(&mut y, &format!("Последний поворот: {turn:+.1}° = {:+.0} отсч.{state}", inp.turn_counts), GOOD);
        line(&mut y, "JoyShockMapper для этой игры:", ACCENT);
        line(&mut y, &format!("  REAL_WORLD_CALIBRATION = {:.4}", 1.0 / YAW_DEG_PER_COUNT), ACCENT);
        line(&mut y, &format!("  IN_GAME_SENS = {}", self.s.sens), ACCENT);
        line(&mut y, &format!("  360° = {:.0} отсчётов", 360.0 / k), DIM);

        let held: Vec<String> = inp
            .down
            .iter()
            .map(|(b, t)| format!("{} {:.2}с", btn_name(*b), now - t))
            .collect();
        let held = if held.is_empty() { "—".to_string() } else { held.join(", ") };
        line(&mut y, &format!("Зажато: {held}"), WHITE);

        y += 6.0;
        let pads = &self.pads;
        if let Some(e) = &pads.error {
            line(&mut y, &format!("gilrs: {e}"), WARN);
        }
        let n = pads.list.len();
        let c = if n == 0 { GOOD } else { WARN };
        line(&mut y, &format!("Геймпадов видит игра: {n}"), c);
        for p in pads.list.iter().take(2) {
            let id = match (p.vid, p.pid) {
                (Some(v), Some(pi)) => format!(" [{v:04X}:{pi:04X}]"),
                _ => String::new(),
            };
            line(&mut y, &format!("  {}{id}", p.name), STICK);
        }
        if n == 0 {
            line(&mut y, "  (скрыт HidHide или не подключён)", DIM);
        }
        let mode = match self.s.mixed {
            MixedInput::Simultaneous => "одновременно",
            MixedInput::LastDeviceWins => "последнее устройство",
        };
        line(&mut y, &format!("Смешанный ввод: {mode}"), DIM);
        line(&mut y, &format!("Смен устройства за 10 с: {}", self.dev_switches.len()), DIM);

        self.draw_stick_panel(x, 446.0);
        self.draw_log(now, w);
    }

    fn draw_stick_panel(&self, x: f32, y: f32) {
        let (pw, ph) = (560.0, 150.0);
        panel(x, y, pw, ph);
        let r = 58.0;
        let (cx, cy) = (x + 16.0 + r, y + ph / 2.0);
        draw_circle_lines(cx, cy, r, 1.5, DIM);
        draw_circle_lines(cx, cy, r * 0.15, 1.0, Color::new(1.0, 1.0, 1.0, 0.25));
        for i in 0..8 {
            let a = (i as f32 * 45.0 + 22.5).to_radians();
            draw_line(cx, cy, cx + a.sin() * r, cy - a.cos() * r, 1.0, Color::new(1.0, 1.0, 1.0, 0.12));
        }
        let keys = self.keys_vec;
        if keys != Vec2::ZERO {
            draw_line(cx, cy, cx + keys.x * r, cy - keys.y * r, 3.0, ACCENT);
        }
        let stick = self.pads.move_stick();
        if stick != Vec2::ZERO {
            draw_circle(cx + stick.x * r, cy - stick.y * r, 6.0, STICK);
        }
        let tx = cx + r + 18.0;
        let mut ty = y + 26.0;
        let mut line = |s: &str, c: Color| {
            self.txt.draw(s, tx, ty, 17.0, c);
            ty += 22.0;
        };
        line("Левый стик → WASD", WHITE);
        let fmt = |v: Vec2| {
            if v == Vec2::ZERO {
                "—".to_string()
            } else {
                format!("{:.2} ∠{:+.0}°", v.length(), angle_deg(v))
            }
        };
        line(&format!("WASD (что получает игра): {}", fmt(keys)), ACCENT);
        if self.pads.list.is_empty() {
            line("стик: игра его не видит", DIM);
        } else {
            line(&format!("стик (аналог): {}", fmt(stick)), STICK);
            if keys != Vec2::ZERO && stick != Vec2::ZERO {
                let da = (angle_deg(keys) - angle_deg(stick) + 540.0).rem_euclid(360.0) - 180.0;
                let dm = keys.length() - stick.length();
                line(&format!("ошибка: угол {da:+.0}°, сила {dm:+.2}"), WARN);
            }
        }
    }

    fn draw_log(&self, now: f64, w: f32) {
        let pw = 430.0;
        let x = w - pw - 12.0;
        let lines = self.input.log.len();
        panel(x, 12.0, pw, 40.0 + lines as f32 * 20.0);
        self.txt.draw("Лог ввода · tap < 150 мс · hold ≥ 150 мс", x + 12.0, 34.0, 17.0, DIM);
        for (i, e) in self.input.log.iter().enumerate() {
            let age = (now - e.t) as f32;
            let a = (1.0 - age / 12.0).clamp(0.4, 1.0);
            let c = Color::new(e.color.r, e.color.g, e.color.b, a);
            self.txt.draw(&format!("{:7.2}  {}", e.t, e.text), x + 12.0, 58.0 + i as f32 * 20.0, 16.0, c);
        }
    }

    fn draw_scoreboard(&self, w: f32, h: f32) {
        let (pw, ph) = (440.0, 330.0);
        let (x, y) = ((w - pw) / 2.0, (h - ph) / 2.0);
        modal(x, y, pw, ph);
        self.txt.centered("Статистика (Tab)", w / 2.0, y + 34.0, 26.0, WHITE);
        let st = &self.stats;
        let acc = if st.shots > 0 { st.hits as f32 / st.shots as f32 * 100.0 } else { 0.0 };
        let rows = [
            ("Выстрелы", st.shots.to_string()),
            ("Попадания", format!("{} ({acc:.0}%)", st.hits)),
            ("В голову", st.heads.to_string()),
            ("Убийства", st.kills.to_string()),
            ("Гранаты", st.grenades.to_string()),
            ("Удары", st.melee.to_string()),
            ("Перезарядки", st.reloads.to_string()),
            ("Смены оружия", st.switches.to_string()),
            ("Прыжки", self.p.jumps.to_string()),
        ];
        for (i, (k, v)) in rows.iter().enumerate() {
            let ry = y + 72.0 + i as f32 * 27.0;
            self.txt.draw(k, x + 40.0, ry, 20.0, DIM);
            self.txt.draw(v, x + 280.0, ry, 20.0, WHITE);
        }
    }

    fn draw_map(&self, w: f32, h: f32) {
        let ph = h - 120.0;
        let span = FLOOR_MAX - FLOOR_MIN;
        let scale = ph / span.y;
        let pw = span.x * scale;
        let (x, y) = ((w - pw) / 2.0, 60.0);
        modal(x - 10.0, y - 40.0, pw + 20.0, ph + 50.0);
        self.txt.centered("Карта (M)", w / 2.0, y - 14.0, 22.0, WHITE);
        let to = |p: Vec3| vec2(x + (p.x - FLOOR_MIN.x) * scale, y + (p.z - FLOOR_MIN.y) * scale);
        for bl in self.world.blocks.iter().skip(1) {
            let a = to(bl.b.min);
            let b = to(bl.b.max);
            draw_rectangle(a.x, a.y, (b.x - a.x).max(1.0), (b.y - a.y).max(1.0), bl.color);
        }
        for t in &self.world.targets {
            let p = to(t.feet);
            let k = t.hp / TARGET_HP;
            draw_circle(p.x, p.y, 3.0, Color::new(1.0, 0.4 * k, 0.2, 1.0));
        }
        let p = to(self.p.pos);
        let f = vec2(self.p.yaw.sin(), -self.p.yaw.cos());
        let r = vec2(-f.y, f.x);
        draw_triangle(p + f * 12.0, p - f * 6.0 + r * 6.0, p - f * 6.0 - r * 6.0, ACCENT);
    }

    fn draw_inventory(&self, w: f32, h: f32) {
        let (pw, ph) = (1000.0, 520.0);
        let (x, y) = ((w - pw) / 2.0, (h - ph) / 2.0);
        modal(x, y, pw, ph);
        self.txt.centered("Инвентарь (I) · R2 + кнопка: первое нажатие — внутреннее, второе — внешнее", w / 2.0, y + 36.0, 22.0, WHITE);
        self.draw_cross(w / 2.0, y + 250.0, 1.3, 1.0);
        let d = self.arms.def();
        let i = self.arms.cur;
        let ammo = if d.mag == 0 { "∞".to_string() } else { format!("{}/{}", self.arms.ammo[i], d.mag) };
        let mode = if d.auto { "авто" } else { "одиночный" };
        let line = format!("{}  {} · {ammo} · {mode} · {:.0} выстр/мин · прицел ×{:.1}", i + 1, d.name, d.rpm, d.zoom);
        self.txt.centered(&line, w / 2.0, y + ph - 36.0, 20.0, ACCENT);
    }

    fn draw_help(&self, w: f32, h: f32) {
        let rows = BINDINGS.len().div_ceil(2);
        let (pw, ph) = (1040.0, 80.0 + rows as f32 * 28.0);
        let (x, y) = ((w - pw) / 2.0, (h - ph) / 2.0);
        modal(x, y, pw, ph);
        self.txt.centered("Раскладка демки (F1)", w / 2.0, y + 34.0, 26.0, WHITE);
        for (i, (action, key)) in BINDINGS.iter().enumerate() {
            let col = (i / rows) as f32;
            let ry = y + 70.0 + (i % rows) as f32 * 28.0;
            let cx = x + 30.0 + col * 510.0;
            self.txt.draw(action, cx, ry, 19.0, DIM);
            self.txt.draw(key, cx + 330.0, ry, 19.0, WHITE);
        }
    }

    fn draw_pause(&self, w: f32, h: f32) {
        draw_rectangle(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.45));
        let (pw, ph) = (640.0, 120.0 + MENU_LEN as f32 * 34.0 + 70.0);
        let (x, y) = ((w - pw) / 2.0, (h - ph) / 2.0);
        modal(x, y, pw, ph);
        self.txt.centered("Пауза · настройки", w / 2.0, y + 40.0, 30.0, WHITE);
        for i in 0..MENU_LEN {
            let (label, value) = self.s.menu_item(i);
            let ry = y + 90.0 + i as f32 * 34.0;
            if i == self.menu_sel {
                draw_rectangle(x + 16.0, ry - 23.0, pw - 32.0, 32.0, Color::new(0.4, 0.85, 1.0, 0.18));
            }
            let c = if i == self.menu_sel { WHITE } else { DIM };
            self.txt.draw(label, x + 30.0, ry, 21.0, c);
            self.txt.draw(&value, x + 420.0, ry, 21.0, ACCENT);
        }
        let hy = y + ph - 50.0;
        self.txt.centered("↑↓ / W S — выбрать · ←→ / A D / Enter — изменить · Esc — продолжить", w / 2.0, hy, 17.0, DIM);
        self.txt.centered("Настройки сохраняются в testrange.cfg", w / 2.0, hy + 24.0, 16.0, DIM);
    }
}
