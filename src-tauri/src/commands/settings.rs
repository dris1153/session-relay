use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;
use tauri::State;

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
pub async fn save_settings(state: State<'_, Arc<AppState>>, patch: SettingsPatch) -> CmdResult<AppStateDto> {
    let state = Arc::clone(&state);
    blocking(move || {
        validate(&patch)?;
        state.update_settings(|s| {
            if let Some(v) = patch.machine_name.filter(|v| !v.trim().is_empty()) {
                s.machine_name = v.trim().to_string();
            }
            s.claude_home = patch.claude_home.unwrap_or(s.claude_home.clone());
            s.workspace_roots = patch.workspace_roots.unwrap_or(s.workspace_roots.clone());
            s.language = patch.language.or(s.language.clone());
            s.autostart = patch.autostart.unwrap_or(s.autostart);
        })?;
        app_state_dto(&state)
    })
    .await
}

fn validate(patch: &SettingsPatch) -> Result<()> {
    let invalid = |what: &str| Err(Error::Invalid(format!("invalid {what}")));
    if patch.language.as_deref().is_some_and(|l| !matches!(l, "vi" | "en")) {
        return invalid("language");
    }
    if patch.claude_home.as_ref().is_some_and(|p| !p.is_absolute() || !p.is_dir()) {
        return invalid("Claude data folder");
    }
    if patch.workspace_roots.iter().flatten().any(|p| !p.is_absolute()) {
        return invalid("workspace folder");
    }
    Ok(())
}
