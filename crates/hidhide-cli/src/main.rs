//! aikm-hidhide: hide DualSense from games through HidHide, undoing only our own changes.

use std::collections::HashSet;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, ExitCode, Stdio};
use std::sync::Mutex;
use std::thread::sleep;
use std::time::Duration;

use hidhide::{HidHide, Status};
use hidhide_cli::journal::Journal;
use hidhide_cli::procs::{self, Owner};
use hidhide_cli::{ops, pads, Result};
use windows::core::BOOL;
use windows::Win32::System::Console::SetConsoleCtrlHandler;

const USAGE: &str = "\
aikm-hidhide status
aikm-hidhide hide [--hold] [--allow <exe>]... [<instance path>...]
    без путей прячет все подключённые DualSense; --allow добавляет exe в whitelist;
    --hold держит скрытие, пока процесс жив: Ctrl+C или taskkill возвращают пад
aikm-hidhide unhide [<instance path>...]
    снимает наши записи blacklist (все или указанные) и возвращает Active
aikm-hidhide restore
    откатывает всё из журнала, включая whitelist";

/// Serialises the hold loop with the Ctrl+C handler, which runs on its own thread.
static LOCK: Mutex<()> = Mutex::new(());

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let res = match args.first().map(String::as_str) {
        Some("status") => status(),
        Some("hide") => hide(&args[1..]),
        Some("unhide") => recover().and_then(|_| unhide(&args[1..])),
        Some("restore") => restore(),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ошибка: {e}");
            ExitCode::FAILURE
        }
    }
}

fn recover() -> Result<()> {
    if let Some(j) = ops::recover_orphan()? {
        println!("журнал остался от упавшего процесса {}, откатил:", j.owner_pid);
        print_undone(&j);
    }
    Ok(())
}

fn status() -> Result<()> {
    match HidHide::probe()? {
        Status::NotInstalled => {
            println!("HidHide не установлен");
            return Ok(());
        }
        Status::Busy => {
            println!("HidHide занят другим процессом (закройте HidHide Configuration Client)");
            return Ok(());
        }
        Status::Installed { version, session_api } => println!(
            "HidHide {}, session API: {}",
            version.as_deref().unwrap_or("(версия неизвестна)"),
            if session_api { "есть" } else { "нет" }
        ),
    }
    let cfg = HidHide.config()?;
    let j = Journal::load()?;
    println!("Active: {}, inverse: {}", on_off(cfg.active), on_off(cfg.inverse));
    println!("blacklist ({}):", cfg.blacklist.len());
    for p in &cfg.blacklist {
        println!("  {}{p}", ours(&j.added_blacklist, p));
    }
    println!("whitelist ({}):", cfg.whitelist.len());
    for p in &cfg.whitelist {
        println!("  {}{p}", ours(&j.added_whitelist, p));
    }
    let present = pads::dualsense_instances();
    println!("DualSense подключено узлов: {}", present.len());
    for p in &present {
        let hidden = cfg.active && cfg.blacklist.contains(p);
        println!("  {} {p}", if hidden { "скрыт  " } else { "виден  " });
    }
    print!("журнал {}: ", Journal::path().display());
    if !j.dirty {
        println!("чистый");
    } else {
        let owner = match j.owner() {
            None => "без владельца".to_string(),
            Some(o) if procs::is_alive(o) => format!("владелец pid {} жив", o.pid),
            Some(o) => format!("владелец pid {} мёртв, откатится при следующем запуске", o.pid),
        };
        println!(
            "грязный, {owner}; prev_active {:?}, наших blacklist {}, whitelist {}",
            j.prev_active,
            j.added_blacklist.len(),
            j.added_whitelist.len()
        );
    }
    Ok(())
}

