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

/// Every project manifest of a snapshot: one listing plus one batch read. Only paths that exist
/// are read, so a stray entry under `p/` is ignored and a missing object is a real error.
pub fn all_manifests(engine: &Engine, sha: Option<&str>) -> Result<Vec<Manifest>> {
    let Some(sha) = sha else { return Ok(vec![]) };
    let paths: Vec<String> = engine.repo.tree_paths(sha, "p")?.into_iter().filter(|p| p.split('/').count() == 3 && p.ends_with("/manifest.age")).collect();
    let mut out = Vec::new();
    for (path, sealed) in paths.iter().zip(engine.repo.read_blobs(sha, &paths)?) {
        let key_hash = path.split('/').nth(1).unwrap_or_default();
        match Manifest::open(&sealed, &engine.keys, None) {
            // A manifest filed under a name that is not its own key hash was moved by someone else.
            Ok(m) if engine.keys.key_hash16(&m.key()) == key_hash => out.push(m),
            Ok(_) => log::warn!("manifest {key_hash} is filed under the wrong project"),
            Err(e) => log::warn!("manifest {key_hash} unreadable: {e}"),
        }
    }
    Ok(out)
}

/// Decrypted manifests of one snapshot. A snapshot never changes, so they stay valid until the
/// sha does (the app drops the cache with the engine, i.e. with the key).
#[derive(Default, Clone)]
pub struct ManifestCache {
    sha: Option<String>,
    manifests: Vec<Manifest>,
}

impl ManifestCache {
    pub fn get(&mut self, engine: &Engine, sha: Option<&str>) -> Result<Vec<Manifest>> {
        if self.sha.as_deref() != sha || sha.is_none() {
            self.manifests = all_manifests(engine, sha)?;
            self.sha = sha.map(str::to_owned);
        }
        Ok(self.manifests.clone())
    }
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
