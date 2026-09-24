use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;

use super::activity::{self, Activity, Source};
use super::base::BaseState;
use super::context::Engine;
use super::error::{Error, Result};
use super::evaluate::{self, session_id, ProjectStatus};
use super::links::{DirRole, Links};
use super::lock::SyncLock;
use super::manifest::{FileEntry, Manifest};
use super::project_identity::ProjectKey;
use super::remote;
use super::state::FileState;

#[derive(Debug, Clone, Serialize)]
pub struct FileRow {
    pub rel: String,
    pub state: FileState,
    pub conflict: bool,
    pub title: Option<String>,
    pub size: Option<u64>,
    pub saved_by: Option<String>,
    pub saved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectSummary {
    pub key_hash: String,
    pub key: ProjectKey,
    pub status: ProjectStatus,
    pub local_dir: Option<String>,
    pub files: Vec<FileRow>,
    /// Files skipped this round (rel, error code).
    pub unreadable: Vec<(String, String)>,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct Overview {
    pub projects: Vec<ProjectSummary>,
    /// Claude project dirs with no GitHub `origin` (shown greyed, never synced).
    pub unmanaged_dirs: Vec<String>,
    /// Dirs of a second checkout/worktree of an already linked repo (never synced).
    pub secondary_dirs: Vec<(String, ProjectKey)>,
    /// Projects whose status could not be computed (key hash, error code).
    pub errors: Vec<(String, String)>,
}

/// Fetches the latest snapshot, waiting up to `wait` for a running save. `Busy` after that:
/// keep the cached overview.
pub fn refresh(engine: &Engine, wait: Duration) -> Result<Option<String>> {
    let _lock = SyncLock::acquire_within(&engine.cfg.lock_file(), "refresh", wait)?;
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

/// Status of every project known locally or in the cloud, against snapshot `sha`.
pub fn list(engine: &Engine, sha: Option<&str>) -> Result<Overview> {
    let _lock = SyncLock::try_acquire(&engine.cfg.lock_file(), "list")?;
    engine.check_identity(sha)?;
    let mut links = Links::load(&engine.cfg.links_file())?;
    let mut overview = Overview::default();
    let mut manifests: BTreeMap<ProjectKey, Manifest> = remote::all_manifests(engine, sha)?.into_iter().map(|m| (m.key(), m)).collect();
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
    for (key, manifest) in manifests {
        let remote_manifest = (!manifest.files.is_empty() || manifest.generation > 0).then_some(&manifest);
        match summarize(engine, &links, sha, &key, remote_manifest) {
            Ok(summary) => overview.projects.push(summary),
            Err(e) => overview.errors.push((engine.keys.key_hash16(&key), e.code().to_string())),
        }
    }
    Ok(overview)
}

fn summarize(engine: &Engine, links: &Links, sha: Option<&str>, key: &ProjectKey, manifest: Option<&Manifest>) -> Result<ProjectSummary> {
    let key_hash = engine.keys.key_hash16(key);
    let Some(enc) = links.dir_for(key) else {
        let files = manifest.map(|m| m.files.iter().map(|(rel, e)| row(rel, FileState::RemoteOnly, false, Some(e))).collect()).unwrap_or_default();
        return Ok(ProjectSummary { key_hash, key: key.clone(), status: ProjectStatus::NotLinked, local_dir: None, files, unreadable: vec![] });
    };
    let dir = engine.cfg.projects_dir().join(&enc);
    let base = BaseState::load(&engine.base_file(key))?;
    let eval = evaluate::files(engine, sha, key, &dir, manifest, &base);
    let files = eval.files.iter().map(|f| row(&f.rel, f.state, f.conflict, manifest.and_then(|m| m.files.get(&f.rel)))).collect();
    let status = evaluate::aggregate(&eval.files);
    Ok(ProjectSummary { key_hash, key: key.clone(), status, local_dir: Some(enc), files, unreadable: eval.unreadable })
}

fn row(rel: &str, state: FileState, conflict: bool, entry: Option<&FileEntry>) -> FileRow {
    FileRow {
        rel: rel.to_string(),
        state,
        conflict,
        title: entry.and_then(|e| e.title.clone()),
        size: entry.map(|e| e.size),
        saved_by: entry.map(|e| e.saved_by.clone()),
        saved_at: entry.map(|e| e.saved_at),
    }
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
