use serde::Serialize;

use super::context::{marker_bytes, MARKER_FILE};
use super::crypto::{unwrap_identity, wrap_identity, Keys};
use super::error::{Error, Result};
use super::fs_util::write_atomic;
use super::store_repo::StoreRepo;

pub const IDENTITY_FILE: &str = "keys/identity.age";
const MIN_PASSPHRASE_SCORE: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StoreKeyState {
    /// Empty store (or only our scaffolding without a key): this machine creates the key.
    NeedsNewKey,
    NeedsUnlock,
    /// Has other content and no key: never write into it.
    Foreign,
}

/// The few remote files that decide the key state: read over the REST API during onboarding
/// (no clone of a possibly large store), or from a fetched snapshot.
#[derive(Debug, Clone, Default)]
pub struct KeyFiles {
    pub root: Vec<String>,
    pub marker: Option<Vec<u8>>,
    pub identity: Option<Vec<u8>>,
}

impl KeyFiles {
    /// From a fetched snapshot (`None`: the remote has none). Caller holds `SyncLock`.
    pub fn from_store(store: &StoreRepo, sha: Option<&str>) -> Result<Self> {
        let Some(sha) = sha else { return Ok(Self::default()) };
        let root = store.ls_tree(sha, "")?.iter().filter_map(|line| line.split('\t').nth(1)).map(str::to_owned).collect();
        Ok(Self { root, marker: store.show(sha, MARKER_FILE)?, identity: store.show(sha, IDENTITY_FILE)? })
    }
}

pub fn store_key_state(files: &KeyFiles) -> StoreKeyState {
    if files.identity.is_some() {
        return StoreKeyState::NeedsUnlock;
    }
    // Project data without a key (e.g. a reset repo an old engine wrote into) is not ours to re-key.
    if files.root.iter().all(|name| name == MARKER_FILE || name == ".gitattributes") {
        StoreKeyState::NeedsNewKey
    } else {
        StoreKeyState::Foreign
    }
}

pub fn check_strength(passphrase: &str) -> Result<()> {
    let score = u8::from(zxcvbn::zxcvbn(passphrase, &[]).score());
    if score < MIN_PASSPHRASE_SCORE {
        return Err(Error::WeakPassphrase);
    }
    Ok(())
}

/// First machine: generates the identity and publishes it passphrase-wrapped. If another
/// machine won the race, returns `KeyExists` without overwriting anything. Caller holds `SyncLock`.
pub fn create_key(store: &StoreRepo, passphrase: &str) -> Result<Keys> {
    check_strength(passphrase)?;
    store.recover()?;
    let sha = store.fetch()?;
    match store_key_state(&KeyFiles::from_store(store, sha.as_deref())?) {
        StoreKeyState::NeedsNewKey => {}
        StoreKeyState::NeedsUnlock => return Err(Error::KeyExists),
        StoreKeyState::Foreign => return Err(Error::Invalid("the repository has content that is not a session-relay store".into())),
    }
    let keys = Keys::generate();
    let wrapped = wrap_identity(&keys, passphrase)?;
    store.prepare_worktree(sha.as_deref(), &["/*".to_string()])?;
    let root = store.dir();
    write_atomic(&root.join(MARKER_FILE), &marker_bytes(&keys))?;
    write_atomic(&root.join(".gitattributes"), b"* -text -diff\n")?;
    write_atomic(&root.join(IDENTITY_FILE), &wrapped)?;
    let commit = store.snapshot_commit()?;
    match store.push_lease(&commit, sha.as_deref()) {
        Err(Error::LeaseRejected) => Err(Error::KeyExists),
        other => other.map(|()| keys),
    }
}

/// Other machines: unwraps the published identity and checks it matches the store marker.
pub fn unlock_key(files: &KeyFiles, passphrase: &str) -> Result<Keys> {
    let wrapped = files.identity.as_deref().ok_or_else(|| Error::Invalid("the store has no key yet".into()))?;
    let keys = unwrap_identity(wrapped, passphrase)?;
    if !marker_matches(files, &keys)? {
        return Err(Error::WrongIdentity);
    }
    Ok(keys)
}

/// Whether `keys` is the identity this store was created with (a store without marker matches).
pub fn marker_matches(files: &KeyFiles, keys: &Keys) -> Result<bool> {
    let Some(marker) = &files.marker else { return Ok(true) };
    let expected: serde_json::Value = serde_json::from_slice(&marker_bytes(keys)).expect("valid json");
    let actual: serde_json::Value = serde_json::from_slice(marker).map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(actual.get("key_check") == expected.get("key_check"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(root: &[&str], marker: Option<&Keys>, identity: Option<Vec<u8>>) -> KeyFiles {
        KeyFiles { root: root.iter().map(|s| s.to_string()).collect(), marker: marker.map(marker_bytes), identity }
    }

    #[test]
    fn key_state_from_root_listing() {
        let keys = Keys::generate();
        assert_eq!(store_key_state(&files(&[], None, None)), StoreKeyState::NeedsNewKey);
        assert_eq!(store_key_state(&files(&[MARKER_FILE, ".gitattributes"], Some(&keys), None)), StoreKeyState::NeedsNewKey);
        assert_eq!(store_key_state(&files(&["README.md"], None, None)), StoreKeyState::Foreign);
        // Marker plus project data but no key: minting a new identity would split the store.
        assert_eq!(store_key_state(&files(&[MARKER_FILE, ".gitattributes", "p"], Some(&keys), None)), StoreKeyState::Foreign);
        assert_eq!(store_key_state(&files(&[MARKER_FILE, "keys"], Some(&keys), Some(vec![1]))), StoreKeyState::NeedsUnlock);
    }

    #[test]
    fn unlock_checks_passphrase_and_marker() {
        let (keys, other) = (Keys::generate(), Keys::generate());
        let wrapped = wrap_identity(&keys, "correct horse battery staple ferry").unwrap();
        let good = files(&[MARKER_FILE, "keys"], Some(&keys), Some(wrapped.clone()));
        assert!(matches!(unlock_key(&good, "wrong passphrase entirely"), Err(Error::Decrypt)));
        assert_eq!(unlock_key(&good, "correct horse battery staple ferry").unwrap().chunk_name(b"x"), keys.chunk_name(b"x"));
        let mismatched = files(&[MARKER_FILE, "keys"], Some(&other), Some(wrapped));
        assert!(matches!(unlock_key(&mismatched, "correct horse battery staple ferry"), Err(Error::WrongIdentity)));
        assert!(matches!(unlock_key(&files(&[], None, Some(b"not age".to_vec())), "x"), Err(Error::Decrypt)));
        assert!(matches!(unlock_key(&files(&[], None, None), "x"), Err(Error::Invalid(_))));
    }
}
