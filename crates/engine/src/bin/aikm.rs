//! aikm run <profile.toml> [--dry]   map the controller with one profile
//! aikm run <folder> [--dry]         pick the profile whose `exe` matches the window in focus
//! aikm check <profile.toml>         validate a profile and print what it binds
//!
//! While running: profiles reload when any .toml in their folder changes; PS + Create pauses
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
            eprintln!("usage: aikm run <profile.toml|folder> [--dry] | aikm check <profile.toml>");
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

/// Executable name of the window in focus, lower case (`testrange.exe`).
fn foreground_exe() -> Option<String> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    // SAFETY: plain Win32 queries on a handle we own; the buffer outlives the call.
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let res = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(process);
        res.ok()?;
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        Path::new(&path).file_name().map(|n| n.to_string_lossy().to_lowercase())
    }
}

/// Game profiles in a folder by `exe` (files starting with `_` are bases, not games).
fn profile_index(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut index = Vec::new();
    for e in entries.filter_map(|e| e.ok()) {
        let path = e.path();
        let is_game = path.extension().is_some_and(|x| x == "toml")
            && !path.file_name().is_some_and(|n| n.to_string_lossy().starts_with('_'));
        if !is_game {
            continue;
        }
        match Profile::load(&path) {
            Ok(p) => {
                if let Some(exe) = p.exe {
                    index.push((exe.to_lowercase(), path));
                }
            }
            Err(e) => eprintln!("\nпрофиль пропущен: {e}"),
        }
    }
    index
}

/// Profile for an exe. Variants inherit `exe` (testrange-hybrid.toml from testrange.toml), so
/// when several match, the file named after the exe wins.
fn pick_profile(index: &[(String, PathBuf)], exe: &str) -> Option<PathBuf> {
    let stem = Path::new(exe).file_stem()?.to_string_lossy().to_lowercase();
    let matches: Vec<&PathBuf> = index.iter().filter(|(e, _)| e == exe).map(|(_, p)| p).collect();
    let named = matches.iter().find(|p| p.file_stem().is_some_and(|s| s.to_string_lossy().to_lowercase() == stem));
    named.or(matches.first()).map(|p| (*p).clone())
}

/// Where profiles come from: one file, or a folder picked by the window in focus (JSM AutoLoad).
enum Source {
    File(PathBuf),
    Auto { dir: PathBuf, index: Vec<(String, PathBuf)>, last_exe: Option<String> },
}

