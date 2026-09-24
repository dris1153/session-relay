use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};

use chrono::{DateTime, Utc};
use serde::Serialize;

use super::base::BaseState;
use super::context::Engine;
use super::error::Result;
use super::evaluate::{self, ProjectStatus};
use super::links::Links;
use super::manifest::{FileEntry, Manifest};
use super::progress::Progress;
use super::project_identity::ProjectKey;
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
    /// This machine's copy, for the conflict dialog.
    pub local_size: Option<u64>,
    pub local_modified: Option<DateTime<Utc>>,
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

/// Projects are independent: up to 4 at a time, since a project without a sync base hashes all
/// its files (a new machine, after "clear local data"). Results keep the input order.
pub fn summarize_all(engine: &Engine, links: &Links, sha: Option<&str>, projects: Vec<(ProjectKey, Manifest)>) -> Vec<(ProjectKey, Result<ProjectSummary>)> {
    let total = projects.len();
    let (next, done) = (AtomicUsize::new(0), AtomicUsize::new(0));
    let results = Mutex::new(Vec::with_capacity(total));
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get()).min(4);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some((key, manifest)) = projects.get(i) else { break };
                let remote = (!manifest.files.is_empty() || manifest.generation > 0).then_some(manifest);
                let summary = summarize(engine, links, sha, key, remote);
                engine.repo.progress.report(Progress::Evaluate { current: done.fetch_add(1, Ordering::Relaxed) + 1, total });
                results.lock().unwrap_or_else(PoisonError::into_inner).push((i, key.clone(), summary));
            });
        }
    });
    let mut results = results.into_inner().unwrap_or_else(PoisonError::into_inner);
    results.sort_by_key(|(i, _, _)| *i);
    results.into_iter().map(|(_, key, summary)| (key, summary)).collect()
}

fn summarize(engine: &Engine, links: &Links, sha: Option<&str>, key: &ProjectKey, manifest: Option<&Manifest>) -> Result<ProjectSummary> {
    let key_hash = engine.keys.key_hash16(key);
    let Some(enc) = links.dir_for(key) else {
        let files = manifest.map(|m| m.files.iter().map(|(rel, e)| row(rel, FileState::RemoteOnly, false, Some(e), None)).collect()).unwrap_or_default();
        return Ok(ProjectSummary { key_hash, key: key.clone(), status: ProjectStatus::NotLinked, local_dir: None, files, unreadable: vec![] });
    };
    let dir = engine.cfg.projects_dir().join(&enc);
    let base = BaseState::load(&engine.base_file(key))?;
    let eval = evaluate::files(engine, sha, key, &dir, manifest, &base);
    let files = eval.files.iter().map(|f| row(&f.rel, f.state, f.conflict, manifest.and_then(|m| m.files.get(&f.rel)), f.local_meta)).collect();
    let status = evaluate::aggregate(&eval.files);
    Ok(ProjectSummary { key_hash, key: key.clone(), status, local_dir: Some(enc), files, unreadable: eval.unreadable })
}

fn row(rel: &str, state: FileState, conflict: bool, entry: Option<&FileEntry>, local: Option<(u64, i64)>) -> FileRow {
    FileRow {
        local_size: local.map(|(len, _)| len),
        local_modified: local.map(|(_, mtime_ns)| DateTime::from_timestamp_nanos(mtime_ns)),
        rel: rel.to_string(),
        state,
        conflict,
        title: entry.and_then(|e| e.title.clone()),
        size: entry.map(|e| e.size),
        saved_by: entry.map(|e| e.saved_by.clone()),
        saved_at: entry.map(|e| e.saved_at),
    }
}
