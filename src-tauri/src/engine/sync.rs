use std::time::Duration;

use serde::Serialize;

use super::activity::{self, Activity, Source};
use super::backup;
use super::base::{BaseEntry, BaseState};
use super::context::Engine;
use super::error::{Error, Result};
use super::evaluate::{self, local_path, session_id, FileEval};
use super::fs_util::system_time_from_ns;
use super::links::Links;
use super::live_sessions::live_session_ids;
use super::lock::SyncLock;
use super::manifest::{FileEntry, Manifest};
use super::normalize::PathRewrite;
use super::publish::{commit_and_push, sparse_paths};
use super::project_identity::ProjectKey;
use super::remote;
use super::state::FileState;
use super::sync_policy::{needs_cloud_backup, needs_local_backup, should_pull, should_push};
use super::transfer::{self, Pulled};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncMode {
    /// Pull what the cloud has newer, push what this machine has newer, skip conflicts.
    Auto,
    /// Hook worker: never overwrites local files (Claude may have them open).
    PushOnly,
    ForceLocal,
    ForceRemote,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct SyncReport {
    pub pushed: Vec<String>,
    pub pulled: Vec<String>,
    pub skipped: Vec<(String, String)>,
}

pub struct Request<'a> {
    pub key: &'a ProjectKey,
    pub mode: SyncMode,
    pub only: Option<&'a [String]>,
    pub source: Source,
    pub lock_wait: Duration,
}

const LEASE_RETRIES: usize = 3;

pub fn sync_project(engine: &Engine, req: &Request) -> Result<SyncReport> {
    let _lock = SyncLock::acquire_within(&engine.cfg.lock_file(), "sync", req.lock_wait)?;
    engine.repo.clear_stale_locks();
    // Reloaded under the lock: the GUI and hook workers each update links.
    let mut links = Links::load(&engine.cfg.links_file())?;
    let mut report = SyncReport::default();
    let mut result = Err(Error::LeaseRejected);
    let mut rebuilt = false;
    for _ in 0..LEASE_RETRIES {
        report.skipped.clear();
        result = sync_once(engine, &mut links, req, &mut report);
        match &result {
            Err(Error::LeaseRejected) => {}
            // The clone is only a cache: a git failure (corrupt objects, broken index) gets one fresh clone.
            Err(Error::Git { .. }) if !rebuilt => {
                rebuilt = true;
                log::warn!("rebuilding store clone after git failure");
                engine.repo.rebuild()?;
            }
            _ => break,
        }
    }
    links.save(&engine.cfg.links_file())?;
    let (outcome, pushed) = match &result {
        Ok(()) => ("ok", report.pushed.len()),
        Err(e) => (e.code(), 0),
    };
    let entry = Activity { ts: chrono::Utc::now(), key_hash: engine.keys.key_hash16(req.key), source: req.source.clone(), action: format!("{:?}", req.mode), result: outcome.into(), pushed, pulled: report.pulled.len() };
    activity::append(&engine.cfg.activity_file(), &entry);
    result.map(|()| report)
}

