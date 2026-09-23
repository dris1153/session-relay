use std::path::PathBuf;

use super::crypto::Keys;
use super::error::{Error, Result};
use super::git_process::GitEnv;
use super::paths::CoreConfig;
use super::project_identity::ProjectKey;
use super::store_repo::StoreRepo;

pub const MARKER_FILE: &str = "session-relay.json";
const KEY_CHECK: &[u8] = b"session-relay key check";

/// Plaintext store marker. `key_check` lets a machine detect a different identity before it
/// writes a second, unreadable set of projects into the same repo.
pub fn marker_bytes(keys: &Keys) -> Vec<u8> {
    let marker = serde_json::json!({ "app": "session-relay", "v": 1, "key_check": keys.mac(KEY_CHECK) });
    serde_json::to_vec_pretty(&marker).expect("serializable")
}

/// Everything an engine operation needs; built once per process after unlock.
pub struct Engine {
    pub cfg: CoreConfig,
    pub keys: Keys,
    pub repo: StoreRepo,
}

impl Engine {
    pub fn new(cfg: CoreConfig, keys: Keys, remote_url: String, credential_helper: Option<String>, allow_file_protocol: bool) -> Self {
        let git = GitEnv { hooks_dir: cfg.empty_hooks_dir(), credential_helper, allow_file_protocol };
        let repo = StoreRepo::new(cfg.store_dir(), remote_url, git);
        Self { cfg, keys, repo }
    }

    pub fn project_path(&self, key_hash: &str) -> String {
        format!("p/{key_hash}")
    }

    pub fn chunk_path(&self, key_hash: &str, name: &str) -> String {
        format!("p/{key_hash}/c/{name}.zst.age")
    }

    pub fn manifest_path(&self, key_hash: &str) -> String {
        format!("p/{key_hash}/manifest.age")
    }

    pub fn check_identity(&self, sha: Option<&str>) -> Result<()> {
        let Some(sha) = sha else { return Ok(()) };
        let Some(bytes) = self.repo.show(sha, MARKER_FILE)? else { return Ok(()) };
        let marker: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| Error::Invalid(e.to_string()))?;
        match marker.get("key_check").and_then(|v| v.as_str()) {
            Some(check) if check != self.keys.mac(KEY_CHECK) => Err(Error::WrongIdentity),
            _ => Ok(()),
        }
    }

    /// Runs a store operation; a git failure (corrupt objects, broken index) gets one fresh clone,
    /// which is safe because the clone is only a cache. Caller holds `SyncLock`.
    pub fn with_store<T>(&self, mut op: impl FnMut() -> Result<T>) -> Result<T> {
        match op() {
            Err(Error::Git { .. }) => {
                log::warn!("rebuilding store clone after git failure");
                self.repo.rebuild()?;
                op()
            }
            other => other,
        }
    }

    pub fn base_file(&self, key: &ProjectKey) -> PathBuf {
        self.cfg.base_dir().join(format!("{}.json", self.keys.key_hash16(key)))
    }
}
