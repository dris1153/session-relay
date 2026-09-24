mod support;

use std::time::Duration;

use session_relay_lib::engine::sync::SyncMode;
use session_relay_lib::{hook_worker, pending};
use support::*;

#[test]
fn a_pending_turn_is_pushed_and_its_marker_cleared() {
    let tmp = tempfile::tempdir().unwrap();
    let url = bare_remote(tmp.path());
    let mut a = Machine::new(tmp.path(), "A", &url, &identity());
    a.link();
    let transcript = format!("{SID}.jsonl");
    a.write(&transcript, transcript_line("user", &a.checkout, "first").as_bytes());
    a.sync(SyncMode::Auto);
    let head = remote_head(tmp.path());

    let app = a.engine.cfg.app_dir.clone();
    let enc = a.project_dir().file_name().unwrap().to_string_lossy().into_owned();
    a.append(&transcript, &transcript_line("assistant", &a.checkout, "second"));
    let older = pending::add(&app, &enc, "Stop").unwrap();
    let newer = pending::add(&app, &enc, "Stop").unwrap();
    hook_worker::save_pending(&a.engine, &enc, older, Duration::from_secs(5)).unwrap();
    assert_ne!(remote_head(tmp.path()), head, "the turn reached the cloud");
    assert_eq!(pending::latest(&app, &enc), Some(newer), "a later turn keeps its marker");
}

#[test]
fn a_dir_without_github_remote_just_drops_its_marker() {
    let tmp = tempfile::tempdir().unwrap();
    let url = bare_remote(tmp.path());
    let a = Machine::new(tmp.path(), "A", &url, &identity());
    let app = a.engine.cfg.app_dir.clone();
    let enc = "d--scratch-notes";
    let dir = a.engine.cfg.projects_dir().join(enc);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{SID}.jsonl")), transcript_line("user", &tmp.path().join("nowhere"), "hi")).unwrap();
    let head = remote_head(tmp.path());
    let stamp = pending::add(&app, enc, "SessionEnd").unwrap();
    hook_worker::save_pending(&a.engine, enc, stamp, Duration::from_secs(5)).unwrap();
    assert_eq!(pending::latest(&app, enc), None);
    assert_eq!(remote_head(tmp.path()), head, "nothing pushed");
}
