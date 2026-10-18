use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use hidapi::{BusType, HidApi};
use macroquad::prelude::*;

const SONY: u16 = 0x054C;
/// DualSense, DualSense Edge, DualShock 4 v1 / v2.
const PIDS: [(u16, bool); 4] = [(0x0CE6, false), (0x0DF2, false), (0x05C4, true), (0x09CC, true)];
const STALE: Duration = Duration::from_millis(500);

#[derive(Clone)]
pub struct Sticks {
    pub left: Vec2,
    pub right: Vec2,
    pub name: String,
}

struct Shared {
    sticks: Option<Sticks>,
    updated: Instant,
}

/// Reads PlayStation sticks straight from HID, in shared mode next to JoyShockMapper.
///
/// Over Bluetooth, JSM (SDL) switches the pad to the extended report (0x31 / 0x11). Windows'
/// generic game-controller driver then stops seeing the sticks, so gilrs reports zeros while
/// the buttons still work. Parsing the raw reports keeps both report formats readable.
pub struct PadHid {
    shared: Arc<Mutex<Shared>>,
}

impl PadHid {
    pub fn start() -> Self {
        let shared = Arc::new(Mutex::new(Shared { sticks: None, updated: Instant::now() }));
        let worker = Arc::clone(&shared);
        thread::spawn(move || loop {
            read_until_disconnect(&worker);
            if let Ok(mut s) = worker.lock() {
                s.sticks = None;
            }
            thread::sleep(Duration::from_secs(1));
        });
        Self { shared }
    }

    pub fn sticks(&self) -> Option<Sticks> {
        let s = self.shared.lock().ok()?;
        if s.updated.elapsed() > STALE {
            return None;
        }
        s.sticks.clone()
    }
}

fn read_until_disconnect(shared: &Mutex<Shared>) {
    let Ok(api) = HidApi::new() else { return };
    let found = api.device_list().find_map(|d| {
        let ds4 = PIDS.iter().find(|(pid, _)| d.vendor_id() == SONY && d.product_id() == *pid)?.1;
        Some((d, ds4))
    });
    let Some((info, ds4)) = found else { return };
    let bus = match info.bus_type() {
        BusType::Bluetooth => "Bluetooth",
        BusType::Usb => "USB",
        _ => "HID",
    };
    let model = if ds4 { "DualShock 4" } else { "DualSense" };
    let name = format!("{model} · {bus}");
    let Ok(dev) = info.open_device(&api) else { return };
    let mut buf = [0u8; 128];
    loop {
        match dev.read_timeout(&mut buf, 200) {
            Ok(0) => continue,
            Ok(n) => {
                let Some(raw) = parse(&buf[..n], ds4) else { continue };
                let axis = |v: u8| (v as f32 - 127.5) / 127.5;
                let sticks = Sticks {
                    left: vec2(axis(raw[0]), -axis(raw[1])),
                    right: vec2(axis(raw[2]), -axis(raw[3])),
                    name: name.clone(),
                };
                if let Ok(mut s) = shared.lock() {
                    s.sticks = Some(sticks);
                    s.updated = Instant::now();
                }
            }
            Err(_) => return,
        }
    }
}

/// Stick bytes LX, LY, RX, RY from an input report (the report id is the first byte).
fn parse(r: &[u8], ds4: bool) -> Option<[u8; 4]> {
    let at = |o: usize| r.get(o..o + 4).map(|s| [s[0], s[1], s[2], s[3]]);
    match r.first()? {
        0x01 => at(1),
        0x31 if !ds4 => at(2),
        0x11 if ds4 => at(3),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_usb_and_bluetooth_reports() {
        assert_eq!(parse(&[0x01, 10, 20, 30, 40, 0], false), Some([10, 20, 30, 40]));
        assert_eq!(parse(&[0x31, 0xAA, 10, 20, 30, 40], false), Some([10, 20, 30, 40]));
        assert_eq!(parse(&[0x11, 0xC0, 0x00, 10, 20, 30, 40], true), Some([10, 20, 30, 40]));
        assert_eq!(parse(&[0x31, 0xAA, 10], false), None);
    }
}
