use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::error::{Error, IoContext, Result};
use super::fs_util::write_atomic;

/// What this machine last saw in sync for one file (the common ancestor of a 3-way compare).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaseEntry {
    /// Local file length and mtime when it matched; lets unchanged files skip hashing.
    pub raw_len: u64,
    pub mtime_ns: i64,
    /// Stored (normalized) size and keyed hash.
    pub size: u64,
    pub hash: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct BaseState {
    /// Highest manifest generation seen; an older one means a replayed snapshot.
    pub generation_seen: u64,
    pub files: BTreeMap<String, BaseEntry>,
}

impl BaseState {
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read(path) {
            // Never fall back to empty: that would resurrect cleaned-up sessions and disable rollback checks.
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| Error::Invalid(format!("{}: {e}", path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).at(path),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        write_atomic(path, &serde_json::to_vec(self).expect("serializable"))
    }
}
