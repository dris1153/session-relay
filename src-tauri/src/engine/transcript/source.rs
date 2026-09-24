//! Where a transcript's bytes come from: this machine's Claude folder or the cloud copy.
//! Viewing never writes anything under the Claude folder.

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::engine::context::Engine;
use crate::engine::error::{Error, IoContext, Result};
use crate::engine::fs_util::mtime_ns;
use crate::engine::links::Links;
use crate::engine::lock::SyncLock;
use crate::engine::normalize::PathRewrite;
use crate::engine::project_identity::ProjectKey;
use crate::engine::remote;

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

/// `None` when the content still has the signature `known` (the caller's cached parse is current).
pub fn load(engine: &Engine, key: &ProjectKey, session_id: &str, side: Side, known: Option<&str>) -> Result<Option<Loaded>> {
    // The id comes from the window and becomes a path: nothing but a UUID gets through.
    if session_id.len() != 36 || !session_id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
        return Err(Error::Invalid("not a session id".into()));
    }
    let rel = format!("{session_id}.jsonl");
    let dir = Links::load(&engine.cfg.links_file())?.dir_for(key).map(|enc| engine.cfg.projects_dir().join(enc));
    match side {
        Side::Local => {
            let path = dir.ok_or(Error::SessionNotFound)?.join(&rel);
            let meta = match std::fs::metadata(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(Error::SessionNotFound),
                other => other.at(&path)?,
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
            let entry = manifest.files.get(&rel).ok_or(Error::SessionNotFound)?;
            // Paths were normalized on save: point them at this machine's folder, or a visible marker.
            let local = dir.unwrap_or_else(|| Path::new("<project>").to_path_buf());
            let signature = format!("{}|{}", entry.hash, local.display());
            if known == Some(signature.as_str()) {
                return Ok(None);
            }
            let stored = remote::file_content(engine, &sha, &engine.keys.key_hash16(key), entry)?;
            Ok(Some(Loaded { bytes: PathRewrite::new(&local).expand(&stored).into_owned(), signature }))
        }
    }
}
