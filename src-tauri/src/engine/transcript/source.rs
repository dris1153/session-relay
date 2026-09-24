//! Where a transcript's bytes come from: this machine's Claude folder or the cloud copy.
//! Viewing never writes anything under the Claude folder.

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::engine::context::Engine;
use crate::engine::error::{Error, IoContext, Result};
use crate::engine::file_set::{classify, FileClass};
use crate::engine::fs_util::mtime_ns;
use crate::engine::links::Links;
use crate::engine::lock::SyncLock;
use crate::engine::normalize::PathRewrite;
use crate::engine::project_identity::ProjectKey;
use crate::engine::remote;
use crate::engine::transfer::refuse_links;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Local,
    Cloud,
}

pub struct Loaded {
    pub bytes: Vec<u8>,
    /// Changes whenever the content does (size + mtime, or the stored hash).
    pub signature: String,
}

/// The session's transcript; `None` when it still has the signature `known` (the caller's
/// cached parse is current).
pub fn load(engine: &Engine, key: &ProjectKey, session_id: &str, side: Side, known: Option<&str>) -> Result<Option<Loaded>> {
    load_file(engine, key, side, &format!("{session_id}.jsonl"), known)
}

/// A file of the session's folder: `tool-results/<name>` or `subagents/agent-<id>.jsonl`.
pub fn load_rel(engine: &Engine, key: &ProjectKey, session_id: &str, side: Side, rel: &str, known: Option<&str>) -> Result<Option<Loaded>> {
    // Refs come from the window: only these two folders, and only names a sync would accept
    // (no `..`, no Windows device names such as `CON`).
    if !(rel.starts_with("tool-results/") || rel.starts_with("subagents/")) {
        return Err(Error::Invalid("not a session file".into()));
    }
    load_file(engine, key, side, &format!("{session_id}/{rel}"), known)
}

/// `rel` is relative to the project's Claude folder, with `/`.
fn load_file(engine: &Engine, key: &ProjectKey, side: Side, rel: &str, known: Option<&str>) -> Result<Option<Loaded>> {
    let class = classify(rel).ok_or_else(|| Error::Invalid("not a session file".into()))?;
    let dir = Links::load(&engine.cfg.links_file())?.dir_for(key).map(|enc| engine.cfg.projects_dir().join(enc));
    match side {
        Side::Local => {
            let dir = dir.ok_or(Error::SessionNotFound)?;
            let path = rel.split('/').fold(dir.clone(), |p, part| p.join(part));
            refuse_links(&dir, &path)?;
            let meta = match std::fs::metadata(&path) {
                Ok(m) if m.is_file() => m,
                Ok(_) => return Err(Error::SessionNotFound),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(Error::SessionNotFound),
                Err(e) => return Err(e).at(&path),
            };
            let signature = format!("{}:{}", meta.len(), mtime_ns(&meta));
            if known == Some(signature.as_str()) {
                return Ok(None);
            }
            Ok(Some(Loaded { bytes: std::fs::read(&path).at(&path)?, signature }))
        }
        Side::Cloud => {
            // A save may rebuild the clone under us; the lock keeps the snapshot's objects in place.
            let _lock = SyncLock::acquire_within(&engine.cfg.lock_file(), "view", Duration::from_secs(5))?;
            let sha = engine.repo.last_snapshot().ok_or(Error::SessionNotFound)?;
            let manifest = remote::manifest(engine, Some(&sha), key)?.ok_or(Error::SessionNotFound)?;
            let entry = manifest.files.get(rel).ok_or(Error::SessionNotFound)?;
            // Transcripts were normalized on save: point their paths at this machine's folder, or
            // a visible marker. Whole files are stored as they were, like a restore writes them.
            let local = dir.unwrap_or_else(|| Path::new("<project>").to_path_buf());
            let signature = format!("{}|{}", entry.hash, local.display());
            if known == Some(signature.as_str()) {
                return Ok(None);
            }
            let stored = remote::file_content(engine, &sha, &engine.keys.key_hash16(key), entry)?;
            let bytes = if class == FileClass::Append { PathRewrite::new(&local).expand(&stored).into_owned() } else { stored };
            Ok(Some(Loaded { bytes, signature }))
        }
    }
}
