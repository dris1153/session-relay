mod support;

use std::os::windows::fs::OpenOptionsExt;

use session_relay_lib::engine::overview;
use session_relay_lib::engine::sync::SyncMode;
use support::*;

fn pair(root: &std::path::Path) -> (Machine, Machine) {
    let (url, id) = (bare_remote(root), identity());
    let mut a = Machine::new(root, "A", &url, &id);
    let mut b = Machine::new(root, "B", &url, &id);
    a.link();
    b.link();
    (a, b)
}

#[test]
fn corrupt_clone_is_rebuilt_on_the_next_sync() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut a, _) = pair(tmp.path());
    let transcript = format!("{SID}.jsonl");
    a.write(&transcript, transcript_line("user", &a.checkout, "one").as_bytes());
    a.sync(SyncMode::Auto);
    std::fs::remove_dir_all(a.engine.repo.dir().join(".git").join("objects")).unwrap();
    a.append(&transcript, &transcript_line("user", &a.checkout, "two"));
    assert_eq!(a.sync(SyncMode::Auto).pushed, vec![transcript]);
}

#[test]
fn a_locked_file_is_skipped_and_the_rest_still_syncs() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut a, mut b) = pair(tmp.path());
    let (locked, other) = (format!("{SID}/tool-results/locked.txt"), format!("{SID}/tool-results/other.txt"));
    a.write(&locked, b"v1");
    a.write(&other, b"v1");
    a.sync(SyncMode::Auto);
    b.sync(SyncMode::Auto);
    a.write(&locked, b"v2 from A");
    a.write(&other, b"v2 from A");
    a.sync(SyncMode::Auto);

    // Another process holds the file without sharing (like an indexer or antivirus scan).
    let handle = std::fs::OpenOptions::new().read(true).share_mode(0).open(b.file(&locked)).unwrap();
    let report = b.sync(SyncMode::Auto);
    drop(handle);
    assert_eq!(report.pulled, vec![other.clone()]);
    assert!(report.skipped.iter().any(|(rel, _)| *rel == locked));
    assert_eq!(std::fs::read(b.file(&locked)).unwrap(), b"v1", "left untouched");
    assert_eq!(b.sync(SyncMode::Auto).pulled, vec![locked], "retried next time");
}

#[test]
fn overwrite_cloud_does_not_reupload_deliberately_deleted_sessions() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut a, mut b) = pair(tmp.path());
    let (gone, kept) = (format!("{SID}.jsonl"), "memory/MEMORY.md".to_string());
    a.write(&gone, transcript_line("user", &a.checkout, "old session").as_bytes());
    a.write(&kept, b"v1\n");
    a.sync(SyncMode::Auto);
    b.sync(SyncMode::Auto);
    overview::delete_remote_session(&a.engine, &a.key(), SID).unwrap();

    b.write(&kept, b"v2 from B\n");
    let report = b.sync(SyncMode::ForceLocal);
    assert_eq!(report.pushed, vec![kept]);
    // Picking the file explicitly does republish it.
    assert_eq!(b.try_sync(SyncMode::ForceLocal, Some(std::slice::from_ref(&gone))).unwrap().pushed, vec![gone]);
}
