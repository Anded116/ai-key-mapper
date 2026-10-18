#![allow(clippy::too_many_arguments)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ds;
mod hud;
mod input;
mod pad;
mod player;
mod render;
mod settings;
mod text;
mod viewmodel;
mod weapons;
mod world;

use std::collections::VecDeque;

use macroquad::miniquad::conf::Platform;
use macroquad::prelude::*;

use input::{Btn, Input};
use pad::Pads;
use player::{MoveIntent, Player, Stance};
use settings::{AnalogMode, MixedInput, Settings, MENU_LEN, WALK_THRESHOLD, YAW_DEG_PER_COUNT};
use text::Txt;
use weapons::{Arsenal, Fx, Stats, DEFS};
use world::World;

const SLOT_KEYS: [KeyCode; 8] = [
    KeyCode::Key1,
    KeyCode::Key2,
    KeyCode::Key3,
    KeyCode::Key4,
    KeyCode::Key5,
    KeyCode::Key6,
    KeyCode::Key7,
    KeyCode::Key8,
];

pub const BINDINGS: &[(&str, &str)] = &[
    ("Движение", "W A S D"),
    ("Прыжок", "Space"),
    ("Присед", "LCtrl / C"),
    ("Бег", "LShift"),
    ("Шаг", "LAlt"),
    ("Огонь", "ЛКМ"),
    ("Прицел", "ПКМ"),
    ("Перезарядка", "R"),
    ("Использовать", "E"),
    ("Удар", "V"),
    ("Граната", "G"),
    ("Фонарик", "F"),
    ("Оружие 1–8", "1 … 8"),
    ("Метка", "СКМ"),
    ("Статистика", "Tab (держать)"),
    ("Карта", "M"),
    ("Инвентарь", "I"),
    ("Пауза / настройки", "Esc"),
    ("Быстрое сохранение / загрузка", "F5 / F9"),
    ("Справка / отладка", "F1 / F2"),
    ("Сброс счётчиков", "F8"),
    ("Полный экран", "F11"),
];

#[cfg(windows)]
fn window_focused() -> bool {
    #[link(name = "user32")]
    extern "system" {
        fn GetActiveWindow() -> *mut std::ffi::c_void;
    }
    // Null when none of this thread's windows is active.
    unsafe { !GetActiveWindow().is_null() }
}

#[cfg(not(windows))]
fn window_focused() -> bool {
    true
}

fn action_of(b: Btn) -> Option<&'static str> {
    use KeyCode::*;
    let name = match b {
        Btn::Key(W) => "вперёд",
        Btn::Key(S) => "назад",
        Btn::Key(A) => "влево",
        Btn::Key(D) => "вправо",
        Btn::Key(Space) => "прыжок",
        Btn::Key(LeftControl) | Btn::Key(C) => "присед",
        Btn::Key(LeftShift) => "бег",
        Btn::Key(LeftAlt) => "шаг",
        Btn::Key(R) => "перезарядка",
        Btn::Key(E) => "использовать",
        Btn::Key(V) => "удар",
        Btn::Key(G) => "граната",
        Btn::Key(F) => "фонарик",
        Btn::Key(Key1) => "оружие 1",
        Btn::Key(Key2) => "оружие 2",
        Btn::Key(Key3) => "оружие 3",
        Btn::Key(Key4) => "оружие 4",
        Btn::Key(Key5) => "оружие 5",
        Btn::Key(Key6) => "оружие 6",
        Btn::Key(Key7) => "оружие 7",
        Btn::Key(Key8) => "оружие 8",
        Btn::Key(Tab) => "статистика",
        Btn::Key(M) => "карта",
        Btn::Key(I) => "инвентарь",
        Btn::Key(Escape) => "пауза",
        Btn::Key(F5) => "быстрое сохранение",
        Btn::Key(F9) => "быстрая загрузка",
        Btn::Key(F1) => "справка",
        Btn::Key(F2) => "отладка",
        Btn::Key(F8) => "сброс счётчиков",
        Btn::Key(F11) => "полный экран",
        Btn::Mouse(MouseButton::Left) => "огонь",
        Btn::Mouse(MouseButton::Right) => "прицел",
        Btn::Mouse(MouseButton::Middle) => "метка",
        _ => return None,
    };
    Some(name)
}

