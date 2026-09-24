use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

use super::backup;
use super::context::Engine;
use super::error::{IoContext, Result};
use super::file_set::TEMP_SUFFIX;
use super::lock::SyncLock;

/// Startup cleanup after crashes or kills: rebuilds a broken store clone, removes our temp
/// files from Claude project dirs and trims backups. Waits for a running save first.
pub fn on_startup(engine: &Engine) -> Result<()> {
    let started = Instant::now();
    let _lock = SyncLock::acquire_within(&engine.cfg.lock_file(), "maintenance", Duration::from_secs(120))?;
    let waited = started.elapsed();
    engine.repo.recover()?;
    let recovered = started.elapsed();
    sweep_temp_files(&engine.cfg.projects_dir(), 0);
    backup::prune(&engine.cfg.backups_dir(), backup::MAX_AGE, backup::MAX_TOTAL_BYTES);
    log::info!("startup maintenance: wait_ms={} recover_ms={} total_ms={}", waited.as_millis(), (recovered - waited).as_millis(), started.elapsed().as_millis());
    Ok(())
}

const GC_EVERY: Duration = Duration::from_secs(7 * 24 * 3600);

/// Drops the objects of replaced snapshots, at most weekly. Slow on a big store, so the watcher
/// calls it when idle; `Busy` while anything else holds the lock.
pub fn gc_if_due(engine: &Engine) -> Result<()> {
    let stamp = engine.repo.dir().join(".git").join("session-relay-last-gc");
    let due = std::fs::metadata(&stamp).and_then(|m| m.modified()).map_or(true, |t| SystemTime::now().duration_since(t).unwrap_or_default() > GC_EVERY);
    if !due || !engine.repo.dir().join(".git").exists() {
        return Ok(());
    }
    let _lock = SyncLock::try_acquire(&engine.cfg.lock_file(), "gc")?;
    engine.repo.clear_stale_locks();
    let started = Instant::now();
    let _ = engine.repo.run_git(&["reflog", "expire", "--expire=now", "--all"], Duration::from_secs(60));
    // Encrypted chunks never delta-compress: skip the search (1.5 s for the 391 MB test store).
    let gc = engine.repo.run_git(&["-c", "pack.window=0", "gc", "-q", "--prune=now"], Duration::from_secs(600))?;
    if gc.status_ok {
        std::fs::write(&stamp, b"").at(&stamp)?;
    }
    log::info!("weekly gc: ok={} total_ms={}", gc.status_ok, started.elapsed().as_millis());
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
