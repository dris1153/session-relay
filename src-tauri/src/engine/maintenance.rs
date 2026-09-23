use std::path::Path;

use super::backup;
use super::context::Engine;
use super::error::Result;
use super::file_set::TEMP_SUFFIX;
use super::lock::SyncLock;

/// Startup cleanup after crashes or kills: rebuilds a broken store clone, removes our temp
/// files from Claude project dirs and trims backups. Skipped if a save is running.
pub fn on_startup(engine: &Engine) -> Result<()> {
    let _lock = SyncLock::try_acquire(&engine.cfg.lock_file(), "maintenance")?;
    engine.repo.recover()?;
    sweep_temp_files(&engine.cfg.projects_dir(), 0);
    backup::prune(&engine.cfg.backups_dir(), backup::MAX_AGE, backup::MAX_TOTAL_BYTES);
    Ok(())
}

fn sweep_temp_files(dir: &Path, depth: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(t) if t.is_dir() && depth < 3 => sweep_temp_files(&path, depth + 1),
            Ok(t) if t.is_file() && entry.file_name().to_string_lossy().ends_with(TEMP_SUFFIX) => {
                if let Err(e) = std::fs::remove_file(&path) {
                    log::warn!("could not remove leftover temp file: {e}");
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweeps_only_our_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let session = dir.path().join("d--work-app").join("sid").join("subagents");
        std::fs::create_dir_all(&session).unwrap();
        std::fs::write(session.join(format!("agent-a.jsonl{TEMP_SUFFIX}")), b"x").unwrap();
        std::fs::write(session.join("agent-a.jsonl"), b"keep").unwrap();
        sweep_temp_files(dir.path(), 0);
        let left: Vec<_> = std::fs::read_dir(&session).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(left, vec![std::ffi::OsString::from("agent-a.jsonl")]);
    }
}