pub struct Game {
    pub txt: Txt,
    pub s: Settings,
    pub input: Input,
    pub pads: Pads,
    pub world: World,
    pub p: Player,
    pub arms: Arsenal,
    pub fx: Fx,
    pub stats: Stats,
    pub grabbed: bool,
    pub focused: bool,
    pub paused: bool,
    pub fullscreen: bool,
    pub menu_sel: usize,
    pub show_help: bool,
    pub show_map: bool,
    pub show_inv: bool,
    pub flashlight: bool,
    pub crouch_on: bool,
    pub sprint_on: bool,
    pub toasts: VecDeque<(String, f64)>,
    pub quicksave: Option<(Vec3, f32, f32, usize)>,
    pub pad_active: bool,
    pub dev_switches: VecDeque<f64>,
    pub keys_vec: Vec2,
    pub stick_vec: Vec2,
    pub fps: f32,
    pub cam: Camera3D,
    pub vfov: f32,
    /// Weapon lag behind fast turns, radians (yaw, pitch).
    pub sway: Vec2,
    pub sprint_k: f32,
    pub switched_at: f64,
    pub force_ads: bool,
    viewmodel_rt: Option<RenderTarget>,
}

impl Game {
    fn toast(&mut self, now: f64, s: impl Into<String>) {
        self.toasts.push_front((s.into(), now + 1.6));
        self.toasts.truncate(4);
    }

    fn set_grab(&mut self, on: bool) {
        self.grabbed = on;
        set_cursor_grab(on);
        show_mouse(!on);
    }

    fn meta_keys(&mut self, now: f64) {
        let inp = &self.input;
        if inp.key_hit(KeyCode::Escape) {
            self.paused = !self.paused;
            let grab = !self.paused;
            self.set_grab(grab);
            return;
        }
        if inp.key_hit(KeyCode::F1) {
            self.show_help = !self.show_help;
        }
        if inp.key_hit(KeyCode::F2) {
            self.s.show_debug = !self.s.show_debug;
            self.s.save();
        }
        if inp.key_hit(KeyCode::F8) {
            self.stats = Stats::default();
            self.input.reset_counters();
            self.toast(now, "Счётчики сброшены");
        }
        if self.input.key_hit(KeyCode::F11) {
            self.fullscreen = !self.fullscreen;
            set_fullscreen(self.fullscreen);
        }
        if !self.paused && !self.grabbed && self.input.hit(Btn::Mouse(MouseButton::Left)) {
            self.set_grab(true);
            // The click that grabs the cursor must not fire.
            self.input.pressed.retain(|b| *b != Btn::Mouse(MouseButton::Left));
            self.input.down.remove(&Btn::Mouse(MouseButton::Left));
        }
    }

    fn menu(&mut self) {
        let inp = &self.input;
        let any = |ks: &[KeyCode]| ks.iter().any(|k| inp.key_hit(*k));
        let up = any(&[KeyCode::Up, KeyCode::W]);
        let down = any(&[KeyCode::Down, KeyCode::S]);
        let left = any(&[KeyCode::Left, KeyCode::A]);
        let right = any(&[KeyCode::Right, KeyCode::D, KeyCode::Enter, KeyCode::Space]);
        if up {
            self.menu_sel = (self.menu_sel + MENU_LEN - 1) % MENU_LEN;
        }
        if down {
            self.menu_sel = (self.menu_sel + 1) % MENU_LEN;
        }
        if left {
            self.s.menu_adjust(self.menu_sel, -1.0);
        }
        if right {
            self.s.menu_adjust(self.menu_sel, 1.0);
        }
    }

