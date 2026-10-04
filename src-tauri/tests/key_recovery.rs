mod support;

use std::path::Path;
use std::process::Command;

use age::secrecy::ExposeSecret;
use session_relay_lib::engine::crypto::{unwrap_identity, Keys};
use session_relay_lib::engine::error::Error;
use session_relay_lib::engine::key_rewrap::{change_passphrase, new_recovery_key, recover_key};
use session_relay_lib::engine::key_setup::{self, KeyFiles, RECOVERY_FILE};
use session_relay_lib::engine::recovery_code::RecoveryCode;
use session_relay_lib::engine::store_repo::StoreRepo;
use session_relay_lib::engine::sync::SyncMode;
use support::{bare_remote, test_store, Machine};

const PASSPHRASE: &str = "correct horse battery staple ferry";
const NEW_PASSPHRASE: &str = "violet anvil harbor pencil sunrise";

fn files_of(store: &StoreRepo) -> KeyFiles {
    store.ensure_clone().unwrap();
    KeyFiles::from_store(store, store.fetch().unwrap().as_deref()).unwrap()
}

/// `mode type sha<TAB>name` of one root entry of the remote's `main`.
fn root_entry(root: &Path, name: &str) -> String {
    let out = Command::new("git").arg("-C").arg(root.join("remote.git")).args(["ls-tree", "refs/heads/main"]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).lines().find(|l| l.ends_with(&format!("\t{name}"))).unwrap_or_else(|| panic!("no {name} in the snapshot")).to_string()
}

#[test]
fn the_recovery_key_replaces_the_passphrase_and_keeps_the_identity() {
    let tmp = tempfile::tempdir().unwrap();
    let url = bare_remote(tmp.path());
    let (a, b) = (test_store(&tmp.path().join("A"), &url), test_store(&tmp.path().join("B"), &url));
    let (keys, code) = key_setup::create_key(&a, PASSPHRASE).unwrap();
    let recovery_before = files_of(&b).recovery.unwrap();
    let head = support::remote_head(tmp.path());

    let recovered = recover_key(&b, &files_of(&b), &code.to_string(), NEW_PASSPHRASE).unwrap();
    assert_eq!(recovered.chunk_name(b"x"), keys.chunk_name(b"x"));
    assert_ne!(support::remote_head(tmp.path()), head);

    let c = test_store(&tmp.path().join("C"), &url);
    let files = files_of(&c);
    assert!(matches!(key_setup::unlock_key(&files, PASSPHRASE), Err(Error::Decrypt)));
    assert_eq!(key_setup::unlock_key(&files, NEW_PASSPHRASE).unwrap().chunk_name(b"x"), keys.chunk_name(b"x"));
    // The recovery wrap is untouched, so the same code keeps working.
    assert_eq!(files.recovery.as_deref(), Some(recovery_before.as_slice()));
    let again = recover_key(&c, &files, &code.to_string().to_lowercase(), "maple orbit canyon violin tundra").unwrap();
    assert_eq!(again.chunk_name(b"x"), keys.chunk_name(b"x"));
}

#[test]
fn a_bad_code_a_weak_passphrase_or_a_missing_wrap_changes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let url = bare_remote(tmp.path());
    let a = test_store(&tmp.path().join("A"), &url);
    let (_, code) = key_setup::create_key(&a, PASSPHRASE).unwrap();
    let b = test_store(&tmp.path().join("B"), &url);
    let files = files_of(&b);
    let head = support::remote_head(tmp.path());

    let other = RecoveryCode::generate().unwrap().to_string();
    assert!(matches!(recover_key(&b, &files, &other, NEW_PASSPHRASE), Err(Error::Decrypt)));
    assert!(matches!(recover_key(&b, &files, "not a recovery key", NEW_PASSPHRASE), Err(Error::InvalidRecoveryCode)));
    assert!(matches!(recover_key(&b, &files, &code.to_string(), "password1"), Err(Error::WeakPassphrase)));
    let without = KeyFiles { recovery: None, ..files };
    assert!(matches!(recover_key(&b, &without, &code.to_string(), NEW_PASSPHRASE), Err(Error::NoRecoveryKey)));
    assert_eq!(support::remote_head(tmp.path()), head);
}

