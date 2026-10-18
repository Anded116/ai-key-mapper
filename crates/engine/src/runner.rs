//! The mapping loop shared by the `aikm` CLI and the app window: opens the controller, picks the
//! profile, runs the engine while its game is in focus, hides the pad when the profile asks.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command as Process, Stdio};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};

use dualsense::{Bus, Buttons, Model, Pad, State};
use hidapi::HidApi;

use crate::output::{RecordSink, Sink, WinSink};
use crate::profile::MoveMode;
use crate::steam;
use crate::{Engine, Output, Profile, Snapshot};

pub struct Config {
    /// A profile file, or a folder: then the profile is picked by the window in focus.
    pub source: PathBuf,
    /// Record outputs instead of sending them.
    pub dry: bool,
    /// `aikm-hidhide.exe`; without it profiles cannot hide the pad.
    pub hidhide_tool: Option<PathBuf>,
}

pub enum Command {
    SetPaused(bool),
    /// Like a tap on PS: start gyro calibration, or finish the running one.
    ToggleCalibration,
    Stop,
}

#[derive(Clone, Debug)]
pub struct PadInfo {
    pub model: Model,
    pub bus: Bus,
}

/// What the loop reports about ~60 times a second.
#[derive(Clone, Debug, Default)]
pub struct Frame {
    pub pad: Option<PadInfo>,
    pub state: Option<State>,
    pub snapshot: Option<Snapshot>,
    pub profile: Option<String>,
    /// File of the active profile; `profile_rev` grows each time it is (re)loaded.
    pub profile_path: Option<PathBuf>,
    pub profile_rev: u32,
    pub focused: bool,
    pub paused: bool,
    pub hidden: bool,
    /// Outputs recorded since the last frame in a dry run.
    pub dry_events: Vec<(Output, bool)>,
}

pub struct Handle {
    tx: Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

impl Handle {
    pub fn send(&self, cmd: Command) {
        let _ = self.tx.send(cmd);
    }

    /// The loop has ended by itself (no HID access).
    pub fn finished(&self) -> bool {
        self.thread.as_ref().is_none_or(|t| t.is_finished())
    }

    /// Stops the loop and waits until every key is released and the pad is visible again.
    pub fn stop(mut self) {
        let _ = self.tx.send(Command::Stop);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Stop);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

pub type OnFrame = Box<dyn FnMut(&Frame) + Send>;
pub type OnLog = Box<dyn FnMut(&str) + Send>;

pub fn spawn(config: Config, on_frame: OnFrame, on_log: OnLog) -> Handle {
    let (tx, rx) = mpsc::channel();
    let thread = thread::spawn(move || Loop::new(config, on_frame, on_log).run(rx));
    Handle { tx, thread: Some(thread) }
}

/// Latest modification time among the .toml files of a folder (base profiles included).
fn folder_mtime(dir: &Path) -> Option<SystemTime> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "toml"))
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .max()
}

/// Full path of a process's executable.
pub fn process_path(pid: u32) -> Option<PathBuf> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    // SAFETY: a process handle we own; the buffer outlives the call.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let res = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(process);
        res.ok()?;
        Some(PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
    }
}

/// Executable name of the window in focus, lower case (`testrange.exe`).
pub fn foreground_exe() -> Option<String> {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    let mut pid = 0u32;
    // SAFETY: plain Win32 queries about the foreground window.
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return None;
        }
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    if pid == 0 {
        return None;
    }
    process_path(pid)?.file_name().map(|n| n.to_string_lossy().to_lowercase())
}

/// Running processes: executable name in lower case → process id.
pub fn running_exes() -> HashMap<String, u32> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    let mut exes = HashMap::new();
    // SAFETY: a snapshot handle we own, walked with a correctly sized entry.
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return exes };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut entry).is_ok();
        while ok {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            exes.insert(String::from_utf16_lossy(&entry.szExeFile[..len]).to_lowercase(), entry.th32ProcessID);
            ok = Process32NextW(snap, &mut entry).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    exes
}

