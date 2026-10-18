//! Input report parsing. Layouts follow Linux `hid-playstation.c` and SDL `SDL_hidapi_ps5.c`.

use crate::{Bus, Buttons, Model, State, Touch};

/// Raw IMU sample before calibration: gyro (pitch, yaw, roll) and accel (x, y, z).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RawImu {
    pub gyro: [i16; 3],
    pub accel: [i16; 3],
}

/// Parses one input report (first byte is the report id). `None` for reports that carry no
/// controller state, such as the short Bluetooth report before the extended mode is enabled.
pub fn parse(model: Model, bus: Bus, r: &[u8]) -> Option<(State, Option<RawImu>)> {
    match model {
        Model::DualSense | Model::DualSenseEdge => parse_dualsense(bus, r),
        Model::DualShock4 => parse_ds4(bus, r),
    }
}

fn le16(r: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([r[at], r[at + 1]])
}

fn axis(v: u8) -> f32 {
    (v as f32 - 127.5) / 127.5
}

/// Stick with +Y up (HID reports +Y down).
fn stick(x: u8, y: u8) -> [f32; 2] {
    [axis(x), -axis(y)]
}

fn hat(v: u8) -> Buttons {
    use Buttons as B;
    match v & 0x0F {
        0 => B::UP,
        1 => B::UP | B::RIGHT,
        2 => B::RIGHT,
        3 => B::RIGHT | B::DOWN,
        4 => B::DOWN,
        5 => B::DOWN | B::LEFT,
        6 => B::LEFT,
        7 => B::LEFT | B::UP,
        _ => B::NONE,
    }
}

/// Face buttons, shoulders and sticks share one bit layout on DualSense and DS4.
fn common_buttons(b0: u8, b1: u8) -> Buttons {
    use Buttons as B;
    let mut b = hat(b0);
    let bits = [
        (b0, 4, B::SQUARE),
        (b0, 5, B::CROSS),
        (b0, 6, B::CIRCLE),
        (b0, 7, B::TRIANGLE),
        (b1, 0, B::L1),
        (b1, 1, B::R1),
        (b1, 2, B::L2),
        (b1, 3, B::R2),
        (b1, 4, B::CREATE),
        (b1, 5, B::OPTIONS),
        (b1, 6, B::L3),
        (b1, 7, B::R3),
    ];
    for (byte, bit, flag) in bits {
        if byte & (1 << bit) != 0 {
            b |= flag;
        }
    }
    b
}

/// Touch point: bit 7 of the first byte is set when the finger is *not* on the pad.
fn touch(p: &[u8]) -> Touch {
    Touch {
        active: p[0] & 0x80 == 0,
        id: p[0] & 0x7F,
        x: p[1] as u16 | ((p[2] as u16 & 0x0F) << 8),
        y: (p[2] as u16 >> 4) | ((p[3] as u16) << 4),
    }
}

fn parse_dualsense(bus: Bus, r: &[u8]) -> Option<(State, Option<RawImu>)> {
    // USB 0x01 and Bluetooth 0x31 share one 63-byte body; Bluetooth adds a sequence byte.
    let o = match (bus, r.first()?) {
        (Bus::Usb, 0x01) => 1,
        (Bus::Bluetooth, 0x31) => 2,
        _ => return None,
    };
    let d = r.get(o..o + 53)?;
    let mut buttons = common_buttons(d[7], d[8]);
    if d[9] & 0x01 != 0 {
        buttons |= Buttons::PS;
    }
    if d[9] & 0x02 != 0 {
        buttons |= Buttons::TOUCHPAD;
    }
    if d[9] & 0x04 != 0 {
        buttons |= Buttons::MIC;
    }
    let imu = RawImu {
        gyro: [le16(d, 15), le16(d, 17), le16(d, 19)],
        accel: [le16(d, 21), le16(d, 23), le16(d, 25)],
    };
    let status = d[52];
    let state = State {
        buttons,
        left: stick(d[0], d[1]),
        right: stick(d[2], d[3]),
        l2: d[4] as f32 / 255.0,
        r2: d[5] as f32 / 255.0,
        touch: [touch(&d[32..36]), touch(&d[36..40])],
        battery: Some(((status & 0x0F) as f32 * 10.0 + 5.0).min(100.0) / 100.0),
        charging: (status >> 4) & 0x0F == 1,
        timestamp_us: u32::from_le_bytes([d[27], d[28], d[29], d[30]]) / 3,
        ..State::default()
    };
    Some((state, Some(imu)))
}