fn hide(args: &[String]) -> Result<()> {
    recover()?;
    let mut hold = false;
    let mut allow = Vec::new();
    let mut ids = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--hold" => hold = true,
            "--allow" => {
                let exe = it.next().ok_or("--allow без пути")?;
                allow.push(hidhide::dos_to_nt_path(Path::new(exe))?);
            }
            _ => ids.push(a.clone()),
        }
    }
    let explicit = !ids.is_empty();
    if !explicit {
        ids = pads::dualsense_instances();
        if ids.is_empty() && !hold {
            return Err("DualSense не найден; подключите пад или передайте instance path".into());
        }
    }

    let owner = hold.then(procs::current);
    let added = ops::hide(&ids, &allow, owner)?;
    for p in &ids {
        let mark = if added.blacklist.contains(p) { "добавлен" } else { "уже был в blacklist" };
        println!("blacklist: {p} ({mark})");
    }
    for p in &added.whitelist {
        println!("whitelist: {p} (добавлен)");
    }
    println!("Active: {} → вкл", on_off(added.was_active));

    let Some(me) = owner else { return Ok(()) };
    spawn_guard(me)?;
    unsafe { SetConsoleCtrlHandler(Some(on_ctrl), true) }?;
    println!("держу скрытие (pid {}); Ctrl+C снимает", me.pid);

    let mut seen: HashSet<String> = ids.into_iter().collect();
    loop {
        sleep(Duration::from_secs(1));
        let _g = LOCK.lock().unwrap();
        if !Journal::load()?.owned_by(me) {
            println!("журнал откатили снаружи, выхожу");
            return Ok(());
        }
        if explicit {
            continue;
        }
        let now = pads::dualsense_instances();
        let new: Vec<String> = now.iter().filter(|p| !seen.contains(*p)).cloned().collect();
        if !new.is_empty() {
            ops::hide(&new, &[], Some(me))?;
            println!("подключён новый узел, скрыт: {}", new.join(", "));
        }
        seen = now.into_iter().collect();
    }
}

unsafe extern "system" fn on_ctrl(_ctrl: u32) -> BOOL {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let code = match ops::restore() {
        Ok(j) => {
            println!("\nоткатил:");
            j.iter().for_each(print_undone);
            0
        }
        Err(e) => {
            eprintln!("\nошибка отката: {e}");
            1
        }
    };
    std::process::exit(code);
}

/// The guard is a separate exe so `taskkill /IM aikm-hidhide.exe` does not take it down too.
/// It must not stay in our job object, or a job with KILL_ON_JOB_CLOSE would kill it with us.
fn spawn_guard(me: Owner) -> Result<()> {
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let exe = std::env::current_exe()?.with_file_name("aikm-hidhide-guard.exe");
    let mut cmd = Command::new(&exe);
    cmd.args([me.pid.to_string(), me.start.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let flags = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
    let child = cmd
        .creation_flags(flags | CREATE_BREAKAWAY_FROM_JOB)
        .spawn()
        .or_else(|_| cmd.creation_flags(flags).spawn())
        .map_err(|e| format!("{}: {e}", exe.display()))?;
    println!("сторож запущен (pid {})", child.id());
    Ok(())
}

fn unhide(args: &[String]) -> Result<()> {
    let (removed, active) = ops::unhide(args)?;
    for a in args.iter().filter(|a| !removed.contains(a)) {
        println!("не наша запись, не трогаю: {a}");
    }
    for p in &removed {
        println!("blacklist: {p} (снят)");
    }
    match active {
        Some(a) => println!("Active возвращён в исходное состояние: {}", on_off(a)),
        None if removed.is_empty() => println!("ничего не менял"),
        None => {}
    }
    Ok(())
}

fn restore() -> Result<()> {
    match ops::restore()? {
        Some(j) => {
            if let Some(o) = j.owner().filter(|&o| procs::is_alive(o)) {
                println!("скрытие держал живой процесс {}, он завершится сам", o.pid);
            }
            print_undone(&j);
        }
        None => println!("журнал чистый, откатывать нечего"),
    }
    Ok(())
}

fn print_undone(j: &Journal) {
    for p in &j.added_blacklist {
        println!("  blacklist: снят {p}");
    }
    for p in &j.added_whitelist {
        println!("  whitelist: снят {p}");
    }
    if let Some(a) = j.prev_active {
        println!("  Active: {}", on_off(a));
    }
}

fn ours(list: &[String], p: &str) -> &'static str {
    if list.iter().any(|x| x == p) {
        "[наша] "
    } else {
        ""
    }
}

fn on_off(b: bool) -> &'static str {
    if b {
        "вкл"
    } else {
        "выкл"
    }
}
