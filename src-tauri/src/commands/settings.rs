use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;
use tauri::{AppHandle, State};

use super::setup::{app_state_dto, AppStateDto};
use super::{blocking, CmdResult};
use crate::app_state::AppState;
use crate::engine::error::{Error, Result};

#[derive(Deserialize)]
pub struct SettingsPatch {
    machine_name: Option<String>,
    claude_home: Option<PathBuf>,
    workspace_roots: Option<Vec<PathBuf>>,
    language: Option<String>,
    autostart: Option<bool>,
}

#[tauri::command]
pub async fn save_settings(app: AppHandle, state: State<'_, Arc<AppState>>, patch: SettingsPatch) -> CmdResult<AppStateDto> {
    let state = Arc::clone(&state);
    blocking(move || {
        validate(&patch)?;
        let patch_autostart = patch.autostart;
        let before = state.settings();
        let after = state.update_settings(|s| {
            if let Some(v) = patch.machine_name.filter(|v| !v.trim().is_empty()) {
                s.machine_name = v.trim().to_string();
            }
            s.claude_home = patch.claude_home.unwrap_or(s.claude_home.clone());
            s.workspace_roots = patch.workspace_roots.unwrap_or(s.workspace_roots.clone());
            s.language = patch.language.or(s.language.clone());
            s.autostart = patch.autostart.unwrap_or(s.autostart);
        })?;
        if let Some(language) = after.language.as_deref().filter(|l| before.language.as_deref() != Some(*l)) {
            crate::tray::set_language(&app, language);
        }
        // Only on an explicit choice (end of onboarding, Settings), never at startup: a user who
        // turned the entry off in Task Manager must not find it back on.
        if let Some(enabled) = patch_autostart {
            crate::autostart::apply(&app, enabled);
        }
        // The engine captured both at unlock time.
        if (after.claude_home != before.claude_home || after.machine_name != before.machine_name) && state.engine().is_some() {
            state.restore_engine()?;
            let (app, state) = (app.clone(), Arc::clone(&state));
            std::thread::spawn(move || crate::dashboard::publish(&app, &state));
        }
        app_state_dto(&state)
    })
    .await
}

fn validate(patch: &SettingsPatch) -> Result<()> {
    let invalid = |what: &str| Err(Error::Invalid(format!("invalid {what}")));
    if patch.language.as_deref().is_some_and(|l| !matches!(l, "vi" | "en")) {
        return invalid("language");
    }
    if patch.claude_home.as_ref().is_some_and(|p| !is_local_absolute(p) || !p.is_dir()) {
        return invalid("Claude data folder");
    }
    if patch.workspace_roots.iter().flatten().any(|p| !is_local_absolute(p)) {
        return invalid("workspace folder");
    }
    Ok(())
}

/// Absolute and not a UNC path: probing `\\host\share` can leak NTLM credentials.
pub fn is_local_absolute(path: &std::path::Path) -> bool {
    path.is_absolute() && !path.to_string_lossy().starts_with(r"\\")
}