fn sync_once(engine: &Engine, links: &mut Links, req: &Request, report: &mut SyncReport) -> Result<()> {
    let key = req.key;
    let key_hash = engine.keys.key_hash16(key);
    let enc = links.dir_for(key).ok_or(Error::NotLinked)?;
    let dir = engine.cfg.projects_dir().join(&enc);
    engine.repo.ensure_clone()?;
    let sha = engine.repo.fetch()?;
    engine.check_identity(sha.as_deref())?;
    let manifest = remote::manifest(engine, sha.as_deref(), key)?;
    let base_file = engine.base_file(key);
    let mut base = BaseState::load(&base_file)?;
    let seen = manifest.as_ref().map_or(0, |m| m.generation);
    // A replayed or vanished snapshot must not read as "the cloud is empty", unless the user
    // explicitly chose to overwrite the cloud with this machine's copy.
    if req.mode != SyncMode::ForceLocal && seen < base.generation_seen {
        return Err(Error::RollbackDetected);
    }
    let eval = evaluate::files(engine, sha.as_deref(), key, &dir, manifest.as_ref(), &base);
    report.skipped.extend(eval.unreadable.iter().cloned());
    let files = eval.files;
    let rewrite = PathRewrite::new(&dir);
    let explicit = req.only.is_some();
    let wanted = |rel: &str| req.only.is_none_or(|only| only.iter().any(|r| r == rel));

    let live = live_session_ids(&engine.cfg.sessions_registry_dir());
    for f in files.iter().filter(|f| wanted(&f.rel) && should_pull(req.mode, f, explicit)) {
        let (Some(sha), Some(entry)) = (sha.as_deref(), manifest.as_ref().and_then(|m| m.files.get(&f.rel))) else { continue };
        if session_id(&f.rel).is_some_and(|sid| live.contains(sid)) {
            report.skipped.push((f.rel.clone(), "session_open".into()));
            continue;
        }
        let target = local_path(&dir, &f.rel);
        if needs_local_backup(f) {
            let saved = std::fs::read(&target)
                .map_err(|source| Error::Io { path: target.clone(), source })
                .and_then(|content| backup::save(&engine.cfg.backups_dir(), &engine.keys, &key_hash, &f.rel, "local", &content));
            if saved.is_err() {
                report.skipped.push((f.rel.clone(), "backup_failed".into()));
                continue;
            }
        }
        match transfer::pull(engine, sha, &key_hash, &dir, &target, entry, &rewrite, f.local_meta) {
            Ok(Pulled::Done(b)) => {
                base.files.insert(f.rel.clone(), b);
                report.pulled.push(f.rel.clone());
            }
            Ok(Pulled::BeingWritten) => report.skipped.push((f.rel.clone(), "being_written".into())),
            // One file (sharing violation, corrupt chunk) must not stop the rest of the project.
            Err(e @ Error::Git { .. }) => return Err(e),
            Err(e) => report.skipped.push((f.rel.clone(), e.code().into())),
        }
    }
    // RemoteDeleted keeps its base entry: that is what stops the file from being re-uploaded.
    base.files.retain(|rel, _| local_path(&dir, rel).is_file() || manifest.as_ref().is_some_and(|m| m.files.contains_key(rel)));
    base.files.extend(evaluate::in_sync_entries(&files, manifest.as_ref()));
    base.generation_seen = base.generation_seen.max(seen);
    links.dirs.insert(enc, key.clone());
    base.save(&base_file)?;

    for f in files.iter().filter(|f| wanted(&f.rel) && f.state == FileState::Diverged) {
        if !should_push(req.mode, f) && !should_pull(req.mode, f, explicit) {
            report.skipped.push((f.rel.clone(), "diverged".into()));
        }
    }
    // Re-uploading sessions deleted from the cloud on purpose needs an explicit pick, or a vanished manifest.
    let republish_deleted = req.mode == SyncMode::ForceLocal && (explicit || manifest.is_none());
    let to_push: Vec<&FileEval> = files
        .iter()
        .filter(|f| wanted(&f.rel) && (should_push(req.mode, f) || (republish_deleted && f.state == FileState::RemoteDeleted)))
        .collect();
    if to_push.is_empty() {
        return Ok(());
    }
    engine.repo.prepare_worktree(sha.as_deref(), &sparse_paths(engine, &key_hash))?;
    let mut next = manifest.clone().unwrap_or_else(|| Manifest::new(key));
    let mut new_base: Vec<(String, BaseEntry)> = Vec::new();
    let mut pushed = Vec::new();
    for f in to_push {
        let old = manifest.as_ref().and_then(|m| m.files.get(&f.rel));
        let prepared = (|| -> Result<_> {
            if let (Some(sha), Some(old)) = (sha.as_deref(), old.filter(|_| needs_cloud_backup(f))) {
                let content = remote::file_content(engine, sha, &key_hash, old)?;
                backup::save(&engine.cfg.backups_dir(), &engine.keys, &key_hash, &f.rel, "cloud", &content)?;
            }
            transfer::push(engine, &key_hash, &local_path(&dir, &f.rel), f.class, &rewrite)
        })();
        let snap = match prepared {
            Ok(snap) => snap,
            Err(e @ Error::Git { .. }) => return Err(e),
            Err(e) => {
                report.skipped.push((f.rel.clone(), e.code().into()));
                continue;
            }
        };
        new_base.push((f.rel.clone(), BaseEntry { raw_len: snap.file_len, mtime_ns: snap.mtime_ns, size: snap.size, hash: snap.hash.clone() }));
        if old.is_some_and(|o| o.hash == snap.hash) {
            continue; // Only the mtime moved: nothing to upload.
        }
        let modified_at = chrono::DateTime::<chrono::Utc>::from(system_time_from_ns(snap.mtime_ns));
        let entry = FileEntry { class: f.class, size: snap.size, hash: snap.hash, chunks: snap.chunks, title: snap.title, saved_by: engine.cfg.machine_name.clone(), saved_at: chrono::Utc::now(), modified_at: Some(modified_at) };
        next.files.insert(f.rel.clone(), entry);
        pushed.push(f.rel.clone());
    }
    if !pushed.is_empty() {
        next.generation = next.generation.max(base.generation_seen) + 1;
        commit_and_push(engine, &key_hash, &next, sha.as_deref())?;
        base.generation_seen = next.generation;
    }
    base.files.extend(new_base);
    base.save(&base_file)?;
    report.pushed = pushed;
    Ok(())
}
