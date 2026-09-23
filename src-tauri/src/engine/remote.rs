use super::context::Engine;
use super::error::{Error, Result};
use super::manifest::{ChunkRef, FileEntry, Manifest};
use super::project_identity::ProjectKey;

/// Reads straight from git objects of a fetched snapshot; never touches the worktree,
/// so status checks cannot disturb an in-flight save.
pub fn manifest(engine: &Engine, sha: Option<&str>, key: &ProjectKey) -> Result<Option<Manifest>> {
    let Some(sha) = sha else { return Ok(None) };
    let path = engine.manifest_path(&engine.keys.key_hash16(key));
    match engine.repo.show(sha, &path)? {
        Some(sealed) => Manifest::open(&sealed, &engine.keys, Some(key)).map(Some),
        None => Ok(None),
    }
}

pub fn all_manifests(engine: &Engine, sha: Option<&str>) -> Result<Vec<Manifest>> {
    let Some(sha) = sha else { return Ok(vec![]) };
    let mut out = Vec::new();
    for key_hash in engine.repo.list_dirs(sha, "p")? {
        let Some(sealed) = engine.repo.show(sha, &engine.manifest_path(&key_hash))? else { continue };
        match Manifest::open(&sealed, &engine.keys, None) {
            // A manifest filed under a name that is not its own key hash was moved by someone else.
            Ok(m) if engine.keys.key_hash16(&m.key()) == key_hash => out.push(m),
            Ok(_) => log::warn!("manifest {key_hash} is filed under the wrong project"),
            Err(e) => log::warn!("manifest {key_hash} unreadable: {e}"),
        }
    }
    Ok(out)
}

/// Decrypts one chunk and checks it is the content its name claims.
pub fn chunk(engine: &Engine, sha: &str, key_hash: &str, chunk: &ChunkRef) -> Result<Vec<u8>> {
    let sealed = engine
        .repo
        .show(sha, &engine.chunk_path(key_hash, &chunk.name))?
        .ok_or_else(|| Error::Invalid(format!("missing chunk {}", chunk.name)))?;
    let plain = engine.keys.open(&sealed)?;
    if plain.len() as u64 != chunk.len || engine.keys.chunk_name(&plain) != chunk.name {
        return Err(Error::Invalid(format!("corrupt chunk {}", chunk.name)));
    }
    Ok(plain)
}

/// Whole stored (normalized) content; used for backups before a forced overwrite.
pub fn file_content(engine: &Engine, sha: &str, key_hash: &str, entry: &FileEntry) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(entry.size as usize);
    for c in &entry.chunks {
        out.extend(chunk(engine, sha, key_hash, c)?);
    }
    Ok(out)
}
