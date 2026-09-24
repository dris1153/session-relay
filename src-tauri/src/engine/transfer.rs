use std::io::Write;
use std::path::Path;

use super::base::BaseEntry;
use super::context::Engine;
use super::error::{Error, IoContext, Result};
use super::file_set::{FileClass, TEMP_SUFFIX};
use super::fs_util::{mtime_ns, system_time_from_ns, write_atomic};
use super::manifest::FileEntry;
use super::normalize::PathRewrite;
use super::remote;
use super::snapshot::{self, Snapshot};

pub enum Pulled {
    Done(BaseEntry),
    /// Claude wrote to the file while we were downloading; left untouched.
    BeingWritten,
}

/// Downloads a cloud file into the Claude project dir: temp file, verify, re-check the
/// target did not change since it was evaluated, then rename over it.
#[allow(clippy::too_many_arguments)]
pub fn pull(engine: &Engine, sha: &str, key_hash: &str, project_dir: &Path, target: &Path, entry: &FileEntry, rewrite: &PathRewrite, seen: Option<(u64, i64)>) -> Result<Pulled> {
    refuse_links(project_dir, target)?;
    let tmp = target.with_file_name(format!("{}{TEMP_SUFFIX}", target.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()));
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir).at(dir)?;
    }
    let outcome = write_verified(engine, sha, key_hash, &tmp, entry, rewrite).and_then(|()| {
        let current = std::fs::metadata(target).ok().map(|m| (m.len(), mtime_ns(&m)));
        if current != seen {
            return Ok(Pulled::BeingWritten);
        }
        std::fs::rename(&tmp, target).at(target)?;
        // Keep the cloud edit time so Claude's cleanup does not treat old sessions as fresh.
        let edited = entry.modified_at.unwrap_or(entry.saved_at).timestamp_nanos_opt().unwrap_or(0);
        let file = std::fs::OpenOptions::new().write(true).open(target).at(target)?;
        file.set_modified(system_time_from_ns(edited)).at(target)?;
        let meta = file.metadata().at(target)?;
        Ok(Pulled::Done(BaseEntry { raw_len: meta.len(), mtime_ns: mtime_ns(&meta), size: entry.size, hash: entry.hash.clone() }))
    });
    let _ = std::fs::remove_file(&tmp);
    outcome
}

/// A junction or symlink inside the project dir could redirect a restore elsewhere.
pub fn refuse_links(project_dir: &Path, target: &Path) -> Result<()> {
    let rel = target.strip_prefix(project_dir).map_err(|_| Error::Invalid(format!("{} outside project dir", target.display())))?;
    let mut path = project_dir.to_path_buf();
    for part in rel.components() {
        path.push(part);
        if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(Error::Invalid(format!("{} is a link", path.display())));
        }
    }
    Ok(())
}

fn write_verified(engine: &Engine, sha: &str, key_hash: &str, tmp: &Path, entry: &FileEntry, rewrite: &PathRewrite) -> Result<()> {
    let mut out = std::io::BufWriter::new(std::fs::File::create(tmp).at(tmp)?);
    let mut hasher = engine.keys.hasher();
    let mut carry: Vec<u8> = Vec::new();
    for c in &entry.chunks {
        let plain = remote::chunk(engine, sha, key_hash, c)?;
        hasher.update(&plain);
        if entry.class == FileClass::Whole {
            out.write_all(&plain).at(tmp)?;
            continue;
        }
        // Expand per complete line: a hard 32 MiB cut may split a line across chunks.
        carry.extend_from_slice(&plain);
        let split = carry.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
        out.write_all(&rewrite.expand(&carry[..split])).at(tmp)?;
        carry.drain(..split);
    }
    out.write_all(&rewrite.expand(&carry)).at(tmp)?;
    let file = out.into_inner().map_err(|e| Error::Io { path: tmp.to_path_buf(), source: e.into_error() })?;
    file.sync_all().at(tmp)?;
    if hasher.finalize().to_hex().as_str() != entry.hash {
        return Err(Error::Invalid(format!("hash mismatch for {}", tmp.display())));
    }
    Ok(())
}

/// Reads the local file once more and writes every chunk the store does not have yet.
pub fn push(engine: &Engine, key_hash: &str, source: &Path, class: FileClass, rewrite: &PathRewrite) -> Result<Snapshot> {
    let chunk_dir = engine.repo.dir().join(engine.project_path(key_hash)).join("c");
    std::fs::create_dir_all(&chunk_dir).at(&chunk_dir)?;
    snapshot::take(source, class, &engine.keys, rewrite, None, &mut |chunk, data| {
        let path = chunk_dir.join(format!("{}.zst.age", chunk.name));
        // The worktree was reset + cleaned from a committed snapshot, so an existing file is complete.
        if !path.exists() {
            write_atomic(&path, &engine.keys.seal(data)?)?;
        }
        Ok(())
    })
}
