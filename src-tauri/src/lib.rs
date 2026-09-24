pub mod app_paths;
pub mod app_state;
mod auto_save;
mod autostart;
pub mod commands;
mod dashboard;
pub mod engine;
pub mod git_credential;
pub mod hook;
pub mod hook_worker;
pub mod login;
pub mod pending;
mod tray;
mod transcript_cache;
mod watcher;

use std::sync::Arc;

use tauri::WindowEvent;
use tauri_plugin_autostart::MacosLauncher;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    engine::logging::init(app_paths::app_dir().join("logs"), "gui");
    let state = Arc::new(app_state::AppState::load());
    tauri::Builder::default()
        // Must be first: a second launch only focuses the running window.
        .plugin(tauri_plugin_single_instance::init(|app, _, _| tray::show_main(app)))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec![autostart::MINIMIZED_ARG])))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::clone(&state))
        .setup(move |app| {
            let handle = app.handle().clone();
            let _ = state.app.set(handle.clone());
            tray::build(&handle)?;
            if !std::env::args().any(|a| a == autostart::MINIMIZED_ARG) {
                tray::show_main(&handle);
            }
            restore(&state);
            auto_save::repair_stale_path(&state);
            watcher::spawn(handle, Arc::clone(&state));
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing keeps the app in the tray; "Quit" in the tray menu exits.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::setup::get_app_state,
            commands::setup::start_login,
            commands::setup::logout,
            commands::setup::check_storage,
            commands::setup::passphrase_strength,
            commands::setup::create_key,
            commands::setup::unlock_key,
            commands::settings::save_settings,
            commands::settings::set_auto_save,
            commands::dashboard::list_projects,
            commands::dashboard::local_projects,
            commands::dashboard::project_activity,
            commands::dashboard::sync_project,
            commands::dashboard::save_all,
            commands::dashboard::link_project,
            commands::dashboard::delete_remote_session,
            commands::dashboard::open_project_folder,
            commands::dashboard::clear_local_data,
            commands::workspace::scan_workspaces,
            commands::workspace::clone_project,
            commands::workspace::cancel_clone,
            commands::session_view::open_session,
            commands::session_view::session_detail,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Unlocks from the cached key (no network needed), then cleans up after crashes in the background.
fn restore(state: &Arc<app_state::AppState>) {
    match state.restore_engine() {
        Ok(true) => {
            let state = Arc::clone(state);
            std::thread::spawn(move || {
                if let Some(engine) = state.engine() {
                    if let Err(e) = engine::maintenance::on_startup(&engine) {
                        log::warn!("startup maintenance: {}", e.code());
                    }
                }
            });
        }
        Ok(false) => {}
        Err(e) => log::warn!("restore engine: {}", e.code()),
    }
}
