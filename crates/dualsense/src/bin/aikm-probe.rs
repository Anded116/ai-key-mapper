//! Live view of a PlayStation controller.
//!
//! aikm-probe                      status line, refreshed 20 times a second
//! aikm-probe --json               one JSON object per line instead (for agents and scripts)
//! aikm-probe --seconds 5          stop after 5 seconds
//! aikm-probe --still 3            keep the pad still: measures gyro bias and noise
//! aikm-probe --lightbar ff3ca0    set the light bar colour
//! aikm-probe --triggers-off       turn adaptive trigger resistance off

use std::io::Write;
use std::time::{Duration, Instant};

use dualsense::{Pad, State};
use hidapi::HidApi;

struct Args {
    json: bool,
    seconds: Option<f64>,
    still: Option<f64>,
    lightbar: Option<[u8; 3]>,
    triggers_off: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args { json: false, seconds: None, still: None, lightbar: None, triggers_off: false };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "--json" => a.json = true,
            "--triggers-off" => a.triggers_off = true,
            "--seconds" => a.seconds = Some(value("--seconds")?.parse().map_err(|_| "bad --seconds")?),
            "--still" => a.still = Some(value("--still")?.parse().map_err(|_| "bad --still")?),
            "--lightbar" => {
                let hex = value("--lightbar")?;
                let n = u32::from_str_radix(hex.trim_start_matches('#'), 16).map_err(|_| "bad --lightbar")?;
                a.lightbar = Some([(n >> 16) as u8, (n >> 8) as u8, n as u8]);
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(a)
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let api = HidApi::new().unwrap_or_else(|e| fail(&format!("hidapi: {e}")));
    let mut pad = Pad::open_first(&api).unwrap_or_else(|e| fail(&e.to_string()));
    let calib = if pad.factory_calibration { "заводская" } else { "номинальная" };
    eprintln!("{} · {:?} · калибровка IMU: {calib}", pad.model.name(), pad.bus);
    if args.triggers_off {
        pad.disable_trigger_effects().unwrap_or_else(|e| fail(&e.to_string()));
        eprintln!("адаптивные триггеры выключены");
    }
    if let Some(rgb) = args.lightbar {
        pad.set_lightbar(rgb).unwrap_or_else(|e| fail(&e.to_string()));
    }
    if let Some(s) = args.still {
        measure_still(&mut pad, s);
        return;
    }
    live(&mut pad, &args);
}

fn fail(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1);
}

fn live(pad: &mut Pad, args: &Args) {
    let start = Instant::now();
    let mut last_print = Instant::now() - Duration::from_secs(1);
    let mut reports = 0u32;
    let mut rate_t0 = Instant::now();
    let mut rate = 0.0;
    let mut state = State::default();
    loop {
        if args.seconds.is_some_and(|s| start.elapsed().as_secs_f64() >= s) {
            break;
        }
        match pad.read(50) {
            Ok(Some(s)) => {
                state = s;
                reports += 1;
            }
            Ok(None) => {}
            Err(e) => fail(&e.to_string()),
        }
        if rate_t0.elapsed() >= Duration::from_millis(500) {
            rate = reports as f64 / rate_t0.elapsed().as_secs_f64();
            reports = 0;
            rate_t0 = Instant::now();
        }
        if last_print.elapsed() >= Duration::from_millis(50) {
            last_print = Instant::now();
            if args.json {
                println!("{}", to_json(&state, rate));
            } else {
                print!("\r{:<150}", status_line(&state, rate));
                let _ = std::io::stdout().flush();
            }
        }
    }
    if !args.json {
        println!();
    }
}

fn status_line(s: &State, rate: f64) -> String {
    let touch: Vec<String> = s.touch.iter().filter(|t| t.active).map(|t| format!("{},{}", t.x, t.y)).collect();
    format!(
        "{rate:4.0} Гц │ L {:+.2} {:+.2} │ R {:+.2} {:+.2} │ L2 {:.2} R2 {:.2} │ гиро {:+7.1} {:+7.1} {:+7.1} °/с │ акс {:+.2} {:+.2} {:+.2} g │ тач [{}] │ {}",
        s.left[0],
        s.left[1],
        s.right[0],
        s.right[1],
        s.l2,
        s.r2,
        s.gyro[0],
        s.gyro[1],
        s.gyro[2],
        s.accel[0],
        s.accel[1],
        s.accel[2],
        touch.join(" "),
        s.buttons,
    )
}

fn to_json(s: &State, rate: f64) -> String {
    let buttons: Vec<String> = s.buttons.names().map(|n| format!("\"{n}\"")).collect();
    let touch: Vec<String> =
        s.touch.iter().filter(|t| t.active).map(|t| format!("{{\"x\":{},\"y\":{}}}", t.x, t.y)).collect();
    let battery = s.battery.map_or("null".to_string(), |b| format!("{b:.2}"));
    format!(
        "{{\"rate_hz\":{rate:.0},\"buttons\":[{}],\"left\":[{:.3},{:.3}],\"right\":[{:.3},{:.3}],\"l2\":{:.3},\"r2\":{:.3},\
         \"gyro_dps\":[{:.2},{:.2},{:.2}],\"accel_g\":[{:.3},{:.3},{:.3}],\"touch\":[{}],\"battery\":{battery},\"charging\":{}}}",
        buttons.join(","),
        s.left[0],
        s.left[1],
        s.right[0],
        s.right[1],
        s.l2,
        s.r2,
        s.gyro[0],
        s.gyro[1],
        s.gyro[2],
        s.accel[0],
        s.accel[1],
        s.accel[2],
        touch.join(","),
        s.charging,
    )
}

/// Mean and standard deviation of the gyro while the pad lies still: the mean is the bias the
/// engine has to subtract, the deviation is the noise floor.
fn measure_still(pad: &mut Pad, seconds: f64) {
    let start = Instant::now();
    let (mut n, mut sum, mut sq, mut accel) = (0u32, [0f64; 3], [0f64; 3], [0f64; 3]);
    while start.elapsed().as_secs_f64() < seconds {
        if let Ok(Some(s)) = pad.read(50) {
            n += 1;
            for i in 0..3 {
                sum[i] += s.gyro[i] as f64;
                sq[i] += (s.gyro[i] as f64).powi(2);
                accel[i] += s.accel[i] as f64;
            }
        }
    }
    if n == 0 {
        fail("нет отчётов от геймпада");
    }
    let nf = n as f64;
    let mean: Vec<f64> = sum.iter().map(|v| v / nf).collect();
    let std: Vec<f64> = (0..3).map(|i| (sq[i] / nf - mean[i].powi(2)).max(0.0).sqrt()).collect();
    let acc: Vec<f64> = accel.iter().map(|v| v / nf).collect();
    let g = (acc[0].powi(2) + acc[1].powi(2) + acc[2].powi(2)).sqrt();
    println!("отчётов: {n} ({:.0} Гц)", nf / seconds);
    println!("смещение гироскопа, °/с: {:+.3} {:+.3} {:+.3}", mean[0], mean[1], mean[2]);
    println!("шум гироскопа (σ), °/с:  {:.3} {:.3} {:.3}", std[0], std[1], std[2]);
    println!("ускорение, g: {:+.3} {:+.3} {:+.3} (|a| = {g:.3}, в покое должно быть ≈1)", acc[0], acc[1], acc[2]);
}
