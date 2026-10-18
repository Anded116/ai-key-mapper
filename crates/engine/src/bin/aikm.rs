//! aikm run <profile.toml> [--dry]   map the controller to keyboard and mouse
//! aikm check <profile.toml>         validate a profile and print what it binds
//!
//! While running: the profile reloads when any .toml next to it changes; PS + Create pauses
//! and resumes; Ctrl+C releases every key before exiting.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use dualsense::{Buttons, Pad};
use engine::output::{RecordSink, Sink, WinSink};
use engine::{Engine, Profile};
use hidapi::HidApi;

static STOP: AtomicBool = AtomicBool::new(false);

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dry = args.iter().any(|a| a == "--dry");
    let rest: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    match rest.as_slice() {
        [cmd, path] if cmd.as_str() == "check" => check(Path::new(path)),
        [cmd, path] if cmd.as_str() == "run" => run(Path::new(path), dry),
        _ => {
            eprintln!("usage: aikm run <profile.toml> [--dry] | aikm check <profile.toml>");
            std::process::exit(2);
        }
    }
}

fn load(path: &Path) -> Profile {
    Profile::load(path).unwrap_or_else(|e| {
        eprintln!("профиль: {e}");
        std::process::exit(1);
    })
}

fn check(path: &Path) {
    let p = load(path);
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
        println!("  {:<10} {}", b.name(), describe(binding));
    }
    for layer in &p.layers {
        println!("слой {}:", layer.button.name());
        let mut bs: Vec<_> = layer.bindings.iter().collect();
        bs.sort_by_key(|(b, _)| **b);
        for (b, binding) in bs {
            println!("  {:<10} {}", b.name(), describe(binding));
        }
    }
}

fn describe(b: &engine::profile::Binding) -> String {
    let list = |v: &Vec<engine::Output>| v.iter().map(|o| o.name()).collect::<Vec<_>>().join("+");
    let mut parts = Vec::new();
    for (name, v) in [("", &b.press), ("тап ", &b.tap), ("удерж. ", &b.hold), ("×2 ", &b.double), ("повтор ", &b.next)] {
        if !v.is_empty() {
            parts.push(format!("{name}{}", list(v)));
        }
    }
    parts.join(" · ")
}

/// Latest modification time among the profile folder's .toml files (base profiles included).
fn profiles_mtime(path: &Path) -> Option<SystemTime> {
    let dir = path.parent()?;
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "toml"))
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .max()
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

/// Real output, or a dry run that only records what would be sent.
struct Out {
    dry: bool,
    win: WinSink,
    rec: RecordSink,
}

impl Sink for Out {
    fn key(&mut self, out: engine::Output, down: bool) {
        if self.dry {
            self.rec.key(out, down);
        } else {
            self.win.key(out, down);
        }
    }

    fn mouse_move(&mut self, dx: i32, dy: i32) {
        if self.dry {
            self.rec.mouse_move(dx, dy);
        } else {
            self.win.mouse_move(dx, dy);
        }
    }
}

/// Hides the controller from games through `aikm-hidhide hide --hold`, keeping aikm itself on the
/// whitelist. The helper's guard undoes the hiding even if aikm is killed.
struct Hider {
    tool: PathBuf,
    child: Child,
}

impl Hider {
    fn start() -> Option<Self> {
        let me = std::env::current_exe().ok()?;
        let tool = me.with_file_name("aikm-hidhide.exe");
        if !tool.exists() {
            eprintln!("скрытие геймпада: нет {} — собери hidhide-cli", tool.display());
            return None;
        }
        let child = Command::new(&tool)
            .args(["hide", "--hold", "--allow"])
            .arg(&me)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn();
        match child {
            Ok(child) => {
                eprintln!("геймпад скрыт от игр (HidHide); aikm его видит");
                Some(Self { tool, child })
            }
            Err(e) => {
                eprintln!("скрытие геймпада не запустилось: {e}");
                None
            }
        }
    }

