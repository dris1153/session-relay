mod support;

use session_relay_lib::engine::error::Error;
use session_relay_lib::engine::evaluate::ProjectStatus;
use session_relay_lib::engine::overview;
use session_relay_lib::engine::state::FileState;
use session_relay_lib::engine::sync::SyncMode;
use support::*;

fn status(m: &mut Machine) -> ProjectStatus {
    let sha = overview::refresh(&m.engine).unwrap();
    let ov = overview::list(&m.engine, sha.as_deref()).unwrap();
    ov.projects.iter().find(|p| p.key.remote == REMOTE).map(|p| p.status).expect("project listed")
}

fn file_state(m: &mut Machine, rel: &str) -> FileState {
    let sha = overview::refresh(&m.engine).unwrap();
    let ov = overview::list(&m.engine, sha.as_deref()).unwrap();
    ov.projects[0].files.iter().find(|f| f.rel == rel).map(|f| f.state).expect("file listed")
}

#[test]
fn two_machines_roundtrip_conflicts_and_guards() {
    let tmp = tempfile::tempdir().unwrap();
    let url = bare_remote(tmp.path());
    let id = identity();
    let mut a = Machine::new(tmp.path(), "A", &url, &id);
    let mut b = Machine::new(tmp.path(), "B", &url, &id);
    let transcript = format!("{SID}.jsonl");
    let tool = format!("{SID}/tool-results/out.txt");
    let meta = format!("{SID}/subagents/agent-a1.meta.json");

    // Machine A: a session referencing its own tool-results path, a meta file without newline, memory.
    let persisted = format!("{{\"type\":\"user\",\"note\":\"saved to {}\\\\{SID}\\\\tool-results\\\\out.txt\"}}\n", a.escaped_dir());
    a.write(&transcript, (transcript_line("user", &a.checkout, "fix the bug") + &persisted).as_bytes());
    a.write(&tool, b"full tool output");
    a.write(&meta, br#"{"agentType":"general"}"#);
    a.write("memory/MEMORY.md", b"# notes\n");
    assert_eq!(status(&mut a), ProjectStatus::LocalAhead);
    let pushed = a.sync(SyncMode::Auto).pushed;
    assert_eq!(pushed.len(), 4);
    assert_eq!(status(&mut a), ProjectStatus::Synced);

    // Unchanged save makes no commit.
    let head = remote_head(tmp.path());
    assert!(a.sync(SyncMode::Auto).pushed.is_empty());
    assert_eq!(remote_head(tmp.path()), head);

    // Machine B: not linked until the user picks its checkout.
    assert_eq!(status(&mut b), ProjectStatus::NotLinked);
    b.link();
    assert_eq!(status(&mut b), ProjectStatus::RemoteAhead);
    assert_eq!(b.sync(SyncMode::Auto).pulled.len(), 4);
    let b_text = std::fs::read_to_string(b.file(&transcript)).unwrap();
    assert!(b_text.contains(&format!("{}\\\\{SID}\\\\tool-results", b.escaped_dir())), "tool-results path points at B");
    assert!(!b_text.contains(&a.escaped_dir()));
    assert_eq!(std::fs::read(b.file(&meta)).unwrap(), br#"{"agentType":"general"}"#);
    assert_eq!(std::fs::read(b.file(&tool)).unwrap(), b"full tool output");
    assert_eq!(status(&mut b), ProjectStatus::Synced);

    // B continues the session; A pulls it.
    b.append(&transcript, &transcript_line("assistant", &b.checkout, "done on B"));
    assert_eq!(b.sync(SyncMode::PushOnly).pushed, vec![transcript.clone()]);
    assert_eq!(status(&mut a), ProjectStatus::RemoteAhead);
    assert_eq!(a.sync(SyncMode::Auto).pulled, vec![transcript.clone()]);
    let a_text = std::fs::read_to_string(a.file(&transcript)).unwrap();
    assert!(a_text.contains("done on B") && a_text.contains(&a.escaped_dir()));

    // Both continue without syncing: diverged, never auto-overwritten.
    a.append(&transcript, &transcript_line("user", &a.checkout, "only on A"));
    b.append(&transcript, &transcript_line("user", &b.checkout, "only on B"));
    a.sync(SyncMode::PushOnly);
    assert_eq!(status(&mut b), ProjectStatus::Diverged);
    let report = b.sync(SyncMode::Auto);
    assert!(report.pushed.is_empty() && report.skipped.iter().any(|(r, why)| *r == transcript && why == "diverged"));
    assert!(b.sync(SyncMode::PushOnly).pushed.is_empty());
    let forced = b.sync(SyncMode::ForceLocal).pushed;
    assert_eq!(forced, vec![transcript.clone()]);
    assert!(b.engine.cfg.backups_dir().exists(), "cloud copy backed up before overwrite");

    // Restore is refused while the session is open in Claude.
    let registry = a.engine.cfg.sessions_registry_dir();
    std::fs::create_dir_all(&registry).unwrap();
    let pid = std::process::id();
    let start = session_relay_lib::engine::live_sessions::process_start_time(pid).unwrap();
    std::fs::write(registry.join("1.json"), serde_json::json!({ "pid": pid, "sessionId": SID, "procStart": start.to_string() }).to_string()).unwrap();
    let skipped = a.sync(SyncMode::ForceRemote).skipped;
    assert!(skipped.iter().any(|(r, why)| *r == transcript && why == "session_open"));
    std::fs::remove_file(registry.join("1.json")).unwrap();
    assert_eq!(a.sync(SyncMode::ForceRemote).pulled, vec![transcript.clone()]);
    assert!(std::fs::read_to_string(a.file(&transcript)).unwrap().contains("only on B"));

    // Claude's cleanup deletes the local file: shown as cloud-only, not resurrected.
    std::fs::remove_file(a.file(&tool)).unwrap();
    assert_eq!(file_state(&mut a, &tool), FileState::LocalDeleted);
    assert!(a.sync(SyncMode::Auto).pulled.is_empty());
    assert_eq!(a.try_sync(SyncMode::Auto, Some(std::slice::from_ref(&tool))).unwrap().pulled, vec![tool.clone()]);

    // Deleted from the cloud on purpose: B keeps its copy and does not re-upload it.
    overview::delete_remote_session(&a.engine, &a.key(), SID).unwrap();
    assert_eq!(file_state(&mut b, &transcript), FileState::RemoteDeleted);
    assert!(b.sync(SyncMode::Auto).pushed.is_empty());

    // A replayed older snapshot is detected.
    let current = remote_head(tmp.path());
    a.write("memory/MEMORY.md", b"# notes v2\n");
    a.sync(SyncMode::Auto);
    let newer = remote_head(tmp.path());
    assert_ne!(current, newer);
    b.sync(SyncMode::Auto);
    std::process::Command::new("git").arg("-C").arg(tmp.path().join("remote.git")).args(["update-ref", "refs/heads/main", &current]).status().unwrap();
    assert!(matches!(b.try_sync(SyncMode::Auto, None), Err(Error::RollbackDetected)));
}
