mod support;

use std::process::Command;

use session_relay_lib::engine::error::Error;
use session_relay_lib::engine::key_setup::{self, KeyFiles, StoreKeyState, IDENTITY_FILE};
use session_relay_lib::engine::store_repo::StoreRepo;
use session_relay_lib::engine::sync::SyncMode;
use support::{bare_remote, identity, test_store, Machine};

const PASSPHRASE: &str = "correct horse battery staple ferry";

fn state_of(store: &StoreRepo) -> StoreKeyState {
    key_setup::store_key_state(&files_of(store))
}

fn files_of(store: &StoreRepo) -> KeyFiles {
    store.ensure_clone().unwrap();
    KeyFiles::from_store(store, store.fetch().unwrap().as_deref()).unwrap()
}

#[test]
fn first_machine_creates_second_unlocks_and_nobody_overwrites() {
    let tmp = tempfile::tempdir().unwrap();
    let url = bare_remote(tmp.path());
    let (a, b) = (test_store(&tmp.path().join("A"), &url), test_store(&tmp.path().join("B"), &url));
    assert_eq!(state_of(&a), StoreKeyState::NeedsNewKey);

    assert!(matches!(key_setup::create_key(&a, "password1"), Err(Error::WeakPassphrase)));
    let created = key_setup::create_key(&a, PASSPHRASE).unwrap();
    let head = a.remote_head().unwrap();
    // B arrives second: it must unlock the existing key, never mint a new one.
    assert!(matches!(key_setup::create_key(&b, PASSPHRASE), Err(Error::KeyExists)));
    assert_eq!(a.remote_head().unwrap(), head);
    assert_eq!(state_of(&b), StoreKeyState::NeedsUnlock);
    assert!(matches!(key_setup::unlock_key(&files_of(&b), "wrong passphrase entirely"), Err(Error::Decrypt)));
    let unlocked = key_setup::unlock_key(&files_of(&b), PASSPHRASE).unwrap();
    assert_eq!(unlocked.chunk_name(b"x"), created.chunk_name(b"x"));
}

#[test]
fn a_repo_with_other_content_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let url = bare_remote(tmp.path());
    let a = test_store(&tmp.path().join("A"), &url);
    a.ensure_clone().unwrap();
    a.prepare_worktree(None, &["/*".to_string()]).unwrap();
    std::fs::write(a.dir().join("README.md"), b"someone else's repo").unwrap();
    a.push_lease(&a.snapshot_commit().unwrap(), None).unwrap();

    let b = test_store(&tmp.path().join("B"), &url);
    assert_eq!(state_of(&b), StoreKeyState::Foreign);
    assert!(matches!(key_setup::create_key(&b, PASSPHRASE), Err(Error::Invalid(_))));
}

#[test]
fn a_marker_without_key_or_data_can_be_keyed_again() {
    let tmp = tempfile::tempdir().unwrap();
    let url = bare_remote(tmp.path());
    let a = test_store(&tmp.path().join("A"), &url);
    key_setup::create_key(&a, PASSPHRASE).unwrap();
    // Simulates a key file lost from the store: only the marker and .gitattributes remain.
    let sha = a.fetch().unwrap().unwrap();
    a.prepare_worktree(Some(&sha), &["/*".to_string()]).unwrap();
    std::fs::remove_file(a.dir().join(IDENTITY_FILE)).unwrap();
    a.push_lease(&a.snapshot_commit().unwrap(), Some(&sha)).unwrap();

    let c = test_store(&tmp.path().join("C"), &url);
    assert_eq!(state_of(&c), StoreKeyState::NeedsNewKey);
    let keys = key_setup::create_key(&c, PASSPHRASE).unwrap();
    assert_eq!(key_setup::unlock_key(&files_of(&c), PASSPHRASE).unwrap().chunk_name(b"x"), keys.chunk_name(b"x"));
}

#[test]
fn the_engine_refuses_to_publish_into_a_reset_store() {
    let tmp = tempfile::tempdir().unwrap();
    let url = bare_remote(tmp.path());
    let mut a = Machine::new(tmp.path(), "A", &url, &identity());
    // The user deletes and re-creates the repository on GitHub. (A project synced before would
    // already stop at the anti-rollback check; this one was never synced.)
    let reset = Command::new("git").arg("-C").arg(tmp.path().join("remote.git")).args(["update-ref", "-d", "refs/heads/main"]).status().unwrap();
    assert!(reset.success());
    a.link();
    a.write(&format!("{}.jsonl", support::SID), support::transcript_line("user", &a.checkout, "hello").as_bytes());
    let result = a.try_sync(SyncMode::Auto, None).map(|r| r.pushed);
    assert!(matches!(result, Err(Error::StoreReset)), "{result:?}");
    assert!(a.engine.repo.remote_head().unwrap().is_none());
}