    /// Key-up events never arrive while another window has focus, so held keys would stick.
    fn check_focus(&mut self, now: f64) {
        let focused = window_focused();
        if self.focused && !focused {
            self.input.down.clear();
            self.set_grab(false);
            self.input.push_log(now, "окно потеряло фокус — зажатые сброшены".into(), ORANGE);
        }
        self.focused = focused;
    }

    pub fn zoom(&self) -> f32 {
        1.0 + (self.arms.def().zoom - 1.0) * self.arms.ads
    }

    pub fn hfov(&self) -> f32 {
        let base = self.s.fov.to_radians();
        2.0 * ((base / 2.0).tan() / self.zoom()).atan()
    }

    fn update_device(&mut self, now: f64) {
        let pad_now = self.pads.last_activity > self.input.last_kbm;
        if pad_now != self.pad_active {
            self.pad_active = pad_now;
            self.dev_switches.push_back(now);
        }
        while self.dev_switches.front().is_some_and(|t| now - t > 10.0) {
            self.dev_switches.pop_front();
        }
    }

    fn gameplay(&mut self, now: f64, dt: f32) {
        let inp = &self.input;

        if self.grabbed {
            let base = self.s.fov.to_radians();
            let mut k = YAW_DEG_PER_COUNT * self.s.sens;
            if self.s.ads_scale_by_fov {
                k *= (self.hfov() / 2.0).tan() / (base / 2.0).tan();
            }
            self.p.look(inp.delta, k, self.s.invert_y);
            // Turn rate in degrees per second drives the weapon lag.
            let rate = inp.delta * k / dt.max(1e-4);
            let target = (-rate * 0.00025).clamp(Vec2::splat(-0.09), Vec2::splat(0.09));
            let target = vec2(target.x, if self.s.invert_y { target.y } else { -target.y });
            self.sway += (target - self.sway) * (dt * 10.0).min(1.0);
        }

        let axis = |pos: KeyCode, neg: KeyCode| inp.key(pos) as i32 as f32 - inp.key(neg) as i32 as f32;
        let mut keys = vec2(axis(KeyCode::D, KeyCode::A), axis(KeyCode::W, KeyCode::S));
        if keys.length() > 1.0 {
            keys = keys.normalize();
        }
        let stick = if self.s.pad_move { self.pads.move_stick() } else { Vec2::ZERO };
        self.keys_vec = keys;
        self.stick_vec = stick;
        let use_stick = stick != Vec2::ZERO
            && match self.s.mixed {
                // A stick mapped to WASD as well would always add up to full speed: the analog wins.
                MixedInput::Simultaneous => true,
                MixedInput::LastDeviceWins => self.pad_active,
            };
        let mut stick_walk = false;
        let wish = if use_stick {
            match self.s.analog {
                AnalogMode::Smooth => stick,
                AnalogMode::WalkRun => {
                    stick_walk = stick.length() < WALK_THRESHOLD;
                    stick.normalize()
                }
                AnalogMode::Full => stick.normalize(),
            }
        } else if self.s.mixed == MixedInput::LastDeviceWins && self.pad_active {
            Vec2::ZERO
        } else {
            keys
        };

        let crouch_key = inp.key(KeyCode::LeftControl) || inp.key(KeyCode::C);
        let crouch_hit = inp.key_hit(KeyCode::LeftControl) || inp.key_hit(KeyCode::C);
        if self.s.crouch_toggle {
            if crouch_hit {
                self.crouch_on = !self.crouch_on;
            }
        } else {
            self.crouch_on = crouch_key;
        }
        if self.s.sprint_toggle {
            if inp.key_hit(KeyCode::LeftShift) {
                self.sprint_on = !self.sprint_on;
            }
            if wish.y < 0.5 {
                self.sprint_on = false;
            }
        } else {
            self.sprint_on = inp.key(KeyCode::LeftShift);
        }
        let rmb = Btn::Mouse(MouseButton::Right);
        if self.s.ads_toggle {
            if inp.hit(rmb) {
                self.arms.aiming = !self.arms.aiming;
            }
        } else {
            self.arms.aiming = inp.held(rmb);
        }
        self.arms.aiming |= self.force_ads;

        let intent = MoveIntent {
            wish,
            jump: inp.key_hit(KeyCode::Space),
            crouch: self.crouch_on,
            sprint: self.sprint_on,
            walk: inp.key(KeyCode::LeftAlt) || stick_walk,
        };
        self.p.step(dt, &intent, &self.world);
        let sprinting = self.p.stance == Stance::Sprint && self.p.horizontal_speed() > 4.0;
        let target = if sprinting { 1.0 } else { 0.0 };
        self.sprint_k += (target - self.sprint_k) * (dt * 8.0).min(1.0);

        self.weapons_input(now);
        self.misc_input(now);
    }