fn parse_ds4(bus: Bus, r: &[u8]) -> Option<(State, Option<RawImu>)> {
    // USB 0x01 body starts right after the id; Bluetooth 0x11 has two extra header bytes.
    let o = match (bus, r.first()?) {
        (Bus::Usb, 0x01) if r.len() >= 64 => 1,
        (Bus::Bluetooth, 0x11) => 3,
        _ => return None,
    };
    let d = r.get(o..o + 42)?;
    let mut buttons = common_buttons(d[4], d[5]);
    if d[6] & 0x01 != 0 {
        buttons |= Buttons::PS;
    }
    if d[6] & 0x02 != 0 {
        buttons |= Buttons::TOUCHPAD;
    }
    let imu = RawImu {
        gyro: [le16(d, 12), le16(d, 14), le16(d, 16)],
        accel: [le16(d, 18), le16(d, 20), le16(d, 22)],
    };
    let state = State {
        buttons,
        left: stick(d[0], d[1]),
        right: stick(d[2], d[3]),
        l2: d[7] as f32 / 255.0,
        r2: d[8] as f32 / 255.0,
        touch: [touch(&d[34..38]), touch(&d[38..42])],
        battery: Some(((d[29] & 0x0F) as f32 / 10.0).min(1.0)),
        charging: d[29] & 0x10 != 0,
        timestamp_us: u16::from_le_bytes([d[9], d[10]]) as u32 * 16 / 3,
        ..State::default()
    };
    Some((state, Some(imu)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dualsense_body() -> Vec<u8> {
        let mut d = vec![0u8; 63];
        d[0] = 255; // LX right
        d[1] = 0; // LY up
        d[2] = 128;
        d[3] = 128;
        d[4] = 0;
        d[5] = 255; // R2 full
        d[7] = 0x08 | 0x20; // hat released, cross
        d[8] = 0x01 | 0x08; // L1, R2
        d[9] = 0x04; // mic
        d[15..17].copy_from_slice(&100i16.to_le_bytes());
        d[17..19].copy_from_slice(&(-200i16).to_le_bytes());
        d[32] = 0x05; // finger 5 down
        d[33] = 0x34;
        d[34] = 0x12 | 0x50;
        d[35] = 0x2A;
        d[36] = 0x80; // second finger up
        d[52] = 0x08; // battery level 8, discharging
        d
    }

    #[test]
    fn dualsense_usb_and_bluetooth_match() {
        let body = dualsense_body();
        let usb: Vec<u8> = [vec![0x01], body.clone()].concat();
        let bt: Vec<u8> = [vec![0x31, 0x00], body, vec![0; 13]].concat();
        let (a, ia) = parse(Model::DualSense, Bus::Usb, &usb).unwrap();
        let (b, ib) = parse(Model::DualSense, Bus::Bluetooth, &bt).unwrap();
        assert_eq!(a, b);
        assert_eq!(ia, ib);
        assert!(a.buttons.contains(Buttons::CROSS | Buttons::L1 | Buttons::R2 | Buttons::MIC));
        assert!(!a.buttons.contains(Buttons::UP));
        assert!((a.left[0] - 1.0).abs() < 0.01 && (a.left[1] - 1.0).abs() < 0.01);
        assert!((a.r2 - 1.0).abs() < 1e-6);
        assert_eq!(ia.unwrap().gyro[..2], [100, -200]);
        assert!(a.touch[0].active && a.touch[0].id == 5);
        assert_eq!((a.touch[0].x, a.touch[0].y), (0x234, 0x2A5));
        assert!(!a.touch[1].active);
        assert_eq!(a.battery, Some(0.85));
    }

    #[test]
    fn short_bluetooth_report_is_ignored() {
        assert!(parse(Model::DualSense, Bus::Bluetooth, &[0x01, 128, 128, 128, 128, 8, 0, 0, 0, 0]).is_none());
    }

    #[test]
    fn dpad_diagonals() {
        assert_eq!(hat(1), Buttons::UP | Buttons::RIGHT);
        assert_eq!(hat(8), Buttons::NONE);
    }
}
