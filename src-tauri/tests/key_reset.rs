mod support;

use std::path::Path;
use std::process::Command;

use age::secrecy::ExposeSecret;
use session_relay_lib::engine::context::Engine;
use session_relay_lib::engine::crypto::{unwrap_identity, Keys};
use session_relay_lib::engine::error::Error;
use session_relay_lib::engine::key_reset::reset_store;
use session_relay_lib::engine::key_rewrap::recover_key;
use session_relay_lib::engine::key_setup::{self, KeyFiles};
use session_relay_lib::engine::store_repo::StoreRepo;
use session_relay_lib::engine::sync::SyncMode;
use support::{bare_remote, test_store, Machine};

const PASSPHRASE: &str = "correct horse battery staple ferry";
const NEW_PASSPHRASE: &str = "violet anvil harbor pencil sunrise";

fn files_of(store: &StoreRepo) -> KeyFiles {
    store.ensure_clone().unwrap();
    KeyFiles::from_store(store, store.fetch().unwrap().as_deref()).unwrap()
}

fn root_names(root: &Path) -> Vec<String> {
    let out = Command::new("git").arg("-C").arg(root.join("remote.git")).args(["ls-tree", "--name-only", "refs/heads/main"]).output().unwrap();
    let mut names: Vec<String> = String::from_utf8_lossy(&out.stdout).lines().map(str::to_owned).collect();
    names.sort();
    names
}

#[test]
fn start_over_replaces_the_store_and_the_other_machine_unlocks_again() {
    let tmp = tempfile::tempdir().unwrap();
    let url = bare_remote(tmp.path());
    let (old_keys, old_code) = key_setup::create_key(&test_store(&tmp.path().join("K"), &url), PASSPHRASE).unwrap();
    let old_identity = old_keys.identity_secret().expose_secret().to_string();
    let (mut a, mut b) = (Machine::new(tmp.path(), "A", &url, &old_identity), Machine::new(tmp.path(), "B", &url, &old_identity));
    let session = format!("{}.jsonl", support::SID);
    a.link();
    a.write(&session, support::transcript_line("user", &a.checkout, "hello").as_bytes());
    assert!(!a.sync(SyncMode::Auto).pushed.is_empty());
    b.link();
    b.sync(SyncMode::Auto);
    let local_before = std::fs::read(a.file(&session)).unwrap();
    assert!(root_names(tmp.path()).contains(&"p".to_string()));

    // The user is on a machine that cannot unlock the store.
    let c = test_store(&tmp.path().join("C"), &url);
    let files = files_of(&c);
    let head = support::remote_head(tmp.path());
    assert!(matches!(reset_store(&c, &head, &files, "password1"), Err(Error::WeakPassphrase)));
    assert_eq!(support::remote_head(tmp.path()), head);
    let (new_keys, new_code) = reset_store(&c, &head, &files, NEW_PASSPHRASE).unwrap();

    assert_eq!(root_names(tmp.path()), [".gitattributes", "keys", "session-relay.json"]);
    let fresh = files_of(&test_store(&tmp.path().join("D"), &url));
    assert!(matches!(key_setup::unlock_key(&fresh, PASSPHRASE), Err(Error::Decrypt)));
    assert!(matches!(unwrap_identity(fresh.recovery.as_deref().unwrap(), old_code.canonical()), Err(Error::Decrypt)));
    let unlocked = key_setup::unlock_key(&fresh, NEW_PASSPHRASE).unwrap();
    assert_eq!(unlocked.chunk_name(b"x"), new_keys.chunk_name(b"x"));
    assert_ne!(unlocked.chunk_name(b"x"), old_keys.chunk_name(b"x"));
    let by_code = unwrap_identity(fresh.recovery.as_deref().unwrap(), new_code.canonical()).unwrap();
    assert_eq!(by_code.chunk_name(b"x"), new_keys.chunk_name(b"x"));
    assert_eq!(std::fs::read(a.file(&session)).unwrap(), local_before, "sessions on the machines are never touched");

    // B still holds the old identity and a sync base keyed by it.
    let stale = b.try_sync(SyncMode::Auto, None).map(|r| r.pushed);
    assert!(matches!(stale, Err(Error::WrongIdentity)), "{stale:?}");
    b.engine = Engine::new(b.engine.cfg.clone(), Keys::parse(new_keys.identity_secret().expose_secret()).unwrap(), url.clone(), None, true);
    assert!(!b.sync(SyncMode::Auto).pushed.is_empty(), "B saves its sessions into the new store");
    assert!(root_names(tmp.path()).contains(&"p".to_string()));
}

#[test]
fn only_a_store_with_a_key_can_be_started_over() {
    let tmp = tempfile::tempdir().unwrap();
    let url = bare_remote(tmp.path());
    let a = test_store(&tmp.path().join("A"), &url);
    // Nothing there yet: there is no key to give up on.
    assert!(matches!(reset_store(&a, "0000", &files_of(&a), NEW_PASSPHRASE), Err(Error::Invalid(_))));

    // Someone else's repository: never replaced.
    a.prepare_worktree(None, &["/*".to_string()]).unwrap();
    std::fs::write(a.dir().join("README.md"), b"someone else's repo").unwrap();
    a.push_lease(&a.snapshot_commit().unwrap(), None).unwrap();
    let head = support::remote_head(tmp.path());
    let b = test_store(&tmp.path().join("B"), &url);
    assert!(matches!(reset_store(&b, &head, &files_of(&b), NEW_PASSPHRASE), Err(Error::Invalid(_))));
    assert_eq!(support::remote_head(tmp.path()), head);
    assert_eq!(root_names(tmp.path()), ["README.md"]);
}

#[test]
fn a_push_after_the_head_was_read_makes_the_reset_fail_and_keeps_that_push() {
    let tmp = tempfile::tempdir().unwrap();
    let url = bare_remote(tmp.path());
    let (_, code) = key_setup::create_key(&test_store(&tmp.path().join("K"), &url), PASSPHRASE).unwrap();
    let c = test_store(&tmp.path().join("C"), &url);
    let files = files_of(&c);
    let stale_head = support::remote_head(tmp.path());

    // Another machine recovers and sets a new passphrase while this one is about to reset.
    let b = test_store(&tmp.path().join("B"), &url);
    recover_key(&b, &files_of(&b), &code.to_string(), NEW_PASSPHRASE).unwrap();
    let moved = support::remote_head(tmp.path());
    assert_ne!(moved, stale_head);

    let result = reset_store(&c, &stale_head, &files, "maple orbit canyon violin tundra").map(|_| ());
    assert!(matches!(result, Err(Error::LeaseRejected)), "{result:?}");
    assert_eq!(support::remote_head(tmp.path()), moved);
    key_setup::unlock_key(&files_of(&test_store(&tmp.path().join("D"), &url)), NEW_PASSPHRASE).unwrap();
}
