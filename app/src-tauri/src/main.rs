#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent;
mod edit;
mod shell;
mod state;

use tauri::{Manager, RunEvent, WindowEvent};

fn main() {
    tauri::Builder::default()
        // Второй запуск (ярлык, автозапуск) не поднимает второй маппер — показывает уже открытое окно.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| shell::show(app)))
        .manage(agent::Agent::load())
        .manage(state::Mapper::default())
        .setup(|app| {
            app.state::<state::Mapper>().start(app.handle().clone());
            shell::tray(app.handle())?;
            if std::env::args().any(|a| a == shell::TRAY_FLAG) {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
            Ok(())
        })
        // Крестик прячет окно в трей: маппер работает дальше, выход — из меню трея.
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            agent::send_chat,
            agent::reset_chat,
            agent::chat_history,
            shell::autostart,
            shell::set_autostart,
            state::bindings,
            state::profiles,
            state::profile_bindings,
            state::set_binding,
            state::reset_binding,
            state::set_prompts,
            state::set_paused,
            state::calibrate,
            state::test_turn,
            state::set_real_world,
            state::engine_log
        ])
        .build(tauri::generate_context!())
        .expect("не удалось запустить окно")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                app.state::<state::Mapper>().stop();
            }
        });
}