    fn stop(mut self) {
        // `restore` undoes the journal; the holding helper notices and exits by itself.
        let _ = Command::new(&self.tool).arg("restore").stdout(Stdio::null()).status();
        for _ in 0..20 {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                eprintln!("геймпад снова виден играм");
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = self.child.kill();
    }
}

fn run(path: &Path, dry: bool) {
    install_ctrl_handler();
    let path: PathBuf = path.to_path_buf();
    let mut engine = Engine::new(load(&path));
    let hider = if engine.profile.hide_controller && !dry { Hider::start() } else { None };
    let mut out = Out { dry, win: WinSink, rec: RecordSink::default() };
    let start = Instant::now();
    let mut mtime = profiles_mtime(&path);
    let mut last_check = Instant::now();
    let mut last_status = Instant::now();
    let mut paused = false;
    let mut combo_was = false;
    eprintln!("профиль: {} · {}", engine.profile.game, if dry { "сухой прогон" } else { "вывод в систему" });
    eprintln!("PS + Create — пауза, Ctrl+C — выход. Держи геймпад неподвижно секунду: калибровка гироскопа.");

    let mut api = HidApi::new().unwrap_or_else(|e| {
        eprintln!("hidapi: {e}");
        std::process::exit(1);
    });
    let mut pad: Option<Pad> = None;
    while !STOP.load(Ordering::SeqCst) {
        let now = start.elapsed().as_secs_f64();
        if pad.is_none() {
            let _ = api.refresh_devices();
            match Pad::open_first(&api) {
                Ok(mut p) => {
                    eprintln!("\n{} · {:?}", p.model.name(), p.bus);
                    if let Err(e) = p.disable_trigger_effects() {
                        eprintln!("адаптивные триггеры не сброшены: {e}");
                    }
                    pad = Some(p);
                }
                Err(_) => {
                    std::thread::sleep(Duration::from_millis(500));
                    continue;
                }
            }
        }
        let read = pad.as_mut().map(|p| p.read(4));
        match read {
            Some(Ok(Some(state))) => {
                let combo = state.buttons.contains(Buttons::PS | Buttons::CREATE);
                if combo && !combo_was {
                    paused = !paused;
                    engine.release_all(&mut out);
                    eprintln!("\n{}", if paused { "пауза" } else { "работаю" });
                }
                combo_was = combo;
                if !paused {
                    engine.update(now, &state, &mut out);
                }
            }
            Some(Ok(None)) => engine.tick(now, &mut out),
            Some(Err(e)) => {
                eprintln!("\nгеймпад отключён: {e}");
                engine.release_all(&mut out);
                pad = None;
            }
            None => {}
        }
        if dry && !out.rec.events.is_empty() {
            for (o, down) in out.rec.events.drain(..) {
                eprintln!("\n{:8.3} {}{}", now, if down { "+" } else { "−" }, o);
            }
        }
        if last_check.elapsed() >= Duration::from_millis(500) {
            last_check = Instant::now();
            let m = profiles_mtime(&path);
            if m != mtime {
                mtime = m;
                match Profile::load(&path) {
                    Ok(p) => {
                        engine.set_profile(p, &mut out);
                        eprintln!("\nпрофиль перечитан");
                    }
                    Err(e) => eprintln!("\nпрофиль не перечитан: {e}"),
                }
            }
        }
        if let Some(r) = engine.take_calibration() {
            match r.bias {
                Some(b) => eprintln!("\nгироскоп откалиброван: смещение {:+.3} {:+.3} {:+.3} °/с по {} отсчётам", b[0], b[1], b[2], r.samples),
                None => eprintln!("\nкалибровка не принята: мало неподвижных отсчётов ({}), смещение прежнее", r.samples),
            }
        }
        if last_status.elapsed() >= Duration::from_millis(100) {
            last_status = Instant::now();
            let s = engine.snapshot();
            let outs: Vec<_> = s.outputs.iter().map(|o| o.name()).collect();
            let gyro = if s.gyro_calibrating { "калибровка".to_string() } else { "ок".to_string() };
            eprint!(
                "\r{:<110}",
                format!(
                    "слой {:<6} │ пара {:<8} │ гироскоп {gyro} │ флик {} │ держу: {}",
                    s.layer.map_or("—", |b| b.name()),
                    s.next_pair.map_or("—", |b| b.name()),
                    if s.flick_active { "да " } else { "нет" },
                    outs.join(" ")
                )
            );
        }
    }

    engine.release_all(&mut out);
    eprintln!("\nвыход: все клавиши отпущены");
    if let Some(h) = hider {
        h.stop();
    }
}
