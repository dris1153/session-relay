mod support;

use std::process::Command;
use std::time::Duration;

use session_relay_lib::engine::error::Error;
use session_relay_lib::engine::evaluate::ProjectStatus;
use session_relay_lib::engine::overview;
use session_relay_lib::engine::project_identity::ProjectKey;
use session_relay_lib::engine::remote::ManifestCache;
use session_relay_lib::engine::sync::SyncMode;
use support::*;

#[test]
fn one_batch_read_matches_reading_each_blob() {
    let tmp = tempfile::tempdir().unwrap();
    let url = bare_remote(tmp.path());
    let mut a = Machine::new(tmp.path(), "A", &url, &identity());
    a.link();
    a.write(&format!("{SID}.jsonl"), transcript_line("user", &a.checkout, "hello").as_bytes());
    a.write("memory/MEMORY.md", b"# notes\n");
    a.sync(SyncMode::Auto);

    let repo = &a.engine.repo;
    let sha = repo.last_snapshot().unwrap();
    let key_hash = a.engine.keys.key_hash16(&a.key());
    let paths = vec![a.engine.manifest_path(&key_hash), "session-relay.json".to_string(), "keys/identity.age".to_string()];
    let batch = repo.read_blobs(&sha, &paths).unwrap();
    let single: Vec<Vec<u8>> = paths.iter().map(|p| repo.show(&sha, p).unwrap().unwrap()).collect();
    assert_eq!(batch, single);
    // Paths come from a listing, so an absent one means a damaged clone.
    assert!(matches!(repo.read_blobs(&sha, &["p/nothing/manifest.age".to_string()]), Err(Error::Git { .. })));
}

#[test]
fn fetching_an_emptied_remote_forgets_the_last_snapshot() {
    let tmp = tempfile::tempdir().unwrap();
    let url = bare_remote(tmp.path());
    let a = Machine::new(tmp.path(), "A", &url, &identity());
    a.engine.repo.ensure_clone().unwrap();
    assert!(a.engine.repo.fetch().unwrap().is_some());
    assert!(a.engine.repo.last_snapshot().is_some());

    let reset = Command::new("git").arg("-C").arg(tmp.path().join("remote.git")).args(["update-ref", "-d", "refs/heads/main"]).status().unwrap();
    assert!(reset.success());
    assert_eq!(a.engine.repo.fetch().unwrap(), None);
    assert_eq!(a.engine.repo.last_snapshot(), None);
}

#[test]
fn listing_ignores_stray_entries_and_keeps_project_order() {
    let tmp = tempfile::tempdir().unwrap();
    let url = bare_remote(tmp.path());
    let mut a = Machine::new(tmp.path(), "A", &url, &identity());
    a.link();
    let sub = ProjectKey { remote: REMOTE.into(), subpath: "web".into() };
    a.write(&format!("{SID}.jsonl"), transcript_line("user", &a.checkout, "root").as_bytes());
    let web = a.dir_of("web").join(format!("{SID}.jsonl"));
    std::fs::create_dir_all(web.parent().unwrap()).unwrap();
    std::fs::write(&web, transcript_line("user", &a.checkout.join("web"), "web")).unwrap();
    a.sync(SyncMode::Auto);
    a.sync_key(&sub, SyncMode::Auto, None).unwrap();

    // Someone drops a file next to the project folders.
    let repo = &a.engine.repo;
    let head = repo.last_snapshot().unwrap();
    repo.prepare_worktree(Some(&head), &["/*".to_string()]).unwrap();
    std::fs::write(repo.dir().join("p").join("README"), b"not a project").unwrap();
    repo.push_lease(&repo.snapshot_commit().unwrap(), Some(&head)).unwrap();

    let mut cache = ManifestCache::default();
    let first = overview::list(&a.engine, &mut cache, Duration::ZERO).unwrap();
    let keys: Vec<ProjectKey> = first.projects.iter().map(|p| p.key.clone()).collect();
    assert_eq!(keys, vec![a.key(), sub.clone()], "both projects, in key order, stray file ignored");
    assert!(first.errors.is_empty());

    // A new snapshot must not be answered from the cached manifests of the old one.
    a.append(&format!("{SID}.jsonl"), &transcript_line("assistant", &a.checkout, "more"));
    a.sync(SyncMode::Auto);
    let second = overview::list(&a.engine, &mut cache, Duration::ZERO).unwrap();
    assert_ne!(second.sha, first.sha);
    assert!(second.projects.iter().all(|p| p.status == ProjectStatus::Synced), "{:?}", second.projects.iter().map(|p| p.status).collect::<Vec<_>>());
}
