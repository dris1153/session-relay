use super::crypto::Keys;
use super::error::{Error, Result};
use super::fs_util::remove_dir;
use super::key_setup::{check_strength, publish_key_files, store_key_state, KeyFiles, StoreKeyState};
use super::paths::CoreConfig;
use super::recovery_code::RecoveryCode;
use super::store_repo::StoreRepo;

/// Replaces a store nobody can unlock with an empty one under a new identity, in the same
/// repository (so the GitHub App installation stays). `expected_head` is the remote head read
/// BEFORE `files`, so anything pushed since makes the push fail with `LeaseRejected` instead of
/// destroying it. Caller holds `SyncLock`.
pub fn reset_store(store: &StoreRepo, expected_head: &str, files: &KeyFiles, passphrase: &str) -> Result<(Keys, RecoveryCode)> {
    if store_key_state(files) != StoreKeyState::NeedsUnlock {
        return Err(Error::Invalid("only a store that has a key can be started over".into()));
    }
    check_strength(passphrase)?;
    // The old clone is only a cache of data nobody can read: drop it instead of downloading it.
    store.rebuild()?;
    let keys = Keys::generate();
    let code = RecoveryCode::generate()?;
    publish_key_files(store, None, Some(expected_head), &keys, passphrase, &code)?;
    Ok((keys, code))
}

/// Sync bases and encrypted backups are bound to the old identity and unreadable after a reset.
/// Links, pending markers and Claude's own files stay.
pub fn clear_key_bound_state(cfg: &CoreConfig) -> Result<()> {
    remove_dir(&cfg.base_dir())?;
    remove_dir(&cfg.backups_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clears_bases_and_backups_but_not_links() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = CoreConfig { app_dir: dir.path().to_path_buf(), claude_home: dir.path().join("claude"), machine_name: "m".into() };
        for sub in [cfg.base_dir(), cfg.backups_dir()] {
            std::fs::create_dir_all(&sub).unwrap();
            std::fs::write(sub.join("x"), b"x").unwrap();
        }
        std::fs::write(cfg.links_file(), b"{}").unwrap();
        clear_key_bound_state(&cfg).unwrap();
        assert!(!cfg.base_dir().exists() && !cfg.backups_dir().exists());
        assert!(cfg.links_file().exists());
        // Nothing to remove is fine too.
        clear_key_bound_state(&cfg).unwrap();
    }
}
