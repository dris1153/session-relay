use std::fs::{File, OpenOptions};
use std::path::Path;
use std::time::{Duration, Instant};

use super::error::{Error, IoContext, Result};

/// Cross-process lock serializing every operation on the store clone (GUI and hook workers).
/// Released when dropped or when the process dies.
pub struct SyncLock {
    _file: File,
}

impl SyncLock {
    pub fn try_acquire(path: &Path, op: &str) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).at(dir)?;
        }
        let file = OpenOptions::new().create(true).truncate(false).write(true).open(path).at(path)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => return Err(Error::Busy),
            Err(std::fs::TryLockError::Error(e)) => return Err(e).at(path),
        }
        // Holder info lives next to the lock: Windows blocks reads of a locked range.
        let info = serde_json::json!({ "pid": std::process::id(), "op": op, "started_at": chrono::Utc::now() });
        let _ = std::fs::write(path.with_extension("holder.json"), info.to_string());
        Ok(Self { _file: file })
    }

    pub fn acquire_within(path: &Path, op: &str, wait: Duration) -> Result<Self> {
        let deadline = Instant::now() + wait;
        loop {
            match Self::try_acquire(path, op) {
                Err(Error::Busy) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
                other => return other,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_acquire_is_busy_until_release() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sync.lock");
        let first = SyncLock::try_acquire(&path, "a").unwrap();
        assert!(matches!(SyncLock::try_acquire(&path, "b"), Err(Error::Busy)));
        drop(first);
        assert!(SyncLock::try_acquire(&path, "c").is_ok());
    }
}
