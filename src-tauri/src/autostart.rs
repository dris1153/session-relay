//! The "start with Windows" entry (HKCU `Run`). Written by us rather than the Tauri plugin: the
//! plugin leaves the exe path unquoted, and the install folder ("Session Relay") has a space.

use auto_launch::AutoLaunch;

pub const MINIMIZED_ARG: &str = "--minimized";
/// The value's name, kept from 0.1–0.2 when the product was called "session-relay": an existing
/// entry, and a choice made in Task Manager, carry over.
const ENTRY: &str = "session-relay";

fn entry() -> Option<AutoLaunch> {
    let exe = std::env::current_exe().ok()?;
    Some(AutoLaunch::new(ENTRY, &format!("\"{}\"", exe.display()), &[MINIMIZED_ARG]))
}

/// Keeps the entry in line with an explicit choice. Debug builds never register themselves: that
/// would start a dev binary at every logon.
pub fn apply(enabled: bool) {
    let Some(entry) = entry().filter(|_| !cfg!(debug_assertions)) else { return };
    let result = match (enabled, entry.is_enabled().unwrap_or(false)) {
        (true, false) => entry.enable(),
        (false, true) => entry.disable(),
        _ => Ok(()),
    };
    if let Err(e) = result {
        log::warn!("autostart: {e}");
    }
}

/// Whether Windows will start the app at logon: the entry exists and Task Manager did not turn it
/// off. Debug builds report the saved setting.
pub fn is_on(saved: bool) -> bool {
    if cfg!(debug_assertions) {
        return saved;
    }
    entry().and_then(|e| e.is_enabled().ok()).unwrap_or(false)
}

/// Points an enabled entry at this exe (after an update to another folder the old path is gone).
/// Never turns the entry on. Only `post-install` calls it: a dev or portable exe started once must
/// not take the entry over.
pub fn refresh() {
    let Some(entry) = entry().filter(|e| !cfg!(debug_assertions) && e.is_enabled().unwrap_or(false)) else { return };
    if let Err(e) = entry.enable() {
        log::warn!("autostart refresh: {e}");
    }
}
