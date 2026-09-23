mod support;

use session_relay_lib::engine::project_identity::ProjectKey;
use session_relay_lib::engine::sync::SyncMode;
use support::*;

fn machines(root: &std::path::Path) -> (Machine, Machine) {
    let (url, id) = (bare_remote(root), identity());
    let mut a = Machine::new(root, "A", &url, &id);
    let mut b = Machine::new(root, "B", &url, &id);
    a.link();
    b.link();
    (a, b)
}

#[test]
fn subfolder_sessions_use_subpath_key_and_memory_stays_with_repo_root() {
    let tmp = tempfile::tempdir().unwrap();
    let (a, b) = machines(tmp.path());
    let sub = ProjectKey { remote: REMOTE.into(), subpath: "apps/web".into() };
    let transcript = format!("{SID}.jsonl");
    let write = |dir: std::path::PathBuf, rel: &str, bytes: &[u8]| {
        std::fs::create_dir_all(dir.join(rel).parent().unwrap()).unwrap();
        std::fs::write(dir.join(rel), bytes).unwrap();
    };
    write(a.dir_of("apps/web"), &transcript, transcript_line("user", &a.checkout.join("apps").join("web"), "in web").as_bytes());
    write(a.dir_of("apps/web"), "memory/MEMORY.md", b"not synced for subfolders\n");
    write(a.dir_of(""), "memory/MEMORY.md", b"repo memory\n");

    assert_eq!(a.sync_key(&sub, SyncMode::Auto, None).unwrap().pushed, vec![transcript.clone()]);
    assert_eq!(a.sync_key(&a.key(), SyncMode::Auto, None).unwrap().pushed, vec!["memory/MEMORY.md".to_string()]);
    assert_eq!(b.sync_key(&sub, SyncMode::Auto, None).unwrap().pulled, vec![transcript.clone()]);
    b.sync_key(&b.key(), SyncMode::Auto, None).unwrap();
    assert!(b.dir_of("apps/web").join(&transcript).is_file());
    assert!(!b.dir_of("apps/web").join("memory").exists());
    assert_eq!(std::fs::read(b.dir_of("").join("memory").join("MEMORY.md")).unwrap(), b"repo memory\n");
}

#[test]
fn partial_last_line_excluded_until_completed() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut a, mut b) = machines(tmp.path());
    let transcript = format!("{SID}.jsonl");
    let complete = transcript_line("user", &a.checkout, "complete message");
    a.write(&transcript, format!("{complete}{{\"type\":\"assistant\",\"text\":\"incomplete").as_bytes());
    assert_eq!(a.sync(SyncMode::Auto).pushed, vec![transcript.clone()]);
    b.sync(SyncMode::Auto);
    let first = std::fs::read_to_string(b.file(&transcript)).unwrap();
    assert!(first.contains("complete message") && !first.contains("incomplete"));

    a.append(&transcript, "\"}\n");
    assert_eq!(a.sync(SyncMode::Auto).pushed, vec![transcript.clone()]);
    assert_eq!(b.sync(SyncMode::Auto).pulled, vec![transcript.clone()]);
    assert!(std::fs::read_to_string(b.file(&transcript)).unwrap().contains("incomplete"));
}

#[test]
fn single_line_over_32_mib_with_path_straddling_the_hard_cut() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut a, mut b) = machines(tmp.path());
    let transcript = format!("{SID}.jsonl");
    // One JSON line whose normalized form places the placeholder across the 32 MiB hard cut.
    let cut = 32 * 1024 * 1024;
    let head = r#"{"type":"user","pad":""#;
    let pad = "x".repeat(cut - head.len() - 12);
    let tail = format!(r#"","note":"{}\\{SID}\\tool-results\\out.txt"}}"#, a.escaped_dir());
    let line = format!("{head}{pad}{tail}\n");
    assert!(line.len() > cut + 10);
    a.write(&transcript, line.as_bytes());
    assert_eq!(a.sync(SyncMode::Auto).pushed, vec![transcript.clone()]);
    assert_eq!(b.sync(SyncMode::Auto).pulled, vec![transcript.clone()]);
    let restored = std::fs::read_to_string(b.file(&transcript)).unwrap();
    assert!(restored.contains(&format!(r"{}\\{SID}\\tool-results", b.escaped_dir())), "placeholder expanded across the cut");
    assert!(!restored.contains('\u{1}'));
    assert!(b.sync(SyncMode::Auto).pushed.is_empty(), "restored file counts as in sync");
}

#[test]
fn memory_only_project_and_empty_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut a, mut b) = machines(tmp.path());
    assert!(a.sync(SyncMode::Auto).pushed.is_empty(), "nothing to push yet");
    a.write("memory/MEMORY.md", b"# Only Memory\n");
    assert_eq!(a.sync(SyncMode::Auto).pushed, vec!["memory/MEMORY.md".to_string()]);
    assert_eq!(b.sync(SyncMode::Auto).pulled, vec!["memory/MEMORY.md".to_string()]);
    assert_eq!(std::fs::read(b.file("memory/MEMORY.md")).unwrap(), b"# Only Memory\n");
}

#[test]
fn different_files_from_each_machine_merge() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut a, mut b) = machines(tmp.path());
    let (file1, file2) = (format!("{SID}.jsonl"), format!("{SID}/tool-results/a.txt"));
    a.write(&file1, b"from A\n");
    a.sync(SyncMode::Auto);
    b.write(&file2, b"from B");
    let report = b.sync(SyncMode::Auto);
    assert_eq!((report.pulled, report.pushed), (vec![file1.clone()], vec![file2.clone()]));
    assert_eq!(a.sync(SyncMode::Auto).pulled, vec![file2.clone()]);
    assert_eq!(std::fs::read(a.file(&file2)).unwrap(), b"from B");
}