/// A game profile found in the profiles folder.
pub struct GameProfile {
    /// Lower case.
    pub exe: String,
    pub path: PathBuf,
    /// Hides the pad from the game from its start: `hide = true` with the stick on WASD.
    pub hides: bool,
}

/// Game profiles in a folder by `exe` (files starting with `_` are bases, not games).
pub fn profile_index(dir: &Path, log: &mut dyn FnMut(&str)) -> Vec<GameProfile> {
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
                if let Some(exe) = &p.exe {
                    let hides = p.hide_controller && p.movement.mode == MoveMode::Wasd;
                    index.push(GameProfile { exe: exe.to_lowercase(), path, hides });
                }
            }
            Err(e) => log(&format!("профиль пропущен: {e}")),
        }
    }
    index
}

/// Profile for an exe. Variants inherit `exe` (testrange-hybrid.toml from testrange.toml), so
/// when several match, the file named after the exe wins.
pub fn pick_profile<'a>(index: &'a [GameProfile], exe: &str) -> Option<&'a GameProfile> {
    let stem = Path::new(exe).file_stem()?.to_string_lossy().to_lowercase();
    let matches: Vec<&GameProfile> = index.iter().filter(|g| g.exe == exe).collect();
    let named = matches.iter().find(|g| g.path.file_stem().is_some_and(|s| s.to_string_lossy().to_lowercase() == stem));
    named.or(matches.first()).copied()
}

/// Real output, or a dry run that only records what would be sent.
struct Out {
    dry: bool,
    win: WinSink,
    rec: RecordSink,
}

