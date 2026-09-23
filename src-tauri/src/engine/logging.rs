use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

const MAX_LOG_BYTES: u64 = 1 << 20;

/// Minimal file logger: the release exe has no console, so this is the only trace of failures.
/// Messages must never contain transcript content or tokens.
struct FileLogger {
    path: PathBuf,
    lock: Mutex<()>,
}

impl log::Log for FileLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let _guard = self.lock.lock();
        if std::fs::metadata(&self.path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
            let _ = std::fs::rename(&self.path, self.path.with_extension("log.1"));
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&self.path) {
            let _ = writeln!(f, "{} {} pid={} {}", chrono::Utc::now().to_rfc3339(), record.level(), std::process::id(), record.args());
        }
    }

    fn flush(&self) {}
}

/// Installs the logger and a panic hook for this process (GUI, hook-save or hook-worker).
pub fn init(logs_dir: PathBuf, process: &str) {
    let _ = std::fs::create_dir_all(&logs_dir);
    let logger = FileLogger { path: logs_dir.join("session-relay.log"), lock: Mutex::new(()) };
    if log::set_logger(Box::leak(Box::new(logger))).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
    let process = process.to_string();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("panic in {process}: {info}");
    }));
}
