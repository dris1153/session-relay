mod support;

use std::sync::{Arc, Mutex};

use session_relay_lib::engine::context::Engine;
use session_relay_lib::engine::crypto::Keys;
use session_relay_lib::engine::error::Error;
use session_relay_lib::engine::overview;
use session_relay_lib::engine::progress::Progress;
use session_relay_lib::engine::sync::SyncMode;
use support::*;

fn collect(machine: &Machine) -> Arc<Mutex<Vec<Progress>>> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    machine.engine.repo.progress.set(move |p| sink.lock().unwrap().push(p));
    events
}

fn counts(events: &[Progress], restore: bool) -> Vec<(usize, usize)> {
    events
        .iter()
        .filter_map(|p| match (*p, restore) {
            (Progress::Restore { current, total }, true) | (Progress::Save { current, total }, false) => Some((current, total)),
            _ => None,
        })
        .collect()
}

#[test]
// Transfer percents are not asserted: git prints them only for transfers that take a while.
fn file_progress_is_reported() {
    let tmp = tempfile::tempdir().unwrap();
    let (url, id) = (bare_remote(tmp.path()), identity());
    let mut a = Machine::new(tmp.path(), "A", &url, &id);
    let mut b = Machine::new(tmp.path(), "B", &url, &id);
    a.link();
    a.write(&format!("{SID}.jsonl"), transcript_line("user", &a.checkout, "hello").as_bytes());
    a.write(&format!("{SID}/tool-results/out.txt"), b"tool output");
    a.write("memory/MEMORY.md", b"# notes\n");

    let saved = collect(&a);
    a.sync(SyncMode::Auto);
    let saved = saved.lock().unwrap();
    // Throttled: the first file and the last are always reported.
    let steps = counts(&saved, false);
    assert_eq!((steps.first(), steps.last()), (Some(&(1, 3)), Some(&(3, 3))), "{saved:?}");

    b.link();
    let restored = collect(&b);
    b.sync(SyncMode::Auto);
    let restored = restored.lock().unwrap();
    let steps = counts(&restored, true);
    assert_eq!((steps.first(), steps.last()), (Some(&(1, 3)), Some(&(3, 3))), "{restored:?}");
}

#[test]
fn an_unreachable_remote_keeps_the_clone_and_the_last_snapshot() {
    let tmp = tempfile::tempdir().unwrap();
    let (url, id) = (bare_remote(tmp.path()), identity());
    let mut a = Machine::new(tmp.path(), "A", &url, &id);
    assert_eq!(a.engine.repo.last_snapshot(), None);
    a.link();
    a.write(&format!("{SID}.jsonl"), transcript_line("user", &a.checkout, "hello").as_bytes());
    a.sync(SyncMode::Auto);
    // The pushed commit is known without another fetch.
    assert_eq!(a.engine.repo.last_snapshot(), Some(remote_head(tmp.path())));
    let seen = overview::refresh(&a.engine, std::time::Duration::ZERO).unwrap();
    assert_eq!(a.engine.repo.last_snapshot(), seen);

    // Same clone, but GitHub cannot be reached (`.invalid` never resolves).
    let offline = Engine::new(a.engine.cfg.clone(), Keys::parse(&id).unwrap(), "https://github.invalid/me/store.git".into(), None, true);
    assert!(matches!(overview::refresh(&offline, std::time::Duration::ZERO), Err(Error::Network(_))));
    assert!(offline.repo.dir().join(".git").is_dir(), "an offline refresh must not rebuild the clone");
    assert_eq!(offline.repo.last_snapshot(), seen);
}