fn run(path: &Path, dry: bool) {
    install_ctrl_handler();
    let mut source = if path.is_dir() {
        let index = profile_index(path);
        eprintln!("автозагрузка из {}: профилей игр {}", path.display(), index.len());
        Source::Auto { dir: path.to_path_buf(), index, last_exe: None }
    } else {
        Source::File(path.to_path_buf())
    };
    let watch_dir = match &source {
        Source::File(p) => p.parent().unwrap_or(Path::new(".")).to_path_buf(),
        Source::Auto { dir, .. } => dir.clone(),
    };
    let mut active: Option<PathBuf> = None;
    let mut engine: Option<Engine> = None;
    if let Source::File(p) = &source {
        engine = Some(Engine::new(load(p)));
        active = Some(p.clone());
    }

    let mut out = Out { dry, win: WinSink, rec: RecordSink::default() };
    let mut hider: Option<Hider> = None;
    let start = Instant::now();
    let mut mtime = profiles_mtime(&watch_dir.join("_"));
    let mut last_check = Instant::now();
    let mut last_focus = Instant::now() - Duration::from_secs(1);
    let mut last_status = Instant::now();
    let mut paused = false;
    let mut combo_was = false;
    let mut lightbar: Option<[u8; 3]> = None;
    eprintln!("{}", if dry { "сухой прогон: в систему ничего не уходит" } else { "вывод в систему" });
    eprintln!("PS + Create — пауза, Ctrl+C — выход. Держи геймпад неподвижно секунду: калибровка гироскопа.");

    let mut api = HidApi::new().unwrap_or_else(|e| {
        eprintln!("hidapi: {e}");
        std::process::exit(1);
    });
    let mut pad: Option<Pad> = None;
    while !STOP.load(Ordering::SeqCst) {
        let now = start.elapsed().as_secs_f64();

        if let Source::Auto { index, last_exe, .. } = &mut source {
            if last_focus.elapsed() >= Duration::from_millis(300) {
                last_focus = Instant::now();
                let exe = foreground_exe();
                if exe != *last_exe {
                    *last_exe = exe.clone();
                    // Like JSM: a window without a profile keeps the current one.
                    let hit = exe.and_then(|x| pick_profile(index, &x));
                    if let Some(p) = hit.filter(|p| active.as_ref() != Some(p)) {
                        match Profile::load(&p) {
                            Ok(profile) => {
                                eprintln!("\nпрофиль: {}", profile.game);
                                match &mut engine {
                                    Some(e) => e.set_profile(profile, &mut out),
                                    None => engine = Some(Engine::new(profile)),
                                }
                                active = Some(p);
                            }
                            Err(e) => eprintln!("\nпрофиль не загружен: {e}"),
                        }
                    }
                }
            }
        }

        let wants_hide = !dry && engine.as_ref().is_some_and(|e| e.profile.hide_controller);
        if wants_hide && hider.is_none() {
            hider = Hider::start();
        } else if !wants_hide {
            if let Some(h) = hider.take() {
                h.stop();
            }
        }

        if pad.is_none() {
            let _ = api.refresh_devices();
            match Pad::open_first(&api) {
                Ok(mut p) => {
                    eprintln!("\n{} · {:?}", p.model.name(), p.bus);
                    if let Err(e) = p.disable_trigger_effects() {
                        eprintln!("адаптивные триггеры не сброшены: {e}");
                    }
                    lightbar = None;
                    pad = Some(p);
                }
                Err(_) => {
                    std::thread::sleep(Duration::from_millis(500));
                    continue;
                }
            }
        }
        let read = pad.as_mut().map(|p| p.read(4));
        match (read, &mut engine) {
            (Some(Ok(Some(state))), Some(e)) => {
                let combo = state.buttons.contains(Buttons::PS | Buttons::CREATE);
                if combo && !combo_was {
                    paused = !paused;
                    e.release_all(&mut out);
                    eprintln!("\n{}", if paused { "пауза" } else { "работаю" });
                }
                combo_was = combo;
                if !paused {
                    e.update(now, &state, &mut out);
                }
            }
            (Some(Ok(None)), Some(e)) => e.tick(now, &mut out),
            (Some(Err(err)), _) => {
                eprintln!("\nгеймпад отключён: {err}");
                if let Some(e) = &mut engine {
                    e.release_all(&mut out);
                }
                pad = None;
            }
            _ => {}
        }

        if let (Some(e), Some(p)) = (&engine, &mut pad) {
            if let Some(want) = e.lightbar().filter(|c| lightbar != Some(*c)) {
                if p.set_lightbar(want).is_ok() {
                    lightbar = Some(want);
                }
            }
        }

        if dry && !out.rec.events.is_empty() {
            for (o, down) in out.rec.events.drain(..) {
                eprintln!("\n{:8.3} {}{}", now, if down { "+" } else { "−" }, o);
            }
        }
        if last_check.elapsed() >= Duration::from_millis(500) {
            last_check = Instant::now();
            let m = profiles_mtime(&watch_dir.join("_"));
            if m != mtime {
                mtime = m;
                if let Source::Auto { dir, index, .. } = &mut source {
                    *index = profile_index(dir);
                }
                if let (Some(p), Some(e)) = (&active, &mut engine) {
                    match Profile::load(p) {
                        Ok(profile) => {
                            e.set_profile(profile, &mut out);
                            eprintln!("\nпрофиль перечитан");
                        }
                        Err(err) => eprintln!("\nпрофиль не перечитан: {err}"),
                    }
                }
            }
        }
        if let Some(e) = &mut engine {
            if let Some(r) = e.take_calibration() {
                match r.bias {
                    Some(b) => eprintln!(
                        "\nгироскоп откалиброван: смещение {:+.3} {:+.3} {:+.3} °/с по {} отсчётам",
                        b[0], b[1], b[2], r.samples
                    ),
                    None => eprintln!("\nкалибровка не принята: мало неподвижных отсчётов ({}), смещение прежнее", r.samples),
                }
            }
        }
        if last_status.elapsed() >= Duration::from_millis(100) {
            last_status = Instant::now();
            let line = match &engine {
                None => "жду окно игры с профилем…".to_string(),
                Some(e) => {
                    let s = e.snapshot();
                    let outs: Vec<_> = s.outputs.iter().map(|o| o.name()).collect();
                    format!(
                        "{} │ слой {:<4} │ пара {:<8} │ движение {:?} │ гироскоп {} │ флик {} │ держу: {}",
                        e.profile.game,
                        s.layer.map_or("—", |b| b.name()),
                        s.next_pair.map_or("—", |b| b.name()),
                        s.movement,
                        if s.gyro_calibrating { "калибровка" } else { "ок" },
                        if s.flick_active { "да " } else { "нет" },
                        outs.join(" ")
                    )
                }
            };
            eprint!("\r{line:<130}");
        }
    }

    if let Some(e) = &mut engine {
        e.release_all(&mut out);
    }
    eprintln!("\nвыход: все клавиши отпущены");
    if let Some(h) = hider {
        h.stop();
    }
}
