//! aikm run <profile.toml> [--dry]   map the controller with one profile
//! aikm run <folder> [--dry]         pick the profile whose `exe` matches the window in focus
//! aikm check <profile.toml>         validate a profile and print what it binds
//!
//! While running: profiles reload when any .toml in their folder changes; PS + Create pauses
//! and resumes; Ctrl+C releases every key before exiting.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use engine::runner::{self, Config, Frame};
use engine::Profile;

static STOP: AtomicBool = AtomicBool::new(false);

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dry = args.iter().any(|a| a == "--dry");
    let rest: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    match rest.as_slice() {
        [cmd, path] if cmd.as_str() == "check" => check(Path::new(path)),
        [cmd, path] if cmd.as_str() == "run" => run(Path::new(path), dry),
        _ => {
            eprintln!("usage: aikm run <profile.toml|folder> [--dry] | aikm check <profile.toml>");
            std::process::exit(2);
        }
    }
}

fn check(path: &Path) {
    let p = Profile::load(path).unwrap_or_else(|e| {
        eprintln!("профиль: {e}");
        std::process::exit(1);
    });
    println!("{} · {}", p.game, p.exe.as_deref().unwrap_or("exe не указан"));
    println!(
        "калибровка: {} отсч/° при чувствительности {} → {:.2} отсч/°",
        p.calibration.real_world,
        p.calibration.in_game_sens,
        p.calibration.counts_per_degree()
    );
    println!("движение: {:?} · гироскоп {}–{} · флик {}", p.movement.mode, p.gyro.min_sens, p.gyro.max_sens, p.flick.enabled);
    let mut buttons: Vec<_> = p.buttons.iter().collect();
    buttons.sort_by_key(|(b, _)| **b);
    for (b, binding) in buttons {
        println!("  {:<14} {}", b.name(), describe(binding));
    }
    for layer in &p.layers {
        println!("слой {}:", layer.button.name());
        let mut bs: Vec<_> = layer.bindings.iter().collect();
        bs.sort_by_key(|(b, _)| **b);
        for (b, binding) in bs {
            println!("  {:<14} {}", b.name(), describe(binding));
        }
    }
}

fn describe(b: &engine::profile::Binding) -> String {
    let list = |v: &Vec<engine::Output>| v.iter().map(|o| o.name()).collect::<Vec<_>>().join("+");
    let mut parts = Vec::new();
    for (name, v) in [("", &b.press), ("тап ", &b.tap), ("удерж. ", &b.hold), ("×2 ", &b.double), ("повтор ", &b.next), ("до остановки ", &b.latch)] {
        if !v.is_empty() {
            parts.push(format!("{name}{}", list(v)));
        }
    }
    parts.join(" · ")
}

fn install_ctrl_handler() {
    use windows::core::BOOL;
    use windows::Win32::System::Console::SetConsoleCtrlHandler;
    unsafe extern "system" fn handler(_: u32) -> BOOL {
        STOP.store(true, Ordering::SeqCst);
        BOOL(1)
    }
    // SAFETY: the handler only touches an atomic.
    unsafe {
        let _ = SetConsoleCtrlHandler(Some(handler), true);
    }
}

fn status(f: &Frame) -> String {
    let (Some(s), Some(game)) = (&f.snapshot, &f.profile) else {
        return "жду окно игры с профилем…".to_string();
    };
    let outs: Vec<_> = s.outputs.iter().map(|o| o.name()).collect();
    format!(
        "{}{}{} │ слой {:<4} │ пара {:<8} │ движение {:?} │ гироскоп {} │ флик {} │ держу: {}",
        game,
        if f.focused { "" } else { " (не в фокусе)" },
        if f.paused { " (пауза)" } else { "" },
        s.layer.map_or("—", |b| b.name()),
        s.next_pair.map_or("—", |b| b.name()),
        s.movement,
        if s.gyro_calibrating { "калибровка" } else { "ок" },
        if s.flick_active { "да " } else { "нет" },
        outs.join(" ")
    )
}

fn run(path: &Path, dry: bool) {
    install_ctrl_handler();
    eprintln!("{}", if dry { "сухой прогон: в систему ничего не уходит" } else { "вывод в систему" });
    eprintln!("PS + Create — пауза, Ctrl+C — выход. Держи геймпад неподвижно секунду: калибровка гироскопа.");

    let hidhide_tool = std::env::current_exe().ok().map(|me| me.with_file_name("aikm-hidhide.exe"));
    let config = Config { source: path.to_path_buf(), dry, hidhide_tool };
    let start = Instant::now();
    let mut last_status = Instant::now();
    let on_frame = Box::new(move |f: &Frame| {
        for (o, down) in &f.dry_events {
            eprintln!("\n{:8.3} {}{}", start.elapsed().as_secs_f64(), if *down { "+" } else { "−" }, o);
        }
        if last_status.elapsed() >= Duration::from_millis(100) {
            last_status = Instant::now();
            eprint!("\r{:<130}", status(f));
        }
    });
    let on_log = Box::new(|msg: &str| eprintln!("\n{msg}"));
    let handle = runner::spawn(config, on_frame, on_log);
    while !STOP.load(Ordering::SeqCst) && !handle.finished() {
        std::thread::sleep(Duration::from_millis(50));
    }
    handle.stop();
}
