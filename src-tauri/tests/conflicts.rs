mod support;

use std::path::Path;

use session_relay_lib::engine::error::Error;
use session_relay_lib::engine::sync::SyncMode;
use support::*;

const MEMORY: &str = "memory/MEMORY.md";

fn pair(root: &Path) -> (Machine, Machine) {
    let (url, id) = (bare_remote(root), identity());
    let mut a = Machine::new(root, "A", &url, &id);
    let mut b = Machine::new(root, "B", &url, &id);
    a.link();
    b.link();
    (a, b)
}

/// Decrypted contents of every backup this machine wrote.
fn backups(m: &Machine) -> Vec<Vec<u8>> {
    fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            if e.path().is_dir() {
                walk(&e.path(), out);
            } else if e.path().extension().is_some_and(|x| x == "age") {
                out.push(e.path());
            }
        }
    }
    let mut files = Vec::new();
    walk(&m.engine.cfg.backups_dir(), &mut files);
    files.iter().map(|p| m.engine.keys.open(&std::fs::read(p).unwrap()).unwrap()).collect()
}

#[test]
fn memory_edited_on_both_machines_newer_wins_older_is_backed_up() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut a, mut b) = pair(tmp.path());
    a.write(MEMORY, b"v1\n");
    a.sync(SyncMode::Auto);
    b.sync(SyncMode::Auto);

    a.write(MEMORY, b"edited on A\n");
    a.touch(MEMORY, 1_900_000_000);
    b.write(MEMORY, b"edited on B later\n");
    b.touch(MEMORY, 1_900_000_100);
    assert_eq!(a.sync(SyncMode::Auto).pushed, vec![MEMORY.to_string()]);

    // The hook never settles a two-sided edit.
    assert!(b.sync(SyncMode::PushOnly).pushed.is_empty());
    // The GUI does: B edited later, so B wins and A's edit is kept as an encrypted backup.
    assert_eq!(b.sync(SyncMode::Auto).pushed, vec![MEMORY.to_string()]);
    assert!(backups(&b).contains(&b"edited on A\n".to_vec()), "cloud copy backed up before overwrite");
    assert_eq!(a.sync(SyncMode::Auto).pulled, vec![MEMORY.to_string()]);
    assert_eq!(std::fs::read(a.file(MEMORY)).unwrap(), b"edited on B later\n");
}

#[test]
fn older_local_edit_is_backed_up_before_pulling_newer_cloud_edit() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut a, mut b) = pair(tmp.path());
    a.write(MEMORY, b"v1\n");
    a.sync(SyncMode::Auto);
    b.sync(SyncMode::Auto);

    a.write(MEMORY, b"newer on A\n");
    a.touch(MEMORY, 1_900_000_200);
    a.sync(SyncMode::Auto);
    b.write(MEMORY, b"older on B\n");
    b.touch(MEMORY, 1_900_000_100);
    assert_eq!(b.sync(SyncMode::Auto).pulled, vec![MEMORY.to_string()]);
    assert_eq!(std::fs::read(b.file(MEMORY)).unwrap(), b"newer on A\n");
    assert!(backups(&b).contains(&b"older on B\n".to_vec()), "local edit backed up before overwrite");
}

#[test]
fn touching_memory_without_editing_does_not_overwrite_cloud() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut a, mut b) = pair(tmp.path());
    a.write(MEMORY, b"v1\n");
    a.sync(SyncMode::Auto);
    b.sync(SyncMode::Auto);

    b.write(MEMORY, b"v2 from B\n");
    b.sync(SyncMode::Auto);
    a.touch(MEMORY, 2_000_000_000); // content still v1, newest mtime anywhere
    let report = a.sync(SyncMode::Auto);
    assert!(report.pushed.is_empty(), "stale content must not be pushed");
    assert_eq!(report.pulled, vec![MEMORY.to_string()]);
    assert_eq!(std::fs::read(a.file(MEMORY)).unwrap(), b"v2 from B\n");
}

#[test]
fn force_push_elsewhere_never_wipes_lines_here() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut a, mut b) = pair(tmp.path());
    let transcript = format!("{SID}.jsonl");
    a.write(&transcript, transcript_line("user", &a.checkout, "shared").as_bytes());
    a.sync(SyncMode::Auto);
    b.sync(SyncMode::Auto);

    a.append(&transcript, &transcript_line("user", &a.checkout, "only on A"));
    a.sync(SyncMode::PushOnly);
    b.append(&transcript, &transcript_line("user", &b.checkout, "only on B"));
    assert_eq!(b.sync(SyncMode::ForceLocal).pushed, vec![transcript.clone()]);

    // A's copy equals what it last saved, but the cloud no longer extends it: a conflict, not a pull.
    let report = a.sync(SyncMode::Auto);
    assert!(report.pulled.is_empty());
    assert!(report.skipped.contains(&(transcript.clone(), "diverged".to_string())));
    assert!(std::fs::read_to_string(a.file(&transcript)).unwrap().contains("only on A"));
}

#[test]
fn literal_placeholder_text_and_sibling_paths_survive_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut a, mut b) = pair(tmp.path());
    let transcript = format!("{SID}.jsonl");
    let sibling = format!("{}-v2\\\\x.txt", a.escaped_dir());
    let text = format!("{{\"type\":\"user\",\"t\":\"the {{{{SR_PROJECT_DIR}}}} token\",\"p\":\"{sibling}\"}}\n");
    a.write(&transcript, text.as_bytes());
    a.sync(SyncMode::Auto);
    b.sync(SyncMode::Auto);
    let restored = std::fs::read_to_string(b.file(&transcript)).unwrap();
    assert!(restored.contains("the {{SR_PROJECT_DIR}} token"));
    assert!(restored.contains(&sibling), "sibling dir path is not rewritten");
}

#[test]
fn wrong_identity_is_refused_before_writing_anything() {
    let tmp = tempfile::tempdir().unwrap();
    let url = bare_remote(tmp.path());
    let mut a = Machine::new(tmp.path(), "A", &url, &identity());
    let mut b = Machine::new(tmp.path(), "B", &url, &identity());
    a.link();
    b.link();
    let transcript = format!("{SID}.jsonl");
    a.write(&transcript, b"test content\n");
    a.sync(SyncMode::Auto);

    // Keyed names hide A's projects from B; the store marker stops B from pushing a parallel set.
    b.write(&transcript, b"b content\n");
    let head = remote_head(tmp.path());
    let result = b.try_sync(SyncMode::Auto, None);
    assert!(matches!(result, Err(Error::WrongIdentity)), "got: {result:?}");
    assert_eq!(remote_head(tmp.path()), head, "nothing pushed");
}
