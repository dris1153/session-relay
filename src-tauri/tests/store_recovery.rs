mod support;

use std::collections::BTreeSet;
use std::io::Write;

use session_relay_lib::engine::error::Error;
use session_relay_lib::engine::lock::SyncLock;
use session_relay_lib::engine::normalize::PathRewrite;
use session_relay_lib::engine::overview;
use session_relay_lib::engine::remote;
use session_relay_lib::engine::sync::SyncMode;
use session_relay_lib::engine::transfer::{self, Pulled};
use support::*;

fn chunk_files(m: &Machine) -> BTreeSet<String> {
    let dir = m.engine.repo.dir().join(m.engine.project_path(&m.engine.keys.key_hash16(&m.key()))).join("c");
    std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()
}

#[test]
fn lease_locks_busy_and_incremental_upload() {
    let tmp = tempfile::tempdir().unwrap();
    let url = bare_remote(tmp.path());
    let id = identity();
    let mut a = Machine::new(tmp.path(), "A", &url, &id);
    a.link();
    let transcript = format!("{SID}.jsonl");

    // ~12 MiB transcript of distinct lines → several sealed chunks.
    let checkout = a.checkout.clone();
    let lines = |from: usize, n: usize| (from..from + n).map(|i| transcript_line("assistant", &checkout, &format!("{i}{}", "x".repeat(2000)))).collect::<String>();
    a.write(&transcript, lines(0, 6000).as_bytes());
    a.sync(SyncMode::Auto);
    let before = chunk_files(&a);
    assert!(before.len() >= 3, "chunks: {before:?}");

    // Appending ~1 MiB only replaces the tail chunk (at most one more sealed chunk).
    a.append(&transcript, &lines(6000, 500));
    a.sync(SyncMode::Auto);
    let after = chunk_files(&a);
    assert!(after.difference(&before).count() <= 2, "only tail chunks re-uploaded");
    assert!(before.difference(&after).count() <= 1);

    // A git lock file left by a killed run does not block the next save.
    std::fs::write(a.engine.repo.dir().join(".git").join("index.lock"), b"").unwrap();
    a.append(&transcript, &lines(6500, 1));
    assert_eq!(a.sync(SyncMode::Auto).pushed, vec![transcript.clone()]);

    // Status checks never run while a save holds the lock.
    let held = SyncLock::try_acquire(&a.engine.cfg.lock_file(), "test").unwrap();
    assert!(matches!(overview::refresh(&a.engine), Err(Error::Busy)));
    assert!(matches!(a.try_sync(SyncMode::Auto, None), Err(Error::Busy)));
    drop(held);

    // Compare-and-swap push rejects a stale expectation and an empty lease on an existing branch.
    let head = remote_head(tmp.path());
    let repo = &a.engine.repo;
    repo.prepare_worktree(Some(&head), &["/*".to_string()]).unwrap();
    std::fs::write(repo.dir().join("extra.txt"), b"x").unwrap();
    let commit = repo.snapshot_commit().unwrap();
    assert!(matches!(repo.push_lease(&commit, Some("0000000000000000000000000000000000000000")), Err(Error::LeaseRejected)));
    assert!(matches!(repo.push_lease(&commit, None), Err(Error::LeaseRejected)));
    repo.prepare_worktree(Some(&head), &["/*".to_string()]).unwrap();
    assert!(!repo.dir().join("extra.txt").exists(), "leftovers cleaned");

    // A corrupt clone is rebuilt (it is only a cache).
    std::fs::remove_dir_all(repo.dir().join(".git").join("objects")).unwrap();
    repo.recover().unwrap();
    assert_eq!(repo.fetch().unwrap(), Some(head));
}

