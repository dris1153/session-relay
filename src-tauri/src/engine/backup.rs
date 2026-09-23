use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::crypto::Keys;
use super::error::Result;
use super::fs_util::write_atomic;

pub const MAX_AGE: Duration = Duration::from_secs(14 * 24 * 3600);
pub const MAX_TOTAL_BYTES: u64 = 2 << 30;

/// Encrypted copy of content about to be overwritten (never plaintext on disk).
pub fn save(backups_dir: &Path, keys: &Keys, key_hash: &str, rel: &str, side: &str, content: &[u8]) -> Result<PathBuf> {
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ").to_string();
    let mut path = backups_dir.join(stamp).join(key_hash).join(side);
    rel.split('/').for_each(|part| path.push(part));
    let path = path.with_file_name(format!("{}.age", path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()));
    write_atomic(&path, &keys.seal(content)?)?;
    Ok(path)
}

/// Drops backups older than `max_age`, then the oldest until the total fits `max_bytes`.
pub fn prune(backups_dir: &Path, max_age: Duration, max_bytes: u64) {
    let Ok(entries) = std::fs::read_dir(backups_dir) else { return };
    let mut runs: Vec<(PathBuf, SystemTime, u64)> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| {
            let modified = e.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
            (e.path(), modified, dir_size(&e.path()))
        })
        .collect();
    runs.sort_by_key(|(_, modified, _)| *modified);
    let mut total: u64 = runs.iter().map(|(_, _, size)| size).sum();
    let now = SystemTime::now();
    for (path, modified, size) in runs {
        let too_old = now.duration_since(modified).unwrap_or_default() > max_age;
        if (too_old || total > max_bytes) && std::fs::remove_dir_all(&path).is_ok() {
            total = total.saturating_sub(size);
        }
    }
}

fn dir_size(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| match e.metadata() {
                    Ok(m) if m.is_dir() => dir_size(&e.path()),
                    Ok(m) => m.len(),
                    Err(_) => 0,
                })
                .sum()
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_encrypted_and_prunes_by_size() {
        let dir = tempfile::tempdir().unwrap();
        let keys = Keys::generate();
        let path = save(dir.path(), &keys, "abcd", "s/tool-results/a.txt", "local", b"secret .env content").unwrap();
        let sealed = std::fs::read(&path).unwrap();
        assert!(!String::from_utf8_lossy(&sealed).contains("secret"));
        assert_eq!(keys.open(&sealed).unwrap(), b"secret .env content");
        prune(dir.path(), MAX_AGE, 0);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
