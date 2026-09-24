use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use super::{blocking, CmdResult};
use crate::app_state::AppState;
use crate::dashboard;
use crate::engine::error::Error;
use crate::engine::evaluate::ProjectStatus;
use crate::engine::git_clone;
use crate::engine::overview;
use crate::engine::progress::Progress;
use crate::engine::project_identity::normalize_remote;
use crate::engine::workspace_scan::{self, Checkout};

/// Checkouts under the workspace folders from Settings (a new machine links them in one click).
#[tauri::command]
pub async fn scan_workspaces(state: State<'_, Arc<AppState>>) -> CmdResult<Vec<Checkout>> {
    let state = Arc::clone(&state);
    blocking(move || Ok(workspace_scan::scan(&state.settings().workspace_roots))).await
}

/// Clones a cloud project that has no checkout here into `<root>\<repo>`, then links it.
/// `root` must be one of the workspace folders from Settings.
#[tauri::command]
pub async fn clone_project(app: AppHandle, state: State<'_, Arc<AppState>>, key_hash: String, root: PathBuf) -> CmdResult<PathBuf> {
    let state = Arc::clone(&state);
    blocking(move || {
        if !state.settings().workspace_roots.contains(&root) {
            return Err(Error::Invalid("not a workspace folder".into()));
        }
        let project = dashboard::project(&state, &key_hash)?;
        if project.status != ProjectStatus::NotLinked {
            return Err(Error::Invalid("the project already has a checkout here".into()));
        }
        // The remote comes from a manifest: its MAC proves who wrote it, not that it is well formed.
        let single = matches!(Path::new(&project.name).components().collect::<Vec<_>>()[..], [Component::Normal(_)]);
        if !single || normalize_remote(&format!("https://{}", project.remote)).as_deref() != Some(project.remote.as_str()) {
            return Err(Error::Invalid("unexpected remote".into()));
        }
        let dest = root.join(&project.name);
        git_clone::clone(&project.remote, &dest, &mut |percent| {
            let _ = app.emit("sync-progress", Progress::Clone { percent });
        })?;
        let engine = state.engine().ok_or(Error::NotLoggedIn)?;
        overview::link_repo(&engine, &project.remote, &dest, false)?;
        Ok(dest)
    })
    .await
}

#[tauri::command]
pub async fn cancel_clone() -> CmdResult<()> {
    blocking(|| {
        git_clone::cancel();
        Ok(())
    })
    .await
}