    fn weapons_input(&mut self, now: f64) {
        let inp = &self.input;
        let mut slot = None;
        for (i, k) in SLOT_KEYS.iter().enumerate() {
            if inp.key_hit(*k) {
                slot = Some(i);
            }
        }
        let reload = inp.key_hit(KeyCode::R);
        let lmb = Btn::Mouse(MouseButton::Left);
        let trigger = if self.arms.def().auto { inp.held(lmb) } else { inp.hit(lmb) };
        let melee = inp.key_hit(KeyCode::V);
        let grenade = inp.key_hit(KeyCode::G);

        if let Some(i) = slot {
            if self.arms.select(now, i, &mut self.stats) {
                self.switched_at = now;
            }
        }
        if reload {
            self.arms.start_reload(now, &mut self.stats);
        }

        let eye = self.p.eye();
        let dir = self.p.look_dir();
        if melee {
            self.arms.quick_melee(now, &mut self.stats);
        }
        if grenade {
            self.arms.throw_grenade(now, eye, dir, &mut self.fx, &mut self.stats);
        }
        if trigger && self.grabbed {
            let moving = self.p.horizontal_speed() > 1.0;
            let spread = self.arms.current_spread(moving, !self.p.on_ground);
            self.arms.fire(now, eye, dir, spread, &mut self.world, &mut self.fx, &mut self.stats);
        }
    }

    fn misc_input(&mut self, now: f64) {
        let inp = &self.input;
        let use_hit = inp.key_hit(KeyCode::E);
        let flash = inp.key_hit(KeyCode::F);
        let ping = inp.hit(Btn::Mouse(MouseButton::Middle));
        let map = inp.key_hit(KeyCode::M);
        let inv = inp.key_hit(KeyCode::I);
        let save = inp.key_hit(KeyCode::F5);
        let load = inp.key_hit(KeyCode::F9);
        let eye = self.p.eye();
        let dir = self.p.look_dir();

        if use_hit {
            let near = self.world.button.ray(eye, dir).is_some_and(|t| t < 3.0);
            if near {
                self.world.button_on = !self.world.button_on;
                let msg = if self.world.button_on { "Кнопка: включено" } else { "Кнопка: выключено" };
                self.toast(now, msg);
            } else {
                self.toast(now, "Использовать: рядом ничего нет");
            }
        }
        if flash {
            self.flashlight = !self.flashlight;
        }
        if ping {
            if let Some((t, _)) = self.world.raycast(now, eye, dir, 300.0) {
                self.fx.pings.push((eye + dir * t, now + 4.0));
            }
        }
        if map {
            self.show_map = !self.show_map;
            self.show_inv = false;
        }
        if inv {
            self.show_inv = !self.show_inv;
            self.show_map = false;
        }
        if save {
            self.quicksave = Some((self.p.pos, self.p.yaw, self.p.pitch, self.arms.cur));
            self.toast(now, "Быстрое сохранение");
        }
        if load {
            match self.quicksave {
                Some((pos, yaw, pitch, cur)) => {
                    self.p.pos = pos;
                    self.p.vel = Vec3::ZERO;
                    self.p.yaw = yaw;
                    self.p.pitch = pitch;
                    self.arms.cur = cur;
                    self.toast(now, "Быстрая загрузка");
                }
                None => self.toast(now, "Нет быстрого сохранения"),
            }
        }
    }

