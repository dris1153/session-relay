use std::sync::Arc;
use std::time::Duration;

use tauri::State;

use super::setup::{checked_repo, remember_key, unlockable_files};
use super::{blocking, CmdResult};
use crate::app_state::AppState;
use crate::engine::error::Error;
use crate::engine::lock::SyncLock;
use crate::engine::paths::CoreConfig;
use crate::engine::storage_check::StorageState;
use crate::engine::{key_reset, key_rewrap};

const LOCK_WAIT: Duration = Duration::from_secs(30);

/// Unlocks with the recovery key and sets a new passphrase; nothing is remembered here unless the
/// new passphrase was published.
#[tauri::command]
pub async fn recover_key(state: State<'_, Arc<AppState>>, recovery_key: String, passphrase: String) -> CmdResult<()> {
    let state = Arc::clone(&state);
    blocking(move || {
        let files = unlockable_files(checked_repo(&state)?)?;
        let store = state.store_for(&state.repo()?);
        let _lock = SyncLock::acquire_within(&state.app_dir.join("sync.lock"), "key-recovery", LOCK_WAIT)?;
        remember_key(&state, key_rewrap::recover_key(&store, &files, &recovery_key, &passphrase)?)
    })
    .await
}

/// New passphrase for the unlocked key; the identity and the data are unchanged.
#[tauri::command]
pub async fn change_passphrase(state: State<'_, Arc<AppState>>, passphrase: String) -> CmdResult<()> {
    let state = Arc::clone(&state);
    blocking(move || {
        let engine = state.engine().ok_or(Error::NotLoggedIn)?;
        let _lock = SyncLock::acquire_within(&engine.cfg.lock_file(), "key-change", LOCK_WAIT)?;
        key_rewrap::change_passphrase(&engine.repo, &engine.keys, &passphrase)
    })
    .await
}

/// A new recovery key (shown once); the previous one stops working.
#[tauri::command]
pub async fn new_recovery_key(state: State<'_, Arc<AppState>>) -> CmdResult<String> {
    let state = Arc::clone(&state);
    blocking(move || {
        let engine = state.engine().ok_or(Error::NotLoggedIn)?;
        let _lock = SyncLock::acquire_within(&engine.cfg.lock_file(), "key-change", LOCK_WAIT)?;
        Ok(key_rewrap::new_recovery_key(&engine.repo, &engine.keys)?.to_string())
    })
    .await
}

/// Destroys the cloud copy and starts a new store under a new key; returns its recovery key.
/// Only for a store this machine cannot unlock.
#[tauri::command]
pub async fn reset_store(state: State<'_, Arc<AppState>>, passphrase: String) -> CmdResult<String> {
    let state = Arc::clone(&state);
    blocking(move || {
        let store = state.store_for(&state.repo()?);
        let _lock = SyncLock::acquire_within(&state.app_dir.join("sync.lock"), "key-reset", LOCK_WAIT)?;
        // The head comes first: the key files read next are at least that new, and a push by anyone
        // after this point makes the reset fail instead of overwriting it.
        store.recover()?;
        let head = store.remote_head()?.ok_or_else(|| Error::Invalid("the storage repository is empty: nothing to start over".into()))?;
        let check = checked_repo(&state)?;
        if check.state != StorageState::NeedsUnlock {
            return Err(Error::Invalid("only a store that cannot be unlocked can be started over".into()));
        }
        let (keys, code) = key_reset::reset_store(&store, &head, &unlockable_files(check)?, &passphrase)?;
        // The cloud is replaced: whatever fails below must not lose the new recovery key.
        remember_key(&state, keys)?;
        crate::hook_worker::set_storage_blocked(&state.app_dir, false);
        let settings = state.settings();
        let cfg = CoreConfig { app_dir: state.app_dir.clone(), claude_home: settings.claude_home, machine_name: settings.machine_name };
        if let Err(e) = key_reset::clear_key_bound_state(&cfg) {
            log::warn!("clear key-bound state: {}", e.code());
        }
        Ok(code.to_string())
    })
    .await
}
