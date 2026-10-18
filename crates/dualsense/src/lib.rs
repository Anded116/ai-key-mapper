//! PlayStation controllers over HID: DualSense, DualSense Edge and DualShock 4, USB and Bluetooth.
//!
//! The device is opened in shared mode, so it works next to JoyShockMapper or Steam. Opening a
//! DualSense reads its calibration feature report, which also switches a Bluetooth pad to the
//! extended report with gyro, touchpad and battery.

mod report;

use std::fmt;
use std::ops::{BitOr, BitOrAssign};

use hidapi::{BusType, HidApi, HidDevice};

pub use report::RawImu;

const SONY: u16 = 0x054C;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model {
    DualSense,
    DualSenseEdge,
    DualShock4,
}

impl Model {
    fn from_pid(pid: u16) -> Option<Self> {
        match pid {
            0x0CE6 => Some(Model::DualSense),
            0x0DF2 => Some(Model::DualSenseEdge),
            0x05C4 | 0x09CC => Some(Model::DualShock4),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Model::DualSense => "DualSense",
            Model::DualSenseEdge => "DualSense Edge",
            Model::DualShock4 => "DualShock 4",
        }
    }

    /// Touchpad resolution in touch units.
    pub fn touch_size(self) -> (u16, u16) {
        match self {
            Model::DualShock4 => (1920, 942),
            _ => (1920, 1080),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bus {
    Usb,
    Bluetooth,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Buttons(pub u32);

impl Buttons {
    pub const NONE: Self = Self(0);
    pub const UP: Self = Self(1 << 0);
    pub const DOWN: Self = Self(1 << 1);
    pub const LEFT: Self = Self(1 << 2);
    pub const RIGHT: Self = Self(1 << 3);
    pub const CROSS: Self = Self(1 << 4);
    pub const CIRCLE: Self = Self(1 << 5);
    pub const SQUARE: Self = Self(1 << 6);
    pub const TRIANGLE: Self = Self(1 << 7);
    pub const L1: Self = Self(1 << 8);
    pub const R1: Self = Self(1 << 9);
    pub const L2: Self = Self(1 << 10);
    pub const R2: Self = Self(1 << 11);
    pub const L3: Self = Self(1 << 12);
    pub const R3: Self = Self(1 << 13);
    /// Share on DualShock 4.
    pub const CREATE: Self = Self(1 << 14);
    pub const OPTIONS: Self = Self(1 << 15);
    pub const PS: Self = Self(1 << 16);
    pub const TOUCHPAD: Self = Self(1 << 17);
    pub const MIC: Self = Self(1 << 18);

    pub const ALL: [(Self, &'static str); 19] = [
        (Self::UP, "Up"),
        (Self::DOWN, "Down"),
        (Self::LEFT, "Left"),
        (Self::RIGHT, "Right"),
        (Self::CROSS, "Cross"),
        (Self::CIRCLE, "Circle"),
        (Self::SQUARE, "Square"),
        (Self::TRIANGLE, "Triangle"),
        (Self::L1, "L1"),
        (Self::R1, "R1"),
        (Self::L2, "L2"),
        (Self::R2, "R2"),
        (Self::L3, "L3"),
        (Self::R3, "R3"),
        (Self::CREATE, "Create"),
        (Self::OPTIONS, "Options"),
        (Self::PS, "PS"),
        (Self::TOUCHPAD, "Touchpad"),
        (Self::MIC, "Mic"),
    ];

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn names(self) -> impl Iterator<Item = &'static str> {
        Self::ALL.into_iter().filter(move |(b, _)| self.contains(*b)).map(|(_, n)| n)
    }
}

impl BitOr for Buttons {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for Buttons {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl fmt::Display for Buttons {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<_> = self.names().collect();
        f.write_str(&names.join(" "))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Touch {
    pub active: bool,
    pub id: u8,
    pub x: u16,
    pub y: u16,
}

/// One controller sample. Sticks are -1..1 with +Y up, triggers 0..1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct State {
    pub buttons: Buttons,
    pub left: [f32; 2],
    pub right: [f32; 2],
    pub l2: f32,
    pub r2: f32,
    /// Degrees per second: pitch (X), yaw (Y), roll (Z), with the factory scale applied.
    pub gyro: [f32; 3],
    /// Acceleration in g.
    pub accel: [f32; 3],
    pub touch: [Touch; 2],
    pub battery: Option<f32>,
    pub charging: bool,
    /// Controller sensor clock in microseconds (wraps).
    pub timestamp_us: u32,
}

/// Factory IMU scale from the calibration feature report. Gyro bias is not taken from it: the
/// factory bias drifts, a still-period calibration at runtime is more accurate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Calibration {
    pub gyro_dps_per_lsb: [f32; 3],
    pub accel_center: [f32; 3],
    pub accel_g_per_lsb: [f32; 3],
}

impl Calibration {
    /// Nominal values used when the controller does not report calibration.
    pub fn nominal() -> Self {
        Self { gyro_dps_per_lsb: [1.0 / 16.0; 3], accel_center: [0.0; 3], accel_g_per_lsb: [1.0 / 8192.0; 3] }
    }

    /// DualSense calibration report 0x05 (also DS4 over Bluetooth); `r[0]` is the report id.
    fn from_dualsense_report(r: &[u8]) -> Option<Self> {
        if r.len() < 35 {
            return None;
        }
        let v = |at: usize| i16::from_le_bytes([r[at], r[at + 1]]) as f32;
        let bias = [v(1), v(3), v(5)];
        let plus = [v(7), v(11), v(15)];
        let minus = [v(9), v(13), v(17)];
        let speed_2x = v(19) + v(21);
        let mut gyro = [0.0; 3];
        for i in 0..3 {
            let range = (plus[i] - bias[i]).abs() + (minus[i] - bias[i]).abs();
            if range < 1.0 || speed_2x < 1.0 {
                return None;
            }
            gyro[i] = speed_2x / range;
        }
        let mut accel_center = [0.0; 3];
        let mut accel_scale = [0.0; 3];
        for i in 0..3 {
            let (p, m) = (v(23 + i * 4), v(25 + i * 4));
            let range_2g = p - m;
            if range_2g.abs() < 1.0 {
                return None;
            }
            accel_center[i] = p - range_2g / 2.0;
            accel_scale[i] = 2.0 / range_2g;
        }
        Some(Self { gyro_dps_per_lsb: gyro, accel_center, accel_g_per_lsb: accel_scale })
    }

    pub fn apply(&self, raw: &RawImu, out: &mut State) {
        for i in 0..3 {
            out.gyro[i] = raw.gyro[i] as f32 * self.gyro_dps_per_lsb[i];
            out.accel[i] = (raw.accel[i] as f32 - self.accel_center[i]) * self.accel_g_per_lsb[i];
        }
    }
}

#[derive(Debug)]
pub enum Error {
    NotFound,
    Hid(hidapi::HidError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotFound => f.write_str("PlayStation controller not found"),
            Error::Hid(e) => write!(f, "HID: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<hidapi::HidError> for Error {
    fn from(e: hidapi::HidError) -> Self {
        Error::Hid(e)
    }
}

pub struct Pad {
    dev: HidDevice,
    pub model: Model,
    pub bus: Bus,
    pub calibration: Calibration,
    /// Whether `calibration` came from the controller or is nominal.
    pub factory_calibration: bool,
    output_seq: u8,
    buf: [u8; 128],
}

impl Pad {
    /// Opens the first connected PlayStation controller.
    pub fn open_first(api: &HidApi) -> Result<Self, Error> {
        let info = api
            .device_list()
            .find(|d| d.vendor_id() == SONY && Model::from_pid(d.product_id()).is_some())
            .ok_or(Error::NotFound)?;
        let model = Model::from_pid(info.product_id()).ok_or(Error::NotFound)?;
        let bus = match info.bus_type() {
            BusType::Bluetooth => Bus::Bluetooth,
            _ => Bus::Usb,
        };
        let dev = info.open_device(api)?;
        let mut pad = Self {
            dev,
            model,
            bus,
            calibration: Calibration::nominal(),
            factory_calibration: false,
            output_seq: 0,
            buf: [0; 128],
        };
        pad.read_calibration();
        Ok(pad)
    }

    fn read_calibration(&mut self) {
        let id = match (self.model, self.bus) {
            (Model::DualShock4, Bus::Usb) => return,
            _ => 0x05,
        };
        let mut r = [0u8; 64];
        r[0] = id;
        if let Ok(n) = self.dev.get_feature_report(&mut r) {
            if let Some(c) = Calibration::from_dualsense_report(&r[..n]) {
                self.calibration = c;
                self.factory_calibration = true;
            }
        }
    }

    /// Waits up to `timeout_ms` for the next report. `Ok(None)` on timeout or a report
    /// without controller state.
    pub fn read(&mut self, timeout_ms: i32) -> Result<Option<State>, Error> {
        let n = self.dev.read_timeout(&mut self.buf, timeout_ms)?;
        if n == 0 {
            return Ok(None);
        }
        let Some((mut state, imu)) = report::parse(self.model, self.bus, &self.buf[..n]) else {
            return Ok(None);
        };
        if let Some(raw) = imu {
            self.calibration.apply(&raw, &mut state);
        }
        Ok(Some(state))
    }

    /// Sets the light bar colour (DualSense only for now).
    pub fn set_lightbar(&mut self, rgb: [u8; 3]) -> Result<(), Error> {
        // valid_flag1 bit 2 enables light bar control, colour at 44..47.
        self.send_output(|c| {
            c[1] = 0x04;
            c[44..47].copy_from_slice(&rgb);
        })
    }

    /// Turns adaptive trigger resistance off on both triggers. Effects set by another program
    /// (JoyShockMapper, a game) stay in the controller until something resets them.
    pub fn disable_trigger_effects(&mut self) -> Result<(), Error> {
        // valid_flag0 bits 2 and 3 enable the right and left trigger effect; effect mode 0x05 is
        // "off" (Nielk1's trigger effect generator, used by JSM), parameters zero.
        self.send_output(|c| {
            c[0] = 0x04 | 0x08;
            c[10] = 0x05;
            c[21] = 0x05;
        })
    }

    /// DualSense output report around the 47-byte common body that `fill` writes.
    fn send_output(&mut self, fill: impl FnOnce(&mut [u8; 47])) -> Result<(), Error> {
        if self.model == Model::DualShock4 {
            return Ok(());
        }
        let mut common = [0u8; 47];
        fill(&mut common);
        match self.bus {
            Bus::Usb => {
                let mut r = [0u8; 48];
                r[0] = 0x02;
                r[1..].copy_from_slice(&common);
                self.dev.write(&r)?;
            }
            Bus::Bluetooth => {
                let mut r = [0u8; 78];
                r[0] = 0x31;
                r[1] = self.output_seq << 4;
                r[2] = 0x10;
                r[3..50].copy_from_slice(&common);
                self.output_seq = (self.output_seq + 1) & 0x0F;
                let crc = bt_crc(0xA2, &r[..74]);
                r[74..].copy_from_slice(&crc.to_le_bytes());
                self.dev.write(&r)?;
            }
        }
        Ok(())
    }
}

/// Bluetooth reports carry a CRC32 over a one-byte transaction header plus the report.
fn bt_crc(header: u8, data: &[u8]) -> u32 {
    let mut h = crc32fast::Hasher::new();
    h.update(&[header]);
    h.update(data);
    h.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calibration_scale_from_report() {
        let mut r = [0u8; 41];
        r[0] = 0x05;
        let mut put = |at: usize, v: i16| r[at..at + 2].copy_from_slice(&v.to_le_bytes());
        // Bias 0, ±8192 counts for ±540 deg/s on every gyro axis.
        for at in [7, 11, 15] {
            put(at, 8192);
        }
        for at in [9, 13, 17] {
            put(at, -8192);
        }
        put(19, 540);
        put(21, 540);
        for i in 0..3 {
            put(23 + i * 4, 8192);
            put(25 + i * 4, -8192);
        }
        let c = Calibration::from_dualsense_report(&r).unwrap();
        assert!((c.gyro_dps_per_lsb[0] - 1080.0 / 16384.0).abs() < 1e-6);
        assert!((c.accel_g_per_lsb[1] - 2.0 / 16384.0).abs() < 1e-9);
        assert_eq!(c.accel_center, [0.0; 3]);
    }

    #[test]
    fn crc_matches_known_value() {
        assert_eq!(bt_crc(0xA2, &[]), crc32fast::hash(&[0xA2]));
    }

    #[test]
    fn button_names() {
        let b = Buttons::L1 | Buttons::CROSS;
        assert_eq!(b.to_string(), "Cross L1");
    }
}
