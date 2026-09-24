use std::io::Write;
use std::path::{Component, Path, PathBuf, Prefix};
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use super::session_view::Target;
use super::{blocking, CmdResult};
use crate::app_state::AppState;
use crate::engine::error::{Error, IoContext, Result};
use crate::engine::transcript::source::Side;
use crate::engine::transcript::{first_difference, write_markdown, Hit};

/// Where each copy of a session parts from the other (item indexes), for the compare marker.
#[derive(Serialize)]
pub struct Divergence {
    local: Option<usize>,
    cloud: Option<usize>,
}

/// `system`: also match system events (only while the viewer shows them).
#[tauri::command]
pub async fn search_session(state: State<'_, Arc<AppState>>, key_hash: String, session_id: String, side: Side, query: String, system: bool) -> CmdResult<Vec<Hit>> {
    let state = Arc::clone(&state);
    blocking(move || {
        let target = Target::new(&state, key_hash, session_id, side)?;
        // The parse the viewer shows, so hit indexes match its items even if the file grew since.
        Ok(target.shown(&state)?.search(&query, system))
    })
    .await
}

/// Both copies are parsed (the cache holds two sessions); a missing copy compares as nothing.
#[tauri::command]
pub async fn compare_sides(state: State<'_, Arc<AppState>>, key_hash: String, session_id: String) -> CmdResult<Divergence> {
    let state = Arc::clone(&state);
    blocking(move || {
        let open = |side| Target::new(&state, key_hash.clone(), session_id.clone(), side)?.shown(&state);
        match (open(Side::Local), open(Side::Cloud)) {
            (Ok(local), Ok(cloud)) => {
                let (local, cloud) = first_difference(&local, &cloud);
                Ok(Divergence { local, cloud })
            }
            (Err(Error::SessionNotFound), _) | (_, Err(Error::SessionNotFound)) => Ok(Divergence { local: None, cloud: None }),
            (Err(e), _) | (_, Err(e)) => Err(e),
        }
    })
    .await
}

/// Asks where to save (a native dialog: the window never names a path) and writes the session as
/// Markdown there. `None` when the user cancels.
#[tauri::command]
pub async fn export_session(app: AppHandle, state: State<'_, Arc<AppState>>, key_hash: String, session_id: String, side: Side, title: String) -> CmdResult<Option<PathBuf>> {
    let state = Arc::clone(&state);
    blocking(move || {
        let name: String = title.chars().map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { ' ' } else { c }).take(80).collect();
        let name = if name.trim().is_empty() { "session".to_owned() } else { name.trim().to_owned() };
        let picked = app.dialog().file().set_file_name(format!("{name}.md")).add_filter("Markdown", &["md"]).blocking_save_file();
        let Some(path) = picked.and_then(|p| p.into_path().ok()) else { return Ok(None) };
        check_export_path(&state, &path)?;
        let target = Target::new(&state, key_hash, session_id, side)?;
        let transcript = target.shown(&state)?;
        // Written beside and renamed over: a hard link to another file is replaced, never written through.
        let tmp = path.with_extension("md.sr-tmp");
        let mut out = std::io::BufWriter::new(std::fs::File::create(&tmp).at(&tmp)?);
        let written = write_markdown(&transcript, &mut out, &mut |scope, id| target.open_agent(&state, scope, id).ok()).and_then(|()| out.flush());
        drop(out);
        if let Err(e) = written.and_then(|()| std::fs::rename(&tmp, &path)) {
            let _ = std::fs::remove_file(&tmp);
            return Err(e).at(&path);
        }
        Ok(Some(path))
    })
    .await
}

/// A Markdown file on a local disk, outside Claude's folder and the app's own data.
fn check_export_path(state: &AppState, path: &Path) -> Result<()> {
    let invalid = || Error::Invalid("not an export path".into());
    if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("md")) {
        return Err(invalid());
    }
    let parent = std::fs::canonicalize(path.parent().ok_or_else(invalid)?).at(path)?;
    // UNC paths (even `\\localhost\C$`) would slip past the prefix check below.
    if !matches!(parent.components().next(), Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::VerbatimDisk(_))) {
        return Err(invalid());
    }
    for protected in [state.settings().claude_home, state.app_dir.clone()] {
        if std::fs::canonicalize(&protected).is_ok_and(|p| parent.starts_with(p)) {
            return Err(invalid());
        }
    }
    Ok(())
}