    fn render_world(&mut self, now: f64) {
        clear_background(Color::new(0.52, 0.66, 0.8, 1.0));
        let aspect = screen_width() / screen_height();
        self.vfov = 2.0 * ((self.hfov() / 2.0).tan() / aspect).atan();
        let eye = self.p.eye();
        let (punch_yaw, punch_roll) = viewmodel::swing_punch((now - self.arms.swing_at) as f32);
        let look = Quat::from_rotation_y(-punch_yaw) * self.p.look_dir();
        self.cam = Camera3D {
            position: eye,
            target: eye + look,
            up: Quat::from_axis_angle(look, punch_roll) * Vec3::Y,
            fovy: self.vfov,
            z_near: 0.05,
            z_far: 1000.0,
            ..Default::default()
        };
        set_camera(&self.cam);
        self.world.draw(now);
        weapons::draw_fx(now, &self.fx);
        set_default_camera();

        let scoped = self.arms.cur == 5 && self.arms.ads > 0.85;
        if !scoped {
            self.render_viewmodel(now);
        }
    }

    /// The weapon gets its own pass with a cleared depth buffer so it never clips into walls.
    fn render_viewmodel(&mut self, now: f64) {
        let dpi = macroquad::miniquad::window::dpi_scale();
        let (w, h) = ((screen_width() * dpi).round() as u32, (screen_height() * dpi).round() as u32);
        let stale = self.viewmodel_rt.as_ref().is_none_or(|rt| rt.texture.width() as u32 != w || rt.texture.height() as u32 != h);
        if stale {
            let rt = render_target_ex(w, h, RenderTargetParams { sample_count: 4, depth: true });
            rt.texture.set_filter(FilterMode::Linear);
            self.viewmodel_rt = Some(rt);
        }
        let rt = self.viewmodel_rt.clone().expect("render target created above");
        set_camera(&Camera3D {
            position: Vec3::ZERO,
            target: -Vec3::Z,
            up: Vec3::Y,
            fovy: 68f32.to_radians(),
            aspect: Some(w as f32 / h as f32),
            z_near: 0.01,
            z_far: 10.0,
            render_target: Some(rt.clone()),
            ..Default::default()
        });
        clear_background(Color::new(0.0, 0.0, 0.0, 0.0));
        self.draw_viewmodel(now);
        set_default_camera();
        draw_texture_ex(
            &rt.texture,
            0.0,
            0.0,
            WHITE,
            DrawTextureParams { dest_size: Some(vec2(screen_width(), screen_height())), flip_y: true, ..Default::default() },
        );
    }
}

/// `--screenshot <file.png> [--overlay help|map|inv|pause] [--weapon 1-8]
/// [--anim fire|reload|swing|throw|ads|switch] [--after <seconds>]`: render, save the frame, exit.
/// Lets an agent check what the game shows without a screen-capture tool.
struct Shot {
    path: String,
    overlay: Option<String>,
    weapon: Option<usize>,
    anim: Option<String>,
    after: f64,
}

const SHOT_START: f64 = 2.5;

fn parse_shot() -> Option<Shot> {
    let args: Vec<String> = std::env::args().collect();
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    Some(Shot {
        path: value("--screenshot")?,
        overlay: value("--overlay"),
        weapon: value("--weapon").and_then(|v| v.parse::<usize>().ok()).map(|n| n.clamp(1, 8) - 1),
        anim: value("--anim"),
        after: value("--after").and_then(|v| v.parse().ok()).unwrap_or(0.15),
    })
}

