use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};

use crate::app_state::AppState;
use crate::dashboard;
use crate::engine::error::Error;
use crate::engine::maintenance;
use crate::engine::storage_check::{self, StorageState};

const EVERY: Duration = Duration::from_secs(60);
const OFFLINE_EVERY: Duration = Duration::from_secs(300);
const STORAGE_RECHECK: Duration = Duration::from_secs(24 * 3600);
/// Weekly gc waits until the app has been up this long, so it never competes with opening.
const GC_AFTER_UPTIME: Duration = Duration::from_secs(120);

/// Notices saves from other machines: a cheap `ls-remote` each minute (no lock, no worktree),
/// and a full refresh only when the remote head moved. Also re-checks once a day that the
/// storage repo is still private and ours.
pub fn spawn(app: AppHandle, state: Arc<AppState>) {
    std::thread::spawn(move || {
        let started = Instant::now();
        let mut storage_checked = Instant::now();
        let mut auth_reported = false;
        loop {
            let offline = dashboard::cache(&state).view.as_ref().is_some_and(|v| v.offline);
            std::thread::sleep(if offline { OFFLINE_EVERY } else { EVERY });
            let Some(engine) = state.engine() else { continue };
            if !engine.repo.dir().join(".git").exists() {
                continue; // the dashboard has not fetched yet
            }
            match engine.repo.remote_head() {
                Ok(head) => {
                    auth_reported = false;
                    // Also when another process (a hook save) moved the clone past what is shown.
                    let known = engine.repo.last_snapshot();
                    let shown = dashboard::cache(&state).sha.clone();
                    if head != known || shown != known || offline {
                        dashboard::publish(&app, &state);
                    }
                }
                Err(Error::Network(_) | Error::GitTimeout { .. }) => dashboard::mark_offline(&app, &state),
                // Signed out or access revoked: onboarding takes over, once rather than every minute.
                Err(Error::Auth(_) | Error::NotLoggedIn) => {
                    if !std::mem::replace(&mut auth_reported, true) {
                        let _ = app.emit("storage-changed", ());
                    }
                }
                Err(e) => log::warn!("remote watch: {}", e.code()),
            }
            if started.elapsed() > GC_AFTER_UPTIME {
                match maintenance::gc_if_due(&engine) {
                    Ok(()) | Err(Error::Busy) => {}
                    Err(e) => log::warn!("weekly gc: {}", e.code()),
                }
            }
            if storage_checked.elapsed() > STORAGE_RECHECK {
                storage_checked = Instant::now();
                recheck_storage(&app, &state);
            }
        }
    });
}

/// Public, deleted or re-keyed storage stops syncing until onboarding passes again.
fn recheck_storage(app: &AppHandle, state: &AppState) {
    match storage_check::check(&state.app_dir, &state.settings()) {
        Ok(check) if check.state != StorageState::Ready => {
            state.drop_engine();
            let _ = app.emit("storage-changed", ());
        }
        Ok(_) | Err(Error::Network(_)) => {}
        Err(e) => log::warn!("storage re-check: {}", e.code()),
    }
}
