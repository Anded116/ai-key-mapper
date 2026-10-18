use std::fs;

/// Source-engine style yaw: degrees of rotation per raw mouse count at sensitivity 1.
/// JoyShockMapper: REAL_WORLD_CALIBRATION = 1 / YAW_DEG_PER_COUNT, IN_GAME_SENS = sens.
pub const YAW_DEG_PER_COUNT: f32 = 0.022;

const PATH: &str = "testrange.cfg";

#[derive(Clone, Copy, PartialEq)]
pub enum MixedInput {
    /// Keyboard and gamepad stick work at the same time.
    Simultaneous,
    /// The most recently used device wins, like many console ports do.
    LastDeviceWins,
}

/// How stick deflection maps to movement speed when the game reads the stick itself.
#[derive(Clone, Copy, PartialEq)]
pub enum AnalogMode {
    /// Speed proportional to deflection.
    Smooth,
    /// Walk below WALK_THRESHOLD, run above it.
    WalkRun,
    /// Always full speed, like WASD.
    Full,
}

pub const WALK_THRESHOLD: f32 = 0.6;

pub struct Settings {
    pub sens: f32,
    pub fov: f32,
    pub invert_y: bool,
    pub ads_scale_by_fov: bool,
    pub sprint_toggle: bool,
    pub crouch_toggle: bool,
    pub ads_toggle: bool,
    pub pad_move: bool,
    pub mixed: MixedInput,
    pub analog: AnalogMode,
    pub show_debug: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sens: 1.0,
            fov: 100.0,
            invert_y: false,
            ads_scale_by_fov: true,
            sprint_toggle: false,
            crouch_toggle: false,
            ads_toggle: false,
            pad_move: true,
            mixed: MixedInput::Simultaneous,
            analog: AnalogMode::Smooth,
            show_debug: true,
        }
    }
}

pub const MENU_LEN: usize = 11;

impl Settings {
    pub fn load() -> Self {
        let mut s = Self::default();
        let Ok(text) = fs::read_to_string(PATH) else { return s };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            let b = v == "1";
            match k {
                "sens" => s.sens = v.parse().unwrap_or(s.sens),
                "fov" => s.fov = v.parse().unwrap_or(s.fov),
                "invert_y" => s.invert_y = b,
                "ads_scale_by_fov" => s.ads_scale_by_fov = b,
                "sprint_toggle" => s.sprint_toggle = b,
                "crouch_toggle" => s.crouch_toggle = b,
                "ads_toggle" => s.ads_toggle = b,
                "pad_move" => s.pad_move = b,
                "last_device_wins" => {
                    s.mixed = if b { MixedInput::LastDeviceWins } else { MixedInput::Simultaneous }
                }
                "analog" => {
                    s.analog = match v {
                        "1" => AnalogMode::WalkRun,
                        "2" => AnalogMode::Full,
                        _ => AnalogMode::Smooth,
                    }
                }
                "show_debug" => s.show_debug = b,
                _ => {}
            }
        }
        s
    }

    pub fn save(&self) {
        let f = |b: bool| if b { 1 } else { 0 };
        let text = format!(
            "sens={}\nfov={}\ninvert_y={}\nads_scale_by_fov={}\nsprint_toggle={}\ncrouch_toggle={}\n\
             ads_toggle={}\npad_move={}\nlast_device_wins={}\nanalog={}\nshow_debug={}\n",
            self.sens,
            self.fov,
            f(self.invert_y),
            f(self.ads_scale_by_fov),
            f(self.sprint_toggle),
            f(self.crouch_toggle),
            f(self.ads_toggle),
            f(self.pad_move),
            f(self.mixed == MixedInput::LastDeviceWins),
            self.analog as u8,
            f(self.show_debug),
        );
        let _ = fs::write(PATH, text);
    }

    pub fn menu_item(&self, i: usize) -> (&'static str, String) {
        let yn = |b: bool| if b { "да" } else { "нет" }.to_string();
        let hold = |b: bool| if b { "переключение" } else { "удержание" }.to_string();
        match i {
            0 => ("Чувствительность (IN_GAME_SENS)", format!("{:.2}", self.sens)),
            1 => ("FOV по горизонтали", format!("{:.0}°", self.fov)),
            2 => ("Инверсия Y", yn(self.invert_y)),
            3 => ("Чувств. в прицеле по FOV", yn(self.ads_scale_by_fov)),
            4 => ("Бег (Shift)", hold(self.sprint_toggle)),
            5 => ("Присед (Ctrl / C)", hold(self.crouch_toggle)),
            6 => ("Прицел (ПКМ)", hold(self.ads_toggle)),
            7 => ("Движение стиком геймпада", yn(self.pad_move)),
            8 => (
                "Смешанный ввод",
                match self.mixed {
                    MixedInput::Simultaneous => "одновременно".to_string(),
                    MixedInput::LastDeviceWins => "последнее устройство".to_string(),
                },
            ),
            9 => (
                "Скорость от отклонения стика",
                match self.analog {
                    AnalogMode::Smooth => "плавно".to_string(),
                    AnalogMode::WalkRun => format!("шаг до {:.0}%, дальше бег", WALK_THRESHOLD * 100.0),
                    AnalogMode::Full => "всегда бег".to_string(),
                },
            ),
            _ => ("Отладочный HUD (F2)", yn(self.show_debug)),
        }
    }

    pub fn menu_adjust(&mut self, i: usize, dir: f32) {
        match i {
            0 => {
                let step = if self.sens < 1.0 { 0.05 } else { 0.1 };
                self.sens = ((self.sens + dir * step) * 100.0).round() / 100.0;
                self.sens = self.sens.clamp(0.05, 20.0);
            }
            1 => self.fov = (self.fov + dir * 5.0).clamp(60.0, 130.0),
            2 => self.invert_y = !self.invert_y,
            3 => self.ads_scale_by_fov = !self.ads_scale_by_fov,
            4 => self.sprint_toggle = !self.sprint_toggle,
            5 => self.crouch_toggle = !self.crouch_toggle,
            6 => self.ads_toggle = !self.ads_toggle,
            7 => self.pad_move = !self.pad_move,
            8 => {
                self.mixed = match self.mixed {
                    MixedInput::Simultaneous => MixedInput::LastDeviceWins,
                    MixedInput::LastDeviceWins => MixedInput::Simultaneous,
                }
            }
            9 => {
                const ALL: [AnalogMode; 3] = [AnalogMode::Smooth, AnalogMode::WalkRun, AnalogMode::Full];
                let i = ALL.iter().position(|m| *m == self.analog).unwrap_or(0) as i32;
                self.analog = ALL[(i + dir.signum() as i32).rem_euclid(3) as usize];
            }
            _ => self.show_debug = !self.show_debug,
        }
        self.save();
    }
}
