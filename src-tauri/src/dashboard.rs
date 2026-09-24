use std::path::PathBuf;
use std::sync::{MutexGuard, PoisonError};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::app_state::AppState;
use crate::engine::activity::Source;
use crate::engine::error::{Error, Result};
use crate::engine::evaluate::ProjectStatus;
use crate::engine::links::Links;
use crate::engine::overview::{self, FileRow};
use crate::engine::project_identity::ProjectKey;
use crate::engine::sync::{self, Request, SyncMode, SyncReport};

/// How long a refresh waits for a running save before showing the cached picture.
const REFRESH_WAIT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Serialize)]
pub struct ProjectView {
    pub key_hash: String,
    pub remote: String,
    pub owner: String,
    pub name: String,
    pub subpath: String,
    pub status: ProjectStatus,
    /// Linked checkout on this machine.
    pub local_root: Option<PathBuf>,
    pub files: Vec<FileRow>,
    pub unreadable: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Dashboard {
    pub projects: Vec<ProjectView>,
    pub unmanaged: usize,
    pub secondary: usize,
    pub errors: Vec<(String, String)>,
    /// The last fetch failed on the network: statuses are against the last snapshot seen.
    pub offline: bool,
    pub fetched_at: Option<DateTime<Utc>>,
}

impl Dashboard {
    /// Something in the cloud is newer than here: the tray shows it.
    pub fn needs_attention(&self) -> bool {
        self.projects.iter().any(|p| matches!(p.status, ProjectStatus::RemoteAhead | ProjectStatus::Both | ProjectStatus::Diverged))
    }

    pub fn key(&self, key_hash: &str) -> Result<&ProjectView> {
        self.projects.iter().find(|p| p.key_hash == key_hash).ok_or_else(|| Error::Invalid("unknown project".into()))
    }
}

/// Last computed view. `generation` moves when the engine is replaced, so a load that ran on
/// the old engine does not overwrite the fresh state.
#[derive(Default)]
pub struct DashboardCache {
    pub view: Option<Dashboard>,
    pub generation: u64,
}

/// A panic elsewhere must not take the dashboard down with it.
pub fn cache(state: &AppState) -> MutexGuard<'_, DashboardCache> {
    state.dashboard.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Recomputes project statuses against the newest known snapshot; `fetch` first pulls the
/// latest one. A running save (`Busy`) or a network failure keeps the last known picture.
/// The cache lock is never held across git: a slow first download must not block other commands.
pub fn load(state: &AppState, fetch: bool) -> Result<Dashboard> {
    let engine = state.engine().ok_or(Error::NotLoggedIn)?;
    let (cached, generation) = {
        let cache = cache(state);
        (cache.view.clone(), cache.generation)
    };
    let mut view = cached.clone().unwrap_or_default();
    if fetch {
        match overview::refresh(&engine, REFRESH_WAIT) {
            Ok(_) => {
                view.offline = false;
                view.fetched_at = Some(Utc::now());
            }
            Err(Error::Network(_) | Error::GitTimeout { .. }) => view.offline = true,
            Err(Error::Busy) if cached.is_some() => return Ok(view),
            Err(e) => return Err(e),
        }
    }
    let overview = match overview::list(&engine, engine.repo.last_snapshot().as_deref()) {
        Err(Error::Busy) if cached.is_some() => return Ok(view),
        other => other?,
    };
    let links = Links::load(&engine.cfg.links_file())?;
    view.projects = overview.projects.into_iter().map(|p| project_view(p, &links)).collect();
    view.unmanaged = overview.unmanaged_dirs.len();
    view.secondary = overview.secondary_dirs.len();
    view.errors = overview.errors;
    let mut cache = cache(state);
    if cache.generation == generation {
        cache.view = Some(view.clone());
    }
    drop(cache);
    if let Some(app) = state.app.get() {
        crate::tray::set_attention(app, view.needs_attention());
    }
    Ok(view)
}

fn project_view(p: overview::ProjectSummary, links: &Links) -> ProjectView {
    let mut parts = p.key.remote.splitn(3, '/').skip(1);
    let (owner, name) = (parts.next().unwrap_or_default().to_string(), parts.next().unwrap_or_default().to_string());
    ProjectView { local_root: links.repos.get(&p.key.remote).cloned(), key_hash: p.key_hash, remote: p.key.remote, owner, name, subpath: p.key.subpath, status: p.status, files: p.files, unreadable: p.unreadable }
}

/// Refreshes and tells the window. Failures only log: this runs after the real work.
pub fn publish(app: &AppHandle, state: &AppState) {
    match load(state, true) {
        Ok(view) => {
            let _ = app.emit("projects-changed", view);
        }
        Err(e) => log::warn!("dashboard refresh: {}", e.code()),
    }
}

/// The watcher lost the network: show it now instead of at the next refresh.
pub fn mark_offline(app: &AppHandle, state: &AppState) {
    let view = match cache(state).view.as_mut() {
        Some(view) if !view.offline => {
            view.offline = true;
            view.clone()
        }
        _ => return,
    };
    let _ = app.emit("projects-changed", view);
}

pub fn project(state: &AppState, key_hash: &str) -> Result<ProjectView> {
    let cache = cache(state);
    let view = cache.view.as_ref().ok_or_else(|| Error::Invalid("projects not loaded".into()))?;
    view.key(key_hash).cloned()
}

pub fn project_key(state: &AppState, key_hash: &str) -> Result<ProjectKey> {
    let p = project(state, key_hash)?;
    Ok(ProjectKey { remote: p.remote, subpath: p.subpath })
}

pub fn run_sync(state: &AppState, key: &ProjectKey, mode: SyncMode, only: Option<&[String]>) -> Result<SyncReport> {
    let engine = state.engine().ok_or(Error::NotLoggedIn)?;
    sync::sync_project(&engine, &Request { key, mode, only, source: Source::Gui, lock_wait: Duration::from_secs(30) })
}

#[derive(Debug, Clone, Serialize)]
pub struct SaveAllItem {
    pub key_hash: String,
    pub report: Option<SyncReport>,
    pub error: Option<String>,
}

/// "Save all": push-only over every project with local changes; one failure does not stop the rest.
pub fn save_all(state: &AppState) -> Result<Vec<SaveAllItem>> {
    let view = load(state, false)?;
    let pending = view.projects.iter().filter(|p| matches!(p.status, ProjectStatus::LocalAhead | ProjectStatus::Both));
    Ok(pending
        .map(|p| {
            let key = ProjectKey { remote: p.remote.clone(), subpath: p.subpath.clone() };
            match run_sync(state, &key, SyncMode::PushOnly, None) {
                Ok(report) => SaveAllItem { key_hash: p.key_hash.clone(), report: Some(report), error: None },
                Err(e) => SaveAllItem { key_hash: p.key_hash.clone(), report: None, error: Some(e.code().into()) },
            }
        })
        .collect())
}
