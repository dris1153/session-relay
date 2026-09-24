use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use super::{blocking, CmdResult};
use crate::app_state::AppState;
use crate::dashboard;
use crate::engine::error::Error;
use crate::engine::transcript::source::{self, Side};
use crate::engine::transcript::{self, Item, SessionMeta};

#[derive(Serialize)]
pub struct SessionView {
    key_hash: String,
    session_id: String,
    side: Side,
    meta: SessionMeta,
    items: Vec<Item>,
}

/// The current branch of a session, from this machine's file or the cloud copy.
#[tauri::command]
pub async fn open_session(state: State<'_, Arc<AppState>>, key_hash: String, session_id: String, side: Side) -> CmdResult<SessionView> {
    let state = Arc::clone(&state);
    blocking(move || {
        let engine = state.engine().ok_or(Error::NotLoggedIn)?;
        let key = dashboard::project_key(&state, &key_hash)?;
        let cached = state.transcripts.find(&key_hash, &session_id, side);
        let transcript = match source::load(&engine, &key, &session_id, side, cached.as_ref().map(|(s, _)| s.as_str()))? {
            Some(loaded) => state.transcripts.insert(&key_hash, &session_id, side, loaded.signature, transcript::parse(&loaded.bytes)),
            None => cached.expect("a known signature comes from the cache").1,
        };
        Ok(SessionView { meta: transcript.meta.clone(), items: transcript.view(), key_hash, session_id, side })
    })
    .await
}

/// Full text behind a preview (`in:<tool id>` / `out:<tool id>`) of a session opened before.
#[tauri::command]
pub async fn session_detail(state: State<'_, Arc<AppState>>, key_hash: String, session_id: String, side: Side, reference: String) -> CmdResult<String> {
    let state = Arc::clone(&state);
    blocking(move || {
        state.engine().ok_or(Error::NotLoggedIn)?;
        state.transcripts.find(&key_hash, &session_id, side).and_then(|(_, t)| t.detail(&reference)).ok_or(Error::SessionNotFound)
    })
    .await
}
