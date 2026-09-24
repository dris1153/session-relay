use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use super::{blocking, CmdResult};
use crate::app_state::AppState;
use crate::dashboard;
use crate::engine::context::Engine;
use crate::engine::error::{Error, Result};
use crate::engine::project_identity::ProjectKey;
use crate::engine::transcript::source::{self, Side};
use crate::engine::transcript::{self, cut, Detail, Item, Resolved, SessionMeta, Transcript, DETAIL_MAX};

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

/// One session of one project on one side, as the viewer commands address it.
pub(super) struct Target {
    pub engine: Arc<Engine>,
    pub key: ProjectKey,
    pub key_hash: String,
    pub session_id: String,
    pub side: Side,
}

impl Target {
    pub fn new(state: &AppState, key_hash: String, session_id: String, side: Side) -> Result<Self> {
        let engine = state.engine().ok_or(Error::NotLoggedIn)?;
        let key = dashboard::project_key(state, &key_hash)?;
        Ok(Self { engine, key, key_hash, session_id, side })
    }

    /// The session's parse, reread only when its file changed.
    pub fn open(&self, state: &AppState) -> Result<Arc<Transcript>> {
        let cached = state.transcripts.find(&self.key_hash, &self.session_id, self.side, "");
        match source::load(&self.engine, &self.key, &self.session_id, self.side, cached.as_ref().map(|(s, _)| s.as_str()))? {
            Some(loaded) => Ok(state.transcripts.insert(&self.key_hash, &self.session_id, self.side, "", loaded.signature, transcript::parse(&loaded.bytes))),
            None => Ok(cached.expect("a known signature comes from the cache").1),
        }
    }

    /// The parse the viewer shows (details and search must use the same item indexes); read
    /// again only when it is no longer cached.
    pub fn shown(&self, state: &AppState) -> Result<Arc<Transcript>> {
        match state.transcripts.find(&self.key_hash, &self.session_id, self.side, "") {
            Some((_, t)) => Ok(t),
            None => self.open(state),
        }
    }

    /// A subagent's parse; async agents keep writing after the call, so it is reread on change.
    pub fn open_agent(&self, state: &AppState, scope: &str, id: &str) -> Result<Arc<Transcript>> {
        let cached = state.transcripts.find(&self.key_hash, &self.session_id, self.side, scope);
        match source::load_rel(&self.engine, &self.key, &self.session_id, self.side, &format!("subagents/agent-{id}.jsonl"), cached.as_ref().map(|(s, _)| s.as_str()))? {
            Some(loaded) => Ok(state.transcripts.insert(&self.key_hash, &self.session_id, self.side, scope, loaded.signature, transcript::parse_agent(&loaded.bytes))),
            None => Ok(cached.expect("a known signature comes from the cache").1),
        }
    }
}

/// The current branch of a session, from this machine's file or the cloud copy.
#[tauri::command]
pub async fn open_session(state: State<'_, Arc<AppState>>, key_hash: String, session_id: String, side: Side) -> CmdResult<SessionView> {
    let state = Arc::clone(&state);
    blocking(move || {
        let target = Target::new(&state, key_hash, session_id, side)?;
        let transcript = target.open(&state)?;
        Ok(SessionView { meta: transcript.meta.clone(), items: transcript.view(), key_hash: target.key_hash, session_id: target.session_id, side })
    })
    .await
}

/// What a preview or block of a session opened before points at (`agent:<id>/out:<tool id>`).
#[tauri::command]
pub async fn session_detail(state: State<'_, Arc<AppState>>, key_hash: String, session_id: String, side: Side, reference: String) -> CmdResult<DetailView> {
    let state = Arc::clone(&state);
    blocking(move || {
        let target = Target::new(&state, key_hash, session_id, side)?;
        Ok(match transcript::resolve(target.shown(&state)?, &reference, |scope, id| target.open_agent(&state, scope, id))? {
            Resolved::Agent(t) => DetailView::Session { meta: Box::new(t.meta.clone()), items: t.view() },
            Resolved::Detail(Detail::Text(text)) => text_view(&text),
            Resolved::Detail(Detail::File(rel)) => match source::load_rel(&target.engine, &target.key, &target.session_id, side, &rel, None) {
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
