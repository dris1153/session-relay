use super::crypto::{unwrap_identity, wrap_identity, Keys};
use super::error::{Error, Result};
use super::fs_util::write_atomic;
use super::key_setup::{check_strength, marker_matches, KeyFiles, IDENTITY_FILE, RECOVERY_FILE};
use super::recovery_code::RecoveryCode;
use super::store_repo::StoreRepo;

/// Unlocks with the recovery key and replaces the passphrase wrap with one for `new_passphrase`.
/// The identity itself is unchanged, so every other machine keeps working. Caller holds `SyncLock`.
pub fn recover_key(store: &StoreRepo, files: &KeyFiles, recovery_key: &str, new_passphrase: &str) -> Result<Keys> {
    check_strength(new_passphrase)?;
    let code = RecoveryCode::parse(recovery_key)?;
    let wrapped = files.recovery.as_deref().ok_or(Error::NoRecoveryKey)?;
    let keys = unwrap_identity(wrapped, code.canonical())?;
    if !marker_matches(files, &keys)? {
        return Err(Error::WrongIdentity);
    }
    rewrite_key_files(store, &keys, Some(&wrap_identity(&keys, new_passphrase)?), None)?;
    Ok(keys)
}

/// Same identity, new passphrase: for a machine that is already unlocked. Caller holds `SyncLock`.
pub fn change_passphrase(store: &StoreRepo, keys: &Keys, new_passphrase: &str) -> Result<()> {
    check_strength(new_passphrase)?;
    rewrite_key_files(store, keys, Some(&wrap_identity(keys, new_passphrase)?), None)
}

/// Replaces (or adds) the recovery wrap; the previous recovery key stops working. Returns the new
/// one, to be shown once. Caller holds `SyncLock`.
pub fn new_recovery_key(store: &StoreRepo, keys: &Keys) -> Result<RecoveryCode> {
    let code = RecoveryCode::generate()?;
    rewrite_key_files(store, keys, None, Some(&wrap_identity(keys, code.canonical())?))?;
    Ok(code)
}

/// Pushes a snapshot that differs from the current one only in the given files under `keys/`.
/// Only the marker and `keys/` are checked out; every project stays in the index untouched.
fn rewrite_key_files(store: &StoreRepo, keys: &Keys, identity: Option<&[u8]>, recovery: Option<&[u8]>) -> Result<()> {
    store.recover()?;
    let sha = store.fetch()?.ok_or(Error::StoreReset)?;
    let current = KeyFiles::from_store(store, Some(&sha))?;
    if current.identity.is_none() {
        return Err(Error::Invalid("the store has no key yet".into()));
    }
    if !marker_matches(&current, keys)? {
        return Err(Error::WrongIdentity);
    }
    store.prepare_worktree(Some(&sha), &["/session-relay.json".to_string(), "/.gitattributes".to_string(), "/keys/".to_string()])?;
    for (path, bytes) in [(IDENTITY_FILE, identity), (RECOVERY_FILE, recovery)] {
        if let Some(bytes) = bytes {
            write_atomic(&store.dir().join(path), bytes)?;
        }
    }
    let commit = store.snapshot_commit()?;
    if without_keys(store.ls_tree(&sha, "")?) != without_keys(store.ls_tree(&commit, "")?) {
        return Err(Error::Invalid("snapshot would change more than the key files".into()));
    }
    store.push_lease(&commit, Some(&sha))
}

/// Root entries (`mode type sha<TAB>name`) of a snapshot except `keys`; a project tree is one entry.
fn without_keys(entries: Vec<String>) -> Vec<String> {
    entries.into_iter().filter(|e| !e.ends_with("\tkeys")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_keys_entry_is_ignored() {
        let root = |keys: &str, p: &str| vec![format!("040000 tree {keys}\tkeys"), format!("040000 tree {p}\tp"), "100644 blob c0ffee\tsession-relay.json".to_string()];
        assert_eq!(without_keys(root("aaa", "ppp")), without_keys(root("bbb", "ppp")));
        assert_ne!(without_keys(root("aaa", "ppp")), without_keys(root("aaa", "qqq")));
        assert!(!without_keys(root("aaa", "ppp")).iter().any(|e| e.ends_with("\tkeys")));
    }
}
