#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent;
mod state;

fn main() {
    tauri::Builder::default()
        .manage(agent::Agent::default())
        .setup(|app| {
            state::spawn_stub(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![agent::send_chat, agent::reset_chat])
        .run(tauri::generate_context!())
        .expect("не удалось запустить окно");
}
