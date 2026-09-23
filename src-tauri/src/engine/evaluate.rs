use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::base::{BaseEntry, BaseState};
use super::context::Engine;
use super::error::{Error, Result};
use super::file_set::{self, FileClass};
use super::fs_util::mtime_ns;
use super::manifest::Manifest;
use super::normalize::PathRewrite;
use super::project_identity::ProjectKey;
use super::remote;
use super::snapshot::{self, Snapshot};
use super::state::{self, Decision, Facts, FileState};

#[derive(Debug, Clone)]
pub struct FileEval {
    pub rel: String,
    pub class: FileClass,
    pub state: FileState,
    /// Whole file edited on both machines; last-writer-wins chose `state`.
    pub conflict: bool,
    pub local_meta: Option<(u64, i64)>,
    pub local: Option<Snapshot>,
}

#[derive(Debug, Default)]
pub struct Evaluation {
    pub files: Vec<FileEval>,
    /// Files that could not be read this round (error code); left untouched.
    pub unreadable: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    Synced,
    LocalAhead,
    RemoteAhead,
    Both,
    Diverged,
    NotLinked,
    NoRemote,
}

/// Per-file 3-way states for one project. Reads local content only when metadata cannot decide;
/// one unreadable file never blocks the others.
pub fn files(engine: &Engine, sha: Option<&str>, key: &ProjectKey, dir: &Path, manifest: Option<&Manifest>, base: &BaseState) -> Evaluation {
    let rewrite = PathRewrite::new(dir);
    let mut rels: BTreeSet<(String, FileClass)> = file_set::list(dir, key.subpath.is_empty()).into_iter().collect();
    if let Some(m) = manifest {
        rels.extend(m.files.iter().map(|(rel, e)| (rel.clone(), e.class)));
    }
    rels.extend(base.files.keys().filter_map(|rel| Some((rel.clone(), file_set::classify(rel)?))));

    let mut out = Evaluation::default();
    for (rel, class) in rels {
        let path = local_path(dir, &rel);
        let local_meta = std::fs::metadata(&path).ok().filter(|m| m.is_file()).map(|m| (m.len(), mtime_ns(&m)));
        let facts = Facts { class, local_meta, base: base.files.get(&rel), remote: manifest.and_then(|m| m.files.get(&rel)) };
        if local_meta.is_none() && facts.remote.is_none() {
            continue;
        }
        let decided = match state::quick(&facts) {
            Some(s) => Ok((Decision::from(s), None)),
            None => decide_with_content(engine, sha, key, &path, &facts, &rewrite).or_else(|e| match class {
                // A whole file changing under us: one retry, then skip this round.
                FileClass::Whole => decide_with_content(engine, sha, key, &path, &facts, &rewrite),
                FileClass::Append => Err(e),
            }),
        };
        match decided {
            Ok((d, local)) => out.files.push(FileEval { rel, class, state: d.state, conflict: d.conflict, local_meta, local }),
            Err(e) => {
                log::warn!("skipping unreadable file in {}: {}", engine.keys.key_hash16(key), e.code());
                out.unreadable.push((rel, e.code().to_string()));
            }
        }
    }
    out
}

fn decide_with_content(engine: &Engine, sha: Option<&str>, key: &ProjectKey, path: &Path, facts: &Facts, rewrite: &PathRewrite) -> Result<(Decision, Option<Snapshot>)> {
    let mut last_chunk = Vec::new();
    let snap = snapshot::take(path, facts.class, &engine.keys, rewrite, state::probe_for(facts.remote), &mut |_, data| {
        last_chunk = data.to_vec();
        Ok(())
    })?;
    let extends = || -> Result<bool> {
        let (Some(sha), Some(r)) = (sha, facts.remote) else { return Ok(false) };
        let next = r.chunks.get(snap.chunks.len() - 1).ok_or(Error::Invalid("chunk index".into()))?;
        Ok(remote::chunk(engine, sha, &engine.keys.key_hash16(key), next)?.starts_with(&last_chunk))
    };
    let decision = state::with_content(facts, &snap, extends)?;
    Ok((decision, Some(snap)))
}

pub fn aggregate(files: &[FileEval]) -> ProjectStatus {
    let has = |states: &[FileState]| files.iter().any(|f| states.contains(&f.state));
    let push = has(&[FileState::LocalAhead, FileState::LocalOnly]);
    let pull = has(&[FileState::RemoteAhead, FileState::RemoteOnly]);
    match () {
        _ if has(&[FileState::Diverged]) => ProjectStatus::Diverged,
        _ if push && pull => ProjectStatus::Both,
        _ if push => ProjectStatus::LocalAhead,
        _ if pull => ProjectStatus::RemoteAhead,
        _ => ProjectStatus::Synced,
    }
}

pub fn local_path(dir: &Path, rel: &str) -> PathBuf {
    let mut path = dir.to_path_buf();
    rel.split('/').for_each(|p| path.push(p));
    path
}

/// Session id owning a file: `<sid>.jsonl` or `<sid>/...`.
pub fn session_id(rel: &str) -> Option<&str> {
    let first = rel.split('/').next()?;
    let id = first.strip_suffix(".jsonl").unwrap_or(first);
    (id.len() == 36).then_some(id)
}

/// Base entries for files found equal by content, so the next check skips hashing them.
pub fn in_sync_entries(files: &[FileEval], manifest: Option<&Manifest>) -> Vec<(String, BaseEntry)> {
    files
        .iter()
        .filter(|f| f.state == FileState::InSync)
        .filter_map(|f| {
            let (snap, entry) = (f.local.as_ref()?, manifest?.files.get(&f.rel)?);
            Some((f.rel.clone(), BaseEntry { raw_len: snap.file_len, mtime_ns: snap.mtime_ns, size: entry.size, hash: entry.hash.clone() }))
        })
        .collect()
}
