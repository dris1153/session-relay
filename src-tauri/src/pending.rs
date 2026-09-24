//! Durable "this Claude project has unsaved work" markers. The hook adds one per Claude turn;
//! a save removes the ones it covered. One file per turn (`pending/<enc>/<stamp>`) so removing
//! old markers can never delete a newer one written meanwhile.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::engine::error::{IoContext, Result};
use crate::engine::fs_util::write_atomic;

fn dir(app_dir: &Path, enc: &str) -> PathBuf {
    app_dir.join("pending").join(enc)
}

/// Claude project dir names use `[A-Za-z0-9-]` only; anything else never reaches the disk.
pub fn valid_enc(enc: &str) -> bool {
    !enc.is_empty() && enc.len() <= 255 && enc.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn now_stamp() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos()
}

pub fn add(app_dir: &Path, enc: &str, event: &str) -> Result<u128> {
    let stamp = now_stamp();
    write_atomic(&dir(app_dir, enc).join(stamp.to_string()), event.as_bytes())?;
    Ok(stamp)
}

fn stamps(app_dir: &Path, enc: &str) -> Vec<u128> {
    let entries = std::fs::read_dir(dir(app_dir, enc)).into_iter().flatten().flatten();
    entries.filter_map(|e| e.file_name().to_str()?.parse().ok()).collect()
}

pub fn latest(app_dir: &Path, enc: &str) -> Option<u128> {
    stamps(app_dir, enc).into_iter().max()
}

pub fn oldest(app_dir: &Path, enc: &str) -> Option<u128> {
    stamps(app_dir, enc).into_iter().min()
}

/// How long ago `stamp` was written (zero for a stamp from the future).
pub fn age(stamp: u128) -> Duration {
    Duration::from_nanos(now_stamp().saturating_sub(stamp).min(u64::MAX as u128) as u64)
}

/// Removes the markers a save started at `stamp` covered; newer ones stay for the next save.
pub fn clear_up_to(app_dir: &Path, enc: &str, stamp: u128) -> Result<()> {
    let dir = dir(app_dir, enc);
    for old in stamps(app_dir, enc).into_iter().filter(|s| *s <= stamp) {
        let path = dir.join(old.to_string());
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e).at(&path),
            _ => {}
        }
    }
    // The folder stays: removing it could race a hook creating its next marker in it.
    Ok(())
}

/// Projects whose newest marker is older than `older_than` (their worker died or gave up), with it.
/// A marker from the future (the clock was set back) counts as stale too, or it would block
/// every later save of that project.
pub fn stale(app_dir: &Path, older_than: Duration) -> Vec<(String, u128)> {
    let now = now_stamp();
    let (cutoff, future) = (now.saturating_sub(older_than.as_nanos()), now + Duration::from_secs(60).as_nanos());
    let entries = std::fs::read_dir(app_dir.join("pending")).into_iter().flatten().flatten();
    let encs = entries.filter_map(|e| e.file_name().to_str().map(str::to_owned)).filter(|enc| valid_enc(enc));
    encs.filter_map(|enc| latest(app_dir, &enc).filter(|s| *s <= cutoff || *s > future).map(|s| (enc, s))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clearing_keeps_markers_written_after_the_save_started() {
        let app = tempfile::tempdir().unwrap();
        let first = add(app.path(), "d--work-app", "Stop").unwrap();
        let second = add(app.path(), "d--work-app", "Stop").unwrap();
        assert_eq!(latest(app.path(), "d--work-app"), Some(second));
        clear_up_to(app.path(), "d--work-app", first).unwrap();
        assert_eq!(latest(app.path(), "d--work-app"), Some(second));
        assert_eq!(stale(app.path(), Duration::ZERO), vec![("d--work-app".to_string(), second)]);
        assert!(stale(app.path(), Duration::from_secs(3600)).is_empty());
        clear_up_to(app.path(), "d--work-app", second).unwrap();
        assert_eq!(latest(app.path(), "d--work-app"), None);
        assert!(!valid_enc("..") && !valid_enc("a/b") && !valid_enc(""));
    }
}
