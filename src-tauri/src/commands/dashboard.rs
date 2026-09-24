use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use super::{blocking, CmdResult};
use crate::app_state::AppState;
use crate::dashboard::{self, Dashboard, SaveAllItem};
use crate::engine::activity::{self, Activity};
use crate::engine::error::{Error, IoContext, Result};
use crate::engine::lock::SyncLock;
use crate::engine::overview;
use crate::engine::project_identity::locate;
use crate::engine::sync::{SyncMode, SyncReport};

/// `fetch`: pull the latest snapshot first (on open, after a save); without it only local
/// changes are re-read (window focus).
#[tauri::command]
pub async fn list_projects(state: State<'_, Arc<AppState>>, fetch: bool) -> CmdResult<Dashboard> {
    let state = Arc::clone(&state);
    blocking(move || dashboard::load(&state, fetch)).await
}

/// First step of opening the app: statuses against the snapshot already on disk, no network.
/// None before the first download: without the cloud side every project would look unsaved.
#[tauri::command]
pub async fn local_projects(state: State<'_, Arc<AppState>>) -> CmdResult<Option<Dashboard>> {
    let state = Arc::clone(&state);
    blocking(move || {
        let engine = state.engine().ok_or(Error::NotLoggedIn)?;
        if engine.repo.last_snapshot().is_none() {
            return Ok(None);
        }
        dashboard::load(&state, false).map(Some)
    })
    .await
}

#[tauri::command]
pub async fn project_activity(state: State<'_, Arc<AppState>>, key_hash: String) -> CmdResult<Vec<Activity>> {
    let state = Arc::clone(&state);
    blocking(move || {
        let engine = state.engine().ok_or(Error::NotLoggedIn)?;
        Ok(activity::recent(&engine.cfg.activity_file(), Some(&key_hash), 20))
    })
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiSyncMode {
    Auto,
    ForceLocal,
    ForceRemote,
}

/// Every button runs `auto`; `files` limits it to rows the user picked (restore one session).
#[tauri::command]
pub async fn sync_project(app: AppHandle, state: State<'_, Arc<AppState>>, key_hash: String, mode: UiSyncMode, files: Option<Vec<String>>) -> CmdResult<SyncReport> {
    let state = Arc::clone(&state);
    blocking(move || {
        let key = dashboard::project_key(&state, &key_hash)?;
        let mode = match mode {
            UiSyncMode::Auto => SyncMode::Auto,
            UiSyncMode::ForceLocal => SyncMode::ForceLocal,
            UiSyncMode::ForceRemote => SyncMode::ForceRemote,
        };
        let report = dashboard::run_sync(&state, &key, mode, files.as_deref());
        dashboard::publish(&app, &state, true);
        report
    })
    .await
}

#[tauri::command]
pub async fn save_all(app: AppHandle, state: State<'_, Arc<AppState>>) -> CmdResult<Vec<SaveAllItem>> {
    let state = Arc::clone(&state);
    blocking(move || {
        let items = dashboard::save_all(&state);
        dashboard::publish(&app, &state, true);
        items
    })
    .await
}

#[derive(Serialize)]
pub struct LinkResult {
    origin_matches: bool,
    /// The GitHub remote found at the picked folder, if any.
    found_remote: Option<String>,
}

/// Links a picked folder as this machine's checkout. A folder whose `origin` is another repo
/// is only linked with `force` (the user confirmed the warning).
#[tauri::command]
pub async fn link_project(state: State<'_, Arc<AppState>>, key_hash: String, local_root: PathBuf, force: bool) -> CmdResult<LinkResult> {
    let state = Arc::clone(&state);
    blocking(move || {
        if !super::settings::is_local_absolute(&local_root) || !local_root.is_dir() {
            return Err(Error::Invalid("pick an existing folder".into()));
        }
        let key = dashboard::project_key(&state, &key_hash)?;
        let located = locate(&local_root);
        let found_remote = located.as_ref().map(|l| l.key.remote.clone());
        let origin_matches = found_remote.as_deref() == Some(key.remote.as_str());
        if origin_matches || force {
            let engine = state.engine().ok_or(Error::NotLoggedIn)?;
            // A subfolder of the checkout still links the repo root.
            let root = located.filter(|_| origin_matches).map_or(local_root, |l| l.toplevel);
            overview::link_repo(&engine, &key.remote, &root, true)?;
        }
        Ok(LinkResult { origin_matches, found_remote })
    })
    .await
}

#[tauri::command]
pub async fn delete_remote_session(app: AppHandle, state: State<'_, Arc<AppState>>, key_hash: String, session_id: String) -> CmdResult<()> {
    let state = Arc::clone(&state);
    blocking(move || {
        let key = dashboard::project_key(&state, &key_hash)?;
        let engine = state.engine().ok_or(Error::NotLoggedIn)?;
        overview::delete_remote_session(&engine, &key, &session_id)?;
        dashboard::publish(&app, &state, true);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn open_project_folder(app: AppHandle, state: State<'_, Arc<AppState>>, key_hash: String) -> CmdResult<()> {
    let state = Arc::clone(&state);
    blocking(move || {
        let project = dashboard::project(&state, &key_hash)?;
        // The folder Claude ran in, like the `claude --resume` command the UI copies.
        let mut folder = project.local_root.ok_or(Error::NotLinked)?;
        project.subpath.split('/').filter(|p| !p.is_empty()).for_each(|p| folder.push(p));
        app.opener().open_path(folder.to_string_lossy(), None::<&str>).map_err(|e| Error::Invalid(e.to_string()))
    })
    .await
}

/// "Clear local data": the store clone, sync bases and backups. Links and Claude's own files stay.
#[tauri::command]
pub async fn clear_local_data(app: AppHandle, state: State<'_, Arc<AppState>>) -> CmdResult<()> {
    let state = Arc::clone(&state);
    blocking(move || {
        let engine = state.engine().ok_or(Error::NotLoggedIn)?;
        {
            let _lock = SyncLock::acquire_within(&engine.cfg.lock_file(), "clear", Duration::from_secs(30))?;
            for dir in [engine.cfg.store_dir(), engine.cfg.base_dir(), engine.cfg.backups_dir()] {
                remove_dir(&dir)?;
            }
            // Still under the lock: no load may use the cached view of the deleted clone.
            let mut cache = dashboard::cache(&state);
            let generation = cache.generation + 1;
            *cache = dashboard::DashboardCache { generation, ..Default::default() };
        }
        dashboard::publish(&app, &state, true);
        Ok(())
    })
    .await
}

fn remove_dir(dir: &std::path::Path) -> Result<()> {
    match std::fs::remove_dir_all(dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other.at(dir),
    }
}
