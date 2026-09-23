pub mod app_paths;
pub mod app_state;
pub mod commands;
pub mod engine;
pub mod git_credential;
pub mod login;

use std::sync::Arc;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    engine::logging::init(app_paths::app_dir().join("logs"), "gui");
    let state = Arc::new(app_state::AppState::load());
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::setup::get_app_state,
            commands::setup::start_login,
            commands::setup::logout,
            commands::setup::check_storage,
            commands::setup::passphrase_strength,
            commands::setup::create_key,
            commands::setup::unlock_key,
            commands::settings::save_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
