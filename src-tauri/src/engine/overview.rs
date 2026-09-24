use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use chrono::Utc;
use serde::Serialize;

use super::activity::{self, Activity, Source};
use super::base::BaseState;
use super::context::Engine;
use super::error::{Error, Result};
use super::evaluate::session_id;
use super::links::{DirRole, Links};
use super::lock::SyncLock;
use super::manifest::Manifest;
use super::progress::Progress;
pub use super::project_summary::{FileRow, ProjectSummary};
use super::project_summary::summarize_all;
use super::project_identity::ProjectKey;
use super::remote::{self, ManifestCache};

#[derive(Debug, Default, Clone, Serialize)]
pub struct Overview {
    pub projects: Vec<ProjectSummary>,
    /// Claude project dirs with no GitHub `origin` (shown greyed, never synced).
    pub unmanaged_dirs: Vec<String>,
    /// Dirs of a second checkout/worktree of an already linked repo (never synced).
    pub secondary_dirs: Vec<(String, ProjectKey)>,
    /// Projects whose status could not be computed (key hash, error code).
    pub errors: Vec<(String, String)>,
    /// The snapshot these statuses are against.
    pub sha: Option<String>,
}

/// Takes the lock, telling the window when it has to wait for a save first.
fn lock(engine: &Engine, op: &str, wait: Duration) -> Result<SyncLock> {
    match SyncLock::try_acquire(&engine.cfg.lock_file(), op) {
        Err(Error::Busy) => {
            engine.repo.progress.report(Progress::Waiting);
            SyncLock::acquire_within(&engine.cfg.lock_file(), op, wait)
        }
        other => other,
    }
}

/// Fetches the latest snapshot, waiting up to `wait` for a running save. `Busy` after that:
/// keep the cached overview.
pub fn refresh(engine: &Engine, wait: Duration) -> Result<Option<String>> {
    let _lock = lock(engine, "refresh", wait)?;
    engine.repo.progress.report(Progress::Checking);
    engine.repo.clear_stale_locks();
    engine.with_store(|| {
        engine.repo.ensure_clone()?;
        engine.repo.fetch()
    })
}

/// Links this machine's checkout of `remote` (user picked the folder, or clone finished).
pub fn link_repo(engine: &Engine, remote: &str, root: &Path) -> Result<()> {
    let _lock = SyncLock::acquire_within(&engine.cfg.lock_file(), "link", Duration::from_secs(10))?;
    let mut links = Links::load(&engine.cfg.links_file())?;
    links.link_repo(remote, root);
    links.save(&engine.cfg.links_file())
}

/// Status of every project known locally or in the cloud, against the newest snapshot this
/// clone knows (read under the lock, so a save cannot slip in between).
pub fn list(engine: &Engine, cache: &mut ManifestCache, wait: Duration) -> Result<Overview> {
    let _lock = lock(engine, "list", wait)?;
    let started = std::time::Instant::now();
    let sha = engine.repo.last_snapshot();
    let sha = sha.as_deref();
    engine.check_identity(sha)?;
    let mut links = Links::load(&engine.cfg.links_file())?;
    let mut overview = Overview { sha: sha.map(str::to_owned), ..Default::default() };
    let manifests = match cache.get(engine, sha) {
        // No git error here heals on its own: rebuild now, the next fetch downloads it all again.
        Err(e @ Error::Git { .. }) => {
            log::warn!("rebuilding store clone after a failed read: {e}");
            engine.repo.rebuild()?;
            return Err(e);
        }
        other => other?,
    };
    let mut manifests: BTreeMap<ProjectKey, Manifest> = manifests.into_iter().map(|m| (m.key(), m)).collect();
    let projects_dir = engine.cfg.projects_dir();
    let dirs = std::fs::read_dir(&projects_dir).into_iter().flatten().flatten().filter(|e| e.path().is_dir());
    for enc in dirs.map(|e| e.file_name().to_string_lossy().into_owned()) {
        match links.classify_dir(&projects_dir, &enc) {
            DirRole::Primary(key) => {
                manifests.entry(key.clone()).or_insert_with(|| Manifest::new(&key));
            }
            DirRole::Secondary(key) => overview.secondary_dirs.push((enc, key)),
            DirRole::NoRemote => overview.unmanaged_dirs.push(enc),
        }
    }
    links.save(&engine.cfg.links_file())?;
    let read = started.elapsed();
    for (key, summary) in summarize_all(engine, &links, sha, manifests.into_iter().collect()) {
        match summary {
            Ok(summary) => overview.projects.push(summary),
            Err(e) => overview.errors.push((engine.keys.key_hash16(&key), e.code().to_string())),
        }
    }
    log::info!("overview list: projects={} manifests_and_links_ms={} total_ms={}", overview.projects.len(), read.as_millis(), started.elapsed().as_millis());
    Ok(overview)
}

/// Removes one session (transcript + its subagents/tool-results) from the cloud snapshot.
/// Local copies stay; their base entries keep them from being re-uploaded.
pub fn delete_remote_session(engine: &Engine, key: &ProjectKey, sid: &str) -> Result<()> {
    let _lock = SyncLock::try_acquire(&engine.cfg.lock_file(), "delete")?;
    engine.repo.clear_stale_locks();
    let sha = engine.with_store(|| {
        engine.repo.ensure_clone()?;
        engine.repo.fetch()
    })?;
    let sha = sha.ok_or(Error::Invalid("empty store".into()))?;
    engine.check_identity(Some(&sha))?;
    let mut manifest = remote::manifest(engine, Some(&sha), key)?.ok_or(Error::Invalid("unknown project".into()))?;
    let base_file = engine.base_file(key);
    let mut base = BaseState::load(&base_file)?;
    if manifest.generation < base.generation_seen {
        return Err(Error::RollbackDetected);
    }
    let before = manifest.files.len();
    manifest.files.retain(|rel, _| session_id(rel) != Some(sid));
    if manifest.files.len() == before {
        return Ok(());
    }
    manifest.generation += 1;
    let key_hash = engine.keys.key_hash16(key);
    engine.repo.prepare_worktree(Some(&sha), &super::publish::sparse_paths(engine, &key_hash))?;
    super::publish::commit_and_push(engine, &key_hash, &manifest, Some(&sha))?;
    base.generation_seen = manifest.generation;
    base.save(&base_file)?;
    let entry = Activity { ts: Utc::now(), key_hash, source: Source::Gui, action: "DeleteRemote".into(), result: "ok".into(), pushed: 0, pulled: 0 };
    activity::append(&engine.cfg.activity_file(), &entry);
    Ok(())
}
