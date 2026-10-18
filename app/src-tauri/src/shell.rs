//! Жизнь в фоне: иконка в трее, закрытие окна прячет его в трей (маппер продолжает работать),
//! автозапуск вместе с Windows через ключ `Run` в реестре.

use std::process::Command as Process;

use engine::runner::Command;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::state::Mapper;

/// Флаг командной строки: стартовать свёрнутым в трей (так запускает автозапуск).
pub const TRAY_FLAG: &str = "--tray";

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_NAME: &str = "ai-key-mapper";

/// Показать главное окно поверх остальных.
pub fn show(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

pub fn tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Открыть", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "Пауза / продолжить", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Выход — отпустить геймпад", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &pause, &PredefinedMenuItem::separator(app)?, &quit])?;
    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("ai-key-mapper")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show(app),
            "pause" => app.state::<Mapper>().send(Command::TogglePause),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// Собранное приложение, а не dev-сборка: той нужен запущенный vite, из автозапуска она не поднимется.
fn release_exe() -> Result<std::path::PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dev = exe.components().any(|c| c.as_os_str().eq_ignore_ascii_case("debug"));
    if dev {
        return Err("автозапуск — только для собранного приложения: запусти его через App.cmd".into());
    }
    Ok(exe)
}

fn reg(args: &[&str]) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = Process::new("reg").args(args).creation_flags(CREATE_NO_WINDOW).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[derive(serde::Serialize)]
pub struct Autostart {
    pub enabled: bool,
    /// Почему переключатель недоступен (dev-сборка); `None` — доступен.
    pub unavailable: Option<String>,
}

#[tauri::command]
pub fn autostart() -> Autostart {
    let unavailable = release_exe().err();
    let enabled = reg(&["query", RUN_KEY, "/v", RUN_NAME]).is_ok();
    Autostart { enabled, unavailable }
}

/// Включить или выключить запуск вместе с Windows (свёрнутым в трей).
#[tauri::command]
pub fn set_autostart(enabled: bool) -> Result<Autostart, String> {
    if enabled {
        let exe = release_exe()?;
        let command = format!("\"{}\" {TRAY_FLAG}", exe.display());
        reg(&["add", RUN_KEY, "/v", RUN_NAME, "/t", "REG_SZ", "/d", &command, "/f"])?;
    } else if reg(&["query", RUN_KEY, "/v", RUN_NAME]).is_ok() {
        reg(&["delete", RUN_KEY, "/v", RUN_NAME, "/f"])?;
    }
    Ok(autostart())
}
