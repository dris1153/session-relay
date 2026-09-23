use std::io::Write;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::error::{IoContext, Result};

/// Write via a sibling temp file + rename so readers never see a partial file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).at(dir)?;
    }
    let tmp = path.with_file_name(format!(
        "{}{}",
        path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default(),
        super::file_set::TEMP_SUFFIX
    ));
    let mut file = std::fs::File::create(&tmp).at(&tmp)?;
    file.write_all(bytes).at(&tmp)?;
    // Flush before rename, or a power loss can leave a renamed but empty file.
    file.sync_all().at(&tmp)?;
    drop(file);
    std::fs::rename(&tmp, path).at(path)
}

pub fn mtime_ns(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| i64::try_from(d.as_nanos()).unwrap_or(i64::MAX))
}

pub fn system_time_from_ns(ns: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_nanos(u64::try_from(ns).unwrap_or(0))
}