#[test]
fn restore_leaves_a_file_claude_is_writing() {
    let tmp = tempfile::tempdir().unwrap();
    let url = bare_remote(tmp.path());
    let id = identity();
    let mut a = Machine::new(tmp.path(), "A", &url, &id);
    let mut b = Machine::new(tmp.path(), "B", &url, &id);
    a.link();
    let transcript = format!("{SID}.jsonl");
    a.write(&transcript, transcript_line("user", &a.checkout, "hello").as_bytes());
    a.sync(SyncMode::Auto);

    b.link();
    b.write(&transcript, transcript_line("user", &b.checkout, "hello").as_bytes());
    let seen = std::fs::metadata(b.file(&transcript)).map(|m| m.len()).unwrap();
    // Claude appends after the file was evaluated.
    let mut f = std::fs::OpenOptions::new().append(true).open(b.file(&transcript)).unwrap();
    f.write_all(b"{\"type\":\"assistant\"}\n").unwrap();
    drop(f);

    let engine = &b.engine;
    engine.repo.ensure_clone().unwrap();
    let sha = engine.repo.fetch().unwrap().unwrap();
    let manifest = remote::manifest(engine, Some(&sha), &b.key()).unwrap().unwrap();
    let entry = &manifest.files[&transcript];
    let rewrite = PathRewrite::new(&b.project_dir());
    let outcome = transfer::pull(engine, &sha, &engine.keys.key_hash16(&b.key()), &b.project_dir(), &b.file(&transcript), entry, &rewrite, Some((seen, 0))).unwrap();
    assert!(matches!(outcome, Pulled::BeingWritten));
    assert!(std::fs::read_to_string(b.file(&transcript)).unwrap().contains("assistant"), "local file untouched");
    assert!(!b.project_dir().join(format!("{transcript}.sr-tmp")).exists(), "temp file removed");
}

#[test]
fn broken_store_never_touches_a_parent_repo() {
    let tmp = tempfile::tempdir().unwrap();
    let url = bare_remote(tmp.path());
    let mut a = Machine::new(tmp.path(), "A", &url, &identity());
    a.link();
    // The folder above the store is itself a repo (think: a dotfiles repo in the user profile).
    let parent = tmp.path().join("A");
    assert!(std::process::Command::new("git").args(["init", "-q"]).arg(&parent).status().unwrap().success());
    let transcript = format!("{SID}.jsonl");
    a.write(&transcript, transcript_line("user", &a.checkout, "hi").as_bytes());
    a.sync(SyncMode::Auto);

    let store = a.engine.repo.dir().to_path_buf();
    std::fs::remove_dir_all(store.join(".git")).unwrap();
    std::fs::write(store.join(".git"), b"garbage").unwrap();
    a.engine.repo.recover().unwrap();
    let remotes = std::process::Command::new("git").arg("-C").arg(&parent).arg("remote").output().unwrap();
    assert!(String::from_utf8_lossy(&remotes.stdout).trim().is_empty(), "parent repo untouched");
    a.append(&transcript, &transcript_line("user", &a.checkout, "again"));
    assert_eq!(a.sync(SyncMode::Auto).pushed, vec![transcript]);
}

#[test]
fn vanished_manifest_is_a_rollback_not_an_empty_cloud() {
    let tmp = tempfile::tempdir().unwrap();
    let url = bare_remote(tmp.path());
    let mut a = Machine::new(tmp.path(), "A", &url, &identity());
    a.link();
    let transcript = format!("{SID}.jsonl");
    a.write(&transcript, transcript_line("user", &a.checkout, "hi").as_bytes());
    a.sync(SyncMode::Auto);

    // Someone publishes a snapshot without this project's manifest.
    let head = remote_head(tmp.path());
    let repo = &a.engine.repo;
    repo.prepare_worktree(Some(&head), &["/*".to_string()]).unwrap();
    let manifest = a.engine.manifest_path(&a.engine.keys.key_hash16(&a.key()));
    std::fs::remove_file(repo.dir().join(manifest)).unwrap();
    let commit = repo.snapshot_commit().unwrap();
    repo.push_lease(&commit, Some(&head)).unwrap();

    assert!(matches!(a.try_sync(SyncMode::Auto, None), Err(Error::RollbackDetected)));
    assert!(matches!(a.try_sync(SyncMode::PushOnly, None), Err(Error::RollbackDetected)));
    // Only an explicit "overwrite the cloud" republishes this machine's copy.
    assert_eq!(a.sync(SyncMode::ForceLocal).pushed, vec![transcript]);
}