impl Game {
    fn start_shot_anim(&mut self, now: f64, anim: &str) {
        let eye = self.p.eye();
        let dir = self.p.look_dir();
        match anim {
            "fire" => {
                self.arms.fire(now, eye, dir, 0.0, &mut self.world, &mut self.fx, &mut self.stats);
            }
            "reload" => {
                self.arms.ammo[self.arms.cur] = 0;
                self.arms.start_reload(now, &mut self.stats);
            }
            "swing" => {
                self.arms.quick_melee(now, &mut self.stats);
            }
            "throw" => {
                self.arms.throw_grenade(now, eye, dir, &mut self.fx, &mut self.stats);
            }
            "ads" => self.force_ads = true,
            "switch" => {
                let next = (self.arms.cur + 1) % DEFS.len();
                self.arms.select(now, next, &mut self.stats);
                self.switched_at = now;
            }
            _ => {}
        }
    }
}

fn window_conf() -> macroquad::window::Conf {
    macroquad::window::Conf {
        window_title: "TestRange — стенд для отладки раскладок".to_string(),
        window_width: 1920,
        window_height: 1080,
        high_dpi: true,
        sample_count: 4,
        platform: Platform { swap_interval: Some(0), ..Default::default() },
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let mut g = Game {
        txt: Txt::load().await,
        s: Settings::load(),
        input: Input::new(),
        pads: Pads::new(),
        world: World::new(),
        p: Player::new(),
        arms: Arsenal::new(),
        fx: Fx::new(),
        stats: Stats::default(),
        grabbed: false,
        focused: true,
        paused: false,
        fullscreen: false,
        menu_sel: 0,
        show_help: false,
        show_map: false,
        show_inv: false,
        flashlight: false,
        crouch_on: false,
        sprint_on: false,
        toasts: VecDeque::new(),
        quicksave: None,
        pad_active: false,
        dev_switches: VecDeque::new(),
        keys_vec: Vec2::ZERO,
        stick_vec: Vec2::ZERO,
        fps: 0.0,
        cam: Camera3D::default(),
        vfov: 1.0,
        sway: Vec2::ZERO,
        sprint_k: 0.0,
        switched_at: -10.0,
        force_ads: false,
        viewmodel_rt: None,
    };
    debug_assert_eq!(DEFS.len(), SLOT_KEYS.len());

    let shot = parse_shot();
    if let Some(overlay) = shot.as_ref().and_then(|s| s.overlay.as_deref()) {
        match overlay {
            "help" => g.show_help = true,
            "map" => g.show_map = true,
            "inv" => g.show_inv = true,
            "pause" => g.paused = true,
            _ => {}
        }
    }
    if let Some(w) = shot.as_ref().and_then(|s| s.weapon) {
        g.arms.cur = w;
    }
    let started = get_time();
    let mut anim_started = false;

    loop {
        let now = get_time();
        let dt = get_frame_time().min(0.05);
        g.fps = g.fps * 0.95 + (1.0 / get_frame_time().max(1e-4)) * 0.05;

        g.input.update(now, g.grabbed, action_of);
        g.check_focus(now);
        g.pads.update(now, &mut g.input);
        g.update_device(now);
        g.meta_keys(now);
        if g.paused {
            g.menu();
        } else {
            g.gameplay(now, dt);
        }
        g.arms.update(now, dt);
        let (eye, dir) = (g.p.eye(), g.p.look_dir());
        g.arms.resolve_strike(now, eye, dir, &mut g.world, &mut g.fx, &mut g.stats);
        g.world.update(now);
        weapons::update_fx(now, dt, &mut g.world, &mut g.fx, &mut g.stats);

        g.render_world(now);
        g.draw_hud(now);
        if let Some(s) = &shot {
            let t = now - started;
            if let (Some(anim), false) = (&s.anim, anim_started) {
                if t >= SHOT_START {
                    g.start_shot_anim(now, anim);
                    anim_started = true;
                }
            }
            let at = if s.anim.is_some() { SHOT_START + s.after } else { SHOT_START };
            if t >= at && (anim_started || s.anim.is_none()) {
                get_screen_data().export_png(&s.path);
                std::process::exit(0);
            }
        }
        next_frame().await;
    }
}
