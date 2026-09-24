use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

pub const MINIMIZED_ARG: &str = "--minimized";

/// Keeps the Windows Run entry in line with the setting. Debug builds never register
/// themselves: that would start a dev binary at every logon.
pub fn apply(app: &AppHandle, enabled: bool) {
    if cfg!(debug_assertions) {
        return;
    }
    let manager = app.autolaunch();
    let result = match (enabled, manager.is_enabled().unwrap_or(false)) {
        (true, false) => manager.enable(),
        (false, true) => manager.disable(),
        _ => Ok(()),
    };
    if let Err(e) = result {
        log::warn!("autostart: {e}");
    }
}