impl Sink for Out {
    fn key(&mut self, out: Output, down: bool) {
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

/// Hides the controller from games through `aikm-hidhide hide --hold`, keeping this process on
/// the whitelist. The helper's guard undoes the hiding even if we are killed.
struct Hider {
    tool: PathBuf,
    child: Child,
}

impl Hider {
    fn start(tool: &Path, log: &mut dyn FnMut(&str)) -> Option<Self> {
        if !tool.exists() {
            log(&format!("скрытие геймпада: нет {} — собери hidhide-cli", tool.display()));
            return None;
        }
        let me = std::env::current_exe().ok()?;
        let child = Process::new(tool)
            .args(["hide", "--hold", "--allow"])
            .arg(&me)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn();
        match child {
            Ok(child) => {
                log("геймпад скрыт от игр (HidHide); маппер его видит");
                Some(Self { tool: tool.to_path_buf(), child })
            }
            Err(e) => {
                log(&format!("скрытие геймпада не запустилось: {e}"));
                None
            }
        }
    }

    fn stop(mut self, log: &mut dyn FnMut(&str)) {
        // `restore` undoes the journal; the holding helper notices and exits by itself.
        let _ = Process::new(&self.tool).arg("restore").stdout(Stdio::null()).status();
        for _ in 0..20 {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                log("геймпад снова виден играм");
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
        let _ = self.child.kill();
    }
}

/// Where profiles come from: one file, or a folder picked by the window in focus (JSM AutoLoad).
enum Source {
    File(PathBuf),
    Auto { dir: PathBuf, index: Vec<GameProfile>, last_exe: Option<String> },
}

struct Loop {
    config: Config,
    on_frame: OnFrame,
    on_log: OnLog,
    source: Source,
    watch_dir: PathBuf,
    active: Option<PathBuf>,
    engine: Option<Engine>,
    out: Out,
    hider: Option<Hider>,
    pad: Option<Pad>,
    pad_info: Option<PadInfo>,
    last_state: Option<State>,
    fg_exe: Option<String>,
    focused: bool,
    paused: bool,
    combo_was: bool,
    lightbar: Option<[u8; 3]>,
    waiting_pad: bool,
    /// Running game whose profile hides the pad from it (auto mode).
    hide_game: Option<PathBuf>,
    profile_rev: u32,
}

impl Loop {
    fn new(config: Config, on_frame: OnFrame, mut on_log: OnLog) -> Self {
        let source = if config.source.is_dir() {
            let index = profile_index(&config.source, &mut *on_log);
            on_log(&format!("автозагрузка из {}: профилей игр {}", config.source.display(), index.len()));
            Source::Auto { dir: config.source.clone(), index, last_exe: None }
        } else {
            Source::File(config.source.clone())
        };
        let watch_dir = match &source {
            Source::File(p) => p.parent().unwrap_or(Path::new(".")).to_path_buf(),
            Source::Auto { dir, .. } => dir.clone(),
        };
        let mut engine = None;
        let mut active = None;
        if let Source::File(p) = &source {
            match Profile::load(p) {
                Ok(profile) => {
                    engine = Some(Engine::new(profile));
                    active = Some(p.clone());
                }
                Err(e) => on_log(&format!("профиль: {e}")),
            }
        }
        let dry = config.dry;
        Self {
            config,
            on_frame,
            on_log,
            source,
            watch_dir,
            active,
            engine,
            out: Out { dry, win: WinSink, rec: RecordSink::default() },
            hider: None,
            pad: None,
            pad_info: None,
            last_state: None,
            fg_exe: None,
            focused: false,
            paused: false,
            combo_was: false,
            lightbar: None,
            waiting_pad: false,
            hide_game: None,
            profile_rev: 0,
        }
    }

    fn log(&mut self, msg: &str) {
        (self.on_log)(msg);
    }

    fn run(mut self, rx: Receiver<Command>) {
        let start = Instant::now();
        let mut mtime = folder_mtime(&self.watch_dir);
        let mut last_check = Instant::now();
        let mut last_focus = Instant::now() - Duration::from_secs(1);
        let mut last_frame = Instant::now();
        let mut api = match HidApi::new() {
            Ok(api) => api,
            Err(e) => {
                self.log(&format!("hidapi: {e}"));
                return;
            }
        };
        loop {
            match rx.try_recv() {
                Ok(Command::Stop) | Err(TryRecvError::Disconnected) => break,
                Ok(Command::SetPaused(p)) => self.set_paused(p),
                Ok(Command::ToggleCalibration) => {
                    if let Some(e) = &mut self.engine {
                        e.toggle_calibration(start.elapsed().as_secs_f64());
                    }
                }
                Err(TryRecvError::Empty) => {}
            }
            let now = start.elapsed().as_secs_f64();

            if last_focus.elapsed() >= Duration::from_millis(250) {
                last_focus = Instant::now();
                self.follow_focus();
                self.follow_games();
            }
            self.follow_hiding();

            if self.pad.is_none() && !self.open_pad(&mut api) {
                self.emit_frame();
                thread::sleep(Duration::from_millis(500));
                continue;
            }
            self.step(now);

            if last_check.elapsed() >= Duration::from_millis(500) {
                last_check = Instant::now();
                let m = folder_mtime(&self.watch_dir);
                if m != mtime {
                    mtime = m;
                    self.reload();
                }
            }
            self.report_calibration();
            if last_frame.elapsed() >= Duration::from_millis(16) {
                last_frame = Instant::now();
                self.emit_frame();
            }
        }
        if let Some(e) = &mut self.engine {
            e.release_all(&mut self.out);
        }
        self.log("выход: все клавиши отпущены");
        if let Some(h) = self.hider.take() {
            h.stop(&mut *self.on_log);
        }
    }

    fn set_paused(&mut self, paused: bool) {
        if paused == self.paused {
            return;
        }
        self.paused = paused;
        if let Some(e) = &mut self.engine {
            e.release_all(&mut self.out);
        }
        self.log(if paused { "пауза" } else { "работаю" });
    }

    fn follow_focus(&mut self) {
        self.fg_exe = foreground_exe();
        let mut switch_to = None;
        if let Source::Auto { index, last_exe, .. } = &mut self.source {
            if self.fg_exe != *last_exe {
                *last_exe = self.fg_exe.clone();
                let hit = self.fg_exe.as_deref().and_then(|x| pick_profile(index, x));
                switch_to = hit.map(|g| g.path.clone()).filter(|p| self.active.as_ref() != Some(p));
            }
        }
        if let Some(p) = switch_to {
            match Profile::load(&p) {
                Ok(profile) => {
                    self.log(&format!("профиль: {}", profile.game));
                    match &mut self.engine {
                        Some(e) => e.set_profile(profile, &mut self.out),
                        None => self.engine = Some(Engine::new(profile)),
                    }
                    self.active = Some(p);
                    self.profile_rev += 1;
                }
                Err(e) => self.log(&format!("профиль не загружен: {e}")),
            }
        }
        // A profile only acts while its game is in focus: no gyro mouse on the desktop.
        let fg = self.fg_exe.clone();
        let in_game = self.engine.as_ref().is_some_and(|e| {
            e.profile.exe.as_ref().is_none_or(|exe| fg.as_deref() == Some(exe.to_lowercase().as_str()))
        });
        if in_game != self.focused {
            self.focused = in_game;
            if let Some(e) = &mut self.engine {
                if !in_game {
                    e.release_all(&mut self.out);
                }
            }
            if self.engine.is_some() {
                self.log(if in_game { "игра в фокусе — работаю" } else { "игра не в фокусе — движок молчит" });
            }
        }
    }

    /// A game must not see the pad from its very start: once it has opened the device, hiding
    /// no longer reaches it. So in auto mode hiding follows the game's process, not its window.
    fn follow_games(&mut self) {
        let Source::Auto { index, .. } = &self.source else { return };
        let running = running_exes();
        let game = index
            .iter()
            .filter(|g| running.contains_key(&g.exe))
            .filter_map(|g| pick_profile(index, &g.exe))
            .find(|g| g.hides)
            .map(|g| (g.path.clone(), g.exe.clone()));
        let path = game.as_ref().map(|(p, _)| p.clone());
        if path != self.hide_game {
            if let Some((_, exe)) = &game {
                self.log(&format!("запущена {exe}"));
                let warning = running.get(exe).and_then(|&pid| process_path(pid)).and_then(|p| steam::input_warning(&p));
                if let Some(w) = warning {
                    self.log(&w);
                }
            }
            self.hide_game = path;
        }
    }

    fn follow_hiding(&mut self) {
        let engine_wants = self.engine.as_ref().is_some_and(|e| e.wants_hidden());
        let wants = !self.config.dry
            && match (&self.source, &self.hide_game) {
                (Source::File(_), _) => engine_wants,
                // R2 + L3 in the running game itself can still give the stick back to it.
                (Source::Auto { .. }, Some(game)) => self.active.as_ref() != Some(game) || engine_wants,
                (Source::Auto { .. }, None) => false,
            };
        if wants && self.hider.is_none() {
            if let Some(tool) = self.config.hidhide_tool.clone() {
                self.hider = Hider::start(&tool, &mut *self.on_log);
            }
        } else if !wants {
            if let Some(h) = self.hider.take() {
                h.stop(&mut *self.on_log);
            }
        }
    }

    fn open_pad(&mut self, api: &mut HidApi) -> bool {
        let _ = api.refresh_devices();
        match Pad::open_first(api) {
            Ok(mut p) => {
                self.waiting_pad = false;
                self.log(&format!("{} · {:?}", p.model.name(), p.bus));
                if let Err(e) = p.disable_trigger_effects() {
                    self.log(&format!("адаптивные триггеры не сброшены: {e}"));
                }
                self.pad_info = Some(PadInfo { model: p.model, bus: p.bus });
                self.lightbar = None;
                self.pad = Some(p);
                true
            }
            Err(_) => {
                if !self.waiting_pad {
                    self.waiting_pad = true;
                    self.log("геймпад не найден: не подключён или скрыт HidHide от этой программы. Жду…");
                }
                false
            }
        }
    }

    fn step(&mut self, now: f64) {
        let read = self.pad.as_mut().map(|p| p.read(4));
        match read {
            Some(Ok(Some(state))) => {
                self.last_state = Some(state);
                let combo = state.buttons.contains(Buttons::PS | Buttons::CREATE);
                if combo && !self.combo_was {
                    let p = !self.paused;
                    self.set_paused(p);
                }
                self.combo_was = combo;
                if let Some(e) = &mut self.engine {
                    if !self.focused {
                        e.idle(now, &state);
                    } else if !self.paused {
                        e.update(now, &state, &mut self.out);
                    }
                }
            }
            Some(Ok(None)) => {
                if let (Some(e), true) = (&mut self.engine, self.focused) {
                    e.tick(now, &mut self.out);
                }
            }
            Some(Err(err)) => {
                self.log(&format!("геймпад отключён: {err}"));
                if let Some(e) = &mut self.engine {
                    e.release_all(&mut self.out);
                }
                self.pad = None;
                self.pad_info = None;
                self.last_state = None;
            }
            None => {}
        }
        if let (Some(e), Some(p)) = (&self.engine, &mut self.pad) {
            if let Some(want) = e.lightbar().filter(|c| self.lightbar != Some(*c)) {
                if p.set_lightbar(want).is_ok() {
                    self.lightbar = Some(want);
                }
            }
        }
    }

    fn reload(&mut self) {
        if let Source::Auto { dir, index, .. } = &mut self.source {
            let dir = dir.clone();
            *index = profile_index(&dir, &mut *self.on_log);
        }
        let Some(p) = self.active.clone() else { return };
        match Profile::load(&p) {
            Ok(profile) => {
                if let Some(e) = &mut self.engine {
                    e.set_profile(profile, &mut self.out);
                }
                self.profile_rev += 1;
                self.log("профиль перечитан");
            }
            Err(err) => self.log(&format!("профиль не перечитан: {err}")),
        }
    }

    fn report_calibration(&mut self) {
        let Some(r) = self.engine.as_mut().and_then(|e| e.take_calibration()) else { return };
        let msg = match r.bias {
            Some(b) => format!(
                "гироскоп откалиброван: смещение {:+.3} {:+.3} {:+.3} °/с по {} отсчётам",
                b[0], b[1], b[2], r.samples
            ),
            None => format!("калибровка не принята: мало неподвижных отсчётов ({}), смещение прежнее", r.samples),
        };
        self.log(&msg);
    }

    fn emit_frame(&mut self) {
        let frame = Frame {
            pad: self.pad_info.clone(),
            state: self.last_state,
            snapshot: self.engine.as_ref().map(|e| e.snapshot()),
            profile: self.engine.as_ref().map(|e| e.profile.game.clone()),
            profile_path: self.engine.as_ref().and(self.active.clone()),
            profile_rev: self.profile_rev,
            focused: self.focused,
            paused: self.paused,
            hidden: self.hider.is_some(),
            dry_events: std::mem::take(&mut self.out.rec.events),
        };
        (self.on_frame)(&frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_exes_include_this_test() {
        let me = std::env::current_exe().unwrap();
        let me = me.file_name().unwrap().to_string_lossy().to_lowercase();
        assert!(running_exes().contains_key(&me));
    }

    #[test]
    fn index_marks_hiding_profiles() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles");
        let index = profile_index(&dir, &mut |_| {});
        let testrange = pick_profile(&index, "testrange.exe").unwrap();
        assert!(testrange.path.ends_with("testrange.toml"));
        assert!(testrange.hides);
        assert!(index.iter().any(|g| g.path.ends_with("testrange-hybrid.toml") && !g.hides));
    }
}
