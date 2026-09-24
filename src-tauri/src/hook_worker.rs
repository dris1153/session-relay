//! `hook-worker`: the detached process `hook-save` starts. It waits for Claude to go quiet, then
//! saves the project push-only. Also the save the GUI runs for markers a worker left behind.

use std::path::Path;
use std::time::Duration;

use age::secrecy::ExposeSecret;

use crate::app_paths::app_dir;
use crate::app_state::build_engine;
use crate::engine::activity::Source;
use crate::engine::context::Engine;
use crate::engine::crypto::Keys;
use crate::engine::error::{Error, IoContext, Result};
use crate::engine::links::DirRole;
use crate::engine::settings::Settings;
use crate::engine::sync::{self, Request, SyncMode};
use crate::engine::{logging, overview, secrets};
use crate::pending;

const LOCK_WAIT: Duration = Duration::from_secs(60);
/// Turns closer than the debounce would postpone saving forever: save anyway after this long.
const MAX_WAIT: Duration = Duration::from_secs(600);
const BLOCKED_FILE: &str = "storage-blocked";

/// Written by the GUI when the storage repo became public, foreign or re-keyed. Workers run
/// in their own processes, so the in-memory "engine dropped" of the GUI does not reach them.
pub fn set_storage_blocked(app_dir: &Path, blocked: bool) {
    let path = app_dir.join(BLOCKED_FILE);
    let result = if blocked { std::fs::write(&path, b"").at(&path) } else { std::fs::remove_file(&path).or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e) }).at(&path) };
    if let Err(e) = result {
        log::warn!("storage block flag: {}", e.code());
    }
}

/// `hook-worker --dir <enc> --stamp <n> --delay <s>`.
pub fn worker(args: &[String]) -> i32 {
    logging::init(app_dir().join("logs"), "hook-worker");
    let value = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1));
    let (Some(enc), Some(stamp), Some(delay)) = (value("--dir"), value("--stamp").and_then(|s| s.parse().ok()), value("--delay").and_then(|s| s.parse().ok())) else {
        return 0;
    };
    if !pending::valid_enc(enc) {
        return 0;
    }
    std::thread::sleep(Duration::from_secs(delay));
    let app = app_dir();
    // A newer turn wrote a newer marker: its own worker saves after it, unless turns kept
    // coming for so long that this one must save now.
    let newer = pending::latest(&app, enc).is_some_and(|latest| latest > stamp);
    let overdue = pending::oldest(&app, enc).is_some_and(|oldest| pending::age(oldest) > MAX_WAIT);
    if pending::latest(&app, enc).is_none() || (newer && !overdue) {
        return 0;
    }
    if app.join(BLOCKED_FILE).exists() {
        return 0; // the marker stays until the storage is usable again
    }
    let result = engine_from_saved_identity(&app).and_then(|engine| save_pending(&engine, enc, stamp, LOCK_WAIT));
    if let Err(e) = result {
        log::warn!("hook-worker: {}", e.code());
    }
    0
}

fn engine_from_saved_identity(app: &Path) -> Result<Engine> {
    let settings = Settings::load(&app.join("settings.json"))?;
    let identity = secrets::load_identity()?.ok_or(Error::NotLoggedIn)?;
    build_engine(app, &settings, Keys::parse(identity.expose_secret())?)
}

/// Push-only save of one Claude project dir. The markers up to `stamp` are cleared unless the
/// failure may pass (busy, offline, signed out), so the save is retried later.
pub fn save_pending(engine: &Engine, enc: &str, stamp: u128, lock_wait: Duration) -> Result<()> {
    let result = match overview::classify_dir(engine, enc, lock_wait)? {
        DirRole::Primary(key) => sync::sync_project(engine, &Request { key: &key, mode: SyncMode::PushOnly, only: None, source: Source::Hook, lock_wait }).map(|_| ()),
        // Not a synced project (no GitHub remote, or a second checkout): nothing to save.
        DirRole::Secondary(_) | DirRole::NoRemote => Ok(()),
    };
    let retry = matches!(&result, Err(Error::Busy | Error::Network(_) | Error::GitTimeout { .. } | Error::LeaseRejected | Error::Auth(_) | Error::NotLoggedIn | Error::Io { .. } | Error::Secrets(_)));
    if !retry {
        pending::clear_up_to(&engine.cfg.app_dir, enc, stamp)?;
    }
    result
}