#[test]
fn recovering_leaves_synced_projects_untouched_and_other_machines_working() {
    let tmp = tempfile::tempdir().unwrap();
    let url = bare_remote(tmp.path());
    let (keys, code) = key_setup::create_key(&test_store(&tmp.path().join("K"), &url), PASSPHRASE).unwrap();
    let mut a = Machine::new(tmp.path(), "A", &url, keys.identity_secret().expose_secret());
    a.link();
    a.write(&format!("{}.jsonl", support::SID), support::transcript_line("user", &a.checkout, "hello").as_bytes());
    assert!(!a.sync(SyncMode::Auto).pushed.is_empty());
    let projects = root_entry(tmp.path(), "p");
    let marker = root_entry(tmp.path(), "session-relay.json");
    let keys_dir = root_entry(tmp.path(), "keys");

    let b = test_store(&tmp.path().join("B"), &url);
    recover_key(&b, &files_of(&b), &code.to_string(), NEW_PASSPHRASE).unwrap();

    assert_eq!(root_entry(tmp.path(), "p"), projects);
    assert_eq!(root_entry(tmp.path(), "session-relay.json"), marker);
    assert_ne!(root_entry(tmp.path(), "keys"), keys_dir);
    // Machine A still holds the same identity: it keeps saving into the re-wrapped store.
    a.append(&format!("{}.jsonl", support::SID), &support::transcript_line("user", &a.checkout, "more"));
    assert!(!a.sync(SyncMode::Auto).pushed.is_empty());
}

/// A real key, a synced project on machine A, and the store handle of the machine that stays unlocked.
fn unlocked_with_data(tmp: &Path, url: &str) -> (StoreRepo, Keys, RecoveryCode, Machine) {
    let store = test_store(&tmp.join("K"), url);
    let (keys, code) = key_setup::create_key(&store, PASSPHRASE).unwrap();
    let mut a = Machine::new(tmp, "A", url, keys.identity_secret().expose_secret());
    a.link();
    a.write(&format!("{}.jsonl", support::SID), support::transcript_line("user", &a.checkout, "hello").as_bytes());
    assert!(!a.sync(SyncMode::Auto).pushed.is_empty());
    (store, keys, code, a)
}

#[test]
fn an_unlocked_machine_changes_the_passphrase_without_touching_data_or_the_recovery_key() {
    let tmp = tempfile::tempdir().unwrap();
    let url = bare_remote(tmp.path());
    let (store, keys, code, mut a) = unlocked_with_data(tmp.path(), &url);
    let projects = root_entry(tmp.path(), "p");
    let head = support::remote_head(tmp.path());

    assert!(matches!(change_passphrase(&store, &keys, "password1"), Err(Error::WeakPassphrase)));
    assert_eq!(support::remote_head(tmp.path()), head);
    change_passphrase(&store, &keys, NEW_PASSPHRASE).unwrap();

    assert_eq!(root_entry(tmp.path(), "p"), projects);
    let files = files_of(&test_store(&tmp.path().join("C"), &url));
    assert!(matches!(key_setup::unlock_key(&files, PASSPHRASE), Err(Error::Decrypt)));
    assert_eq!(key_setup::unlock_key(&files, NEW_PASSPHRASE).unwrap().chunk_name(b"x"), keys.chunk_name(b"x"));
    assert_eq!(unwrap_identity(files.recovery.as_deref().unwrap(), code.canonical()).unwrap().chunk_name(b"x"), keys.chunk_name(b"x"));
    a.append(&format!("{}.jsonl", support::SID), &support::transcript_line("user", &a.checkout, "more"));
    assert!(!a.sync(SyncMode::Auto).pushed.is_empty());
}

#[test]
fn a_new_recovery_key_replaces_the_old_one_and_adds_one_to_an_older_store() {
    let tmp = tempfile::tempdir().unwrap();
    let url = bare_remote(tmp.path());
    let (store, keys, old_code, _a) = unlocked_with_data(tmp.path(), &url);
    let projects = root_entry(tmp.path(), "p");

    let fresh = new_recovery_key(&store, &keys).unwrap();
    assert_ne!(fresh.canonical(), old_code.canonical());
    assert_eq!(root_entry(tmp.path(), "p"), projects);
    let files = files_of(&test_store(&tmp.path().join("C"), &url));
    let wrap = files.recovery.as_deref().unwrap();
    assert!(matches!(unwrap_identity(wrap, old_code.canonical()), Err(Error::Decrypt)));
    assert_eq!(unwrap_identity(wrap, fresh.canonical()).unwrap().chunk_name(b"x"), keys.chunk_name(b"x"));
    assert_eq!(key_setup::unlock_key(&files, PASSPHRASE).unwrap().chunk_name(b"x"), keys.chunk_name(b"x"));

    // A store made before recovery keys existed has no recovery wrap yet.
    let sha = store.fetch().unwrap().unwrap();
    store.prepare_worktree(Some(&sha), &["/*".to_string()]).unwrap();
    std::fs::remove_file(store.dir().join(RECOVERY_FILE)).unwrap();
    store.push_lease(&store.snapshot_commit().unwrap(), Some(&sha)).unwrap();
    assert!(files_of(&test_store(&tmp.path().join("D"), &url)).recovery.is_none());
    let added = new_recovery_key(&store, &keys).unwrap();
    let files = files_of(&test_store(&tmp.path().join("E"), &url));
    assert_eq!(unwrap_identity(files.recovery.as_deref().unwrap(), added.canonical()).unwrap().chunk_name(b"x"), keys.chunk_name(b"x"));
}
