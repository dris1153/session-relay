use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use super::{blocking, CmdResult};
use crate::app_state::AppState;
use crate::dashboard;
use crate::engine::error::Error;
use crate::engine::transcript::source::{self, Side};
use crate::engine::transcript::{self, cut, Detail, Item, Resolved, SessionMeta, DETAIL_MAX};

#[derive(Serialize)]
pub struct SessionView {
    key_hash: String,
    session_id: String,
    side: Side,
    meta: SessionMeta,
    items: Vec<Item>,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DetailView {
    Text { text: String, truncated: bool },
    /// A subagent's conversation.
    Session { meta: Box<SessionMeta>, items: Vec<Item> },
    Image { data_uri: String },
    /// A saved output that is no longer on this machine or in the cloud (Claude cleaned it up).
    Gone,
}

/// The current branch of a session, from this machine's file or the cloud copy.
#[tauri::command]
pub async fn open_session(state: State<'_, Arc<AppState>>, key_hash: String, session_id: String, side: Side) -> CmdResult<SessionView> {
    let state = Arc::clone(&state);
    blocking(move || {
        let engine = state.engine().ok_or(Error::NotLoggedIn)?;
        let key = dashboard::project_key(&state, &key_hash)?;
        let cached = state.transcripts.find(&key_hash, &session_id, side, "");
        let transcript = match source::load(&engine, &key, &session_id, side, cached.as_ref().map(|(s, _)| s.as_str()))? {
            Some(loaded) => state.transcripts.insert(&key_hash, &session_id, side, "", loaded.signature, transcript::parse(&loaded.bytes)),
            None => cached.expect("a known signature comes from the cache").1,
        };
        Ok(SessionView { meta: transcript.meta.clone(), items: transcript.view(), key_hash, session_id, side })
    })
    .await
}

/// What a preview or block of a session opened before points at (`agent:<id>/out:<tool id>`).
#[tauri::command]
pub async fn session_detail(state: State<'_, Arc<AppState>>, key_hash: String, session_id: String, side: Side, reference: String) -> CmdResult<DetailView> {
    let state = Arc::clone(&state);
    blocking(move || {
        let engine = state.engine().ok_or(Error::NotLoggedIn)?;
        let key = dashboard::project_key(&state, &key_hash)?;
        let (_, root) = state.transcripts.find(&key_hash, &session_id, side, "").ok_or(Error::SessionNotFound)?;
        // Subagents are reread when their file changed: async agents keep writing after the call.
        let resolved = transcript::resolve(root, &reference, |scope, id| {
            let cached = state.transcripts.find(&key_hash, &session_id, side, scope);
            match source::load_rel(&engine, &key, &session_id, side, &format!("subagents/agent-{id}.jsonl"), cached.as_ref().map(|(s, _)| s.as_str()))? {
                Some(loaded) => Ok(state.transcripts.insert(&key_hash, &session_id, side, scope, loaded.signature, transcript::parse_agent(&loaded.bytes))),
                None => Ok(cached.expect("a known signature comes from the cache").1),
            }
        })?;
        Ok(match resolved {
            Resolved::Agent(t) => DetailView::Session { meta: Box::new(t.meta.clone()), items: t.view() },
            Resolved::Detail(Detail::Text(text)) => text_view(&text),
            Resolved::Detail(Detail::File(rel)) => match source::load_rel(&engine, &key, &session_id, side, &rel, None) {
                Ok(loaded) => text_view(&String::from_utf8_lossy(&loaded.map(|l| l.bytes).unwrap_or_default())),
                Err(Error::SessionNotFound) => DetailView::Gone,
                Err(e) => return Err(e),
            },
            Resolved::Detail(Detail::Image(img)) => DetailView::Image { data_uri: format!("data:{};base64,{}", img.media_type, img.data) },
            Resolved::Detail(Detail::Agent(_)) => unreachable!("resolve opens agents itself"),
        })
    })
    .await
}

fn text_view(text: &str) -> DetailView {
    let (text, truncated) = cut(text, DETAIL_MAX);
    DetailView::Text { text, truncated }
}
