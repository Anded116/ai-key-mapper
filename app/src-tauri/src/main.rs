#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent;
mod state;

use tauri::{Manager, RunEvent};

fn main() {
    tauri::Builder::default()
        .manage(agent::Agent::default())
        .manage(state::Mapper::default())
        .setup(|app| {
            app.state::<state::Mapper>().start(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            agent::send_chat,
            agent::reset_chat,
            state::set_paused,
            state::calibrate,
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
