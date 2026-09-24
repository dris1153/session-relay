//! The GUI side of auto-save: the on/off switch (Claude's own `settings.json` is the source of
//! truth) and finishing saves whose hook worker died.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::app_state::AppState;
use crate::engine::claude_hook_config::{self, HookStatus};
use crate::engine::context::Engine;
use crate::engine::error::{Error, Result};
use crate::{hook_worker, pending};

/// Longer than a live worker can take (2 min wait, lock waits, push): older markers are orphans.
const STALE_AFTER: Duration = Duration::from_secs(15 * 60);
const GUI_LOCK_WAIT: Duration = Duration::from_secs(10);
const MAX_BACKOFF: Duration = Duration::from_secs(30 * 60);

/// Failed retries back off (offline, signed out): no fetch and activity line every minute.
static BACKOFF: Mutex<Option<HashMap<String, (Instant, Duration)>>> = Mutex::new(None);

fn claude_settings(home: &Path) -> PathBuf {
    home.join("settings.json")
}

fn exe() -> Result<PathBuf> {
    std::env::current_exe().map_err(|e| Error::Invalid(e.to_string()))
}

pub fn status(state: &AppState) -> HookStatus {
    exe().map_or(HookStatus::NotInstalled, |exe| claude_hook_config::status(&claude_settings(&state.settings().claude_home), &exe))
}

pub fn set(state: &AppState, enabled: bool) -> Result<()> {
    let settings = claude_settings(&state.settings().claude_home);
    if enabled {
        claude_hook_config::install(&settings, &exe()?)
    } else {
        claude_hook_config::uninstall(&settings)
    }
}

/// The Claude folder changed: the hooks follow it, or they would sit in a profile nobody uses.
pub fn move_home(old_home: &Path, new_home: &Path) -> Result<()> {
    let old = claude_settings(old_home);
    if claude_hook_config::registered(&old).is_empty() {
        return Ok(());
    }
    claude_hook_config::install(&claude_settings(new_home), &exe()?)?;
    claude_hook_config::uninstall(&old)
}

/// Hooks point at an exe that no longer exists (the app moved): point them here. Debug builds
/// never do this, or a dev build and the installed app would keep rewriting Claude's settings.
pub fn repair_stale_path(state: &AppState) {
    if cfg!(debug_assertions) {
        return;
    }
    if let Err(e) = exe().and_then(|exe| claude_hook_config::repair(&claude_settings(&state.settings().claude_home), &exe)) {
        log::warn!("repair auto-save hooks: {}", e.code());
    }
}

/// Saves projects whose newest marker is older than `age`; true when any was saved.
pub fn finish_pending(engine: &Engine, age: Duration, deadline: Instant, backoff: bool) -> bool {
    let mut saved = false;
    for (enc, stamp) in pending::stale(&engine.cfg.app_dir, age) {
        let mut waits = BACKOFF.lock().unwrap_or_else(PoisonError::into_inner);
        let waits = waits.get_or_insert_with(HashMap::new);
        if Instant::now() > deadline || (backoff && waits.get(&enc).is_some_and(|(next, _)| Instant::now() < *next)) {
            continue;
        }
        match hook_worker::save_pending(engine, &enc, stamp, GUI_LOCK_WAIT) {
            Ok(()) => {
                waits.remove(&enc);
                saved = true;
            }
            Err(e) => {
                log::warn!("pending save: {}", e.code());
                let gap = waits.get(&enc).map_or(Duration::from_secs(60), |(_, gap)| (*gap * 2).min(MAX_BACKOFF));
                waits.insert(enc, (Instant::now() + gap, gap));
            }
        }
    }
    saved
}

/// Watcher tick: saves the hook workers left behind.
pub fn retry_stale(engine: &Engine) -> bool {
    finish_pending(engine, STALE_AFTER, Instant::now() + Duration::from_secs(120), true)
}
