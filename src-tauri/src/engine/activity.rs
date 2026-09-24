use std::io::Write;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Gui,
    Hook,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Activity {
    pub ts: DateTime<Utc>,
    pub key_hash: String,
    pub source: Source,
    pub action: String,
    /// Error code (see `Error::code`) or "ok". Never file content or tokens.
    pub result: String,
    #[serde(default)]
    pub pushed: usize,
    #[serde(default)]
    pub pulled: usize,
}

const MAX_BYTES: u64 = 1 << 20;
const KEEP_LINES: usize = 2000;

pub fn append(path: &Path, entry: &Activity) {
    if std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let lines: Vec<&str> = text.lines().collect();
        let kept = lines[lines.len().saturating_sub(KEEP_LINES)..].join("\n") + "\n";
        let _ = super::fs_util::write_atomic(path, kept.as_bytes());
    }
    let line = serde_json::to_string(entry).expect("serializable");
    let written = std::fs::OpenOptions::new().create(true).append(true).open(path).and_then(|mut f| writeln!(f, "{line}"));
    if let Err(e) = written {
        log::warn!("activity log write failed: {e}");
    }
}

/// Most recent entries first, optionally for one project.
pub fn recent(path: &Path, key_hash: Option<&str>, limit: usize) -> Vec<Activity> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    text.lines()
        .rev()
        .filter_map(|l| serde_json::from_str::<Activity>(l).ok())
        .filter(|a| key_hash.is_none_or(|k| a.key_hash == k))
        .take(limit)
        .collect()
}

/// Projects whose latest activity is a failed auto-save, with the error code: the user has to
/// know. Any later success (a manual save too) clears it.
pub fn failed_auto_saves(path: &Path) -> Vec<(String, String)> {
    let mut latest = std::collections::BTreeMap::new();
    for entry in recent(path, None, KEEP_LINES) {
        latest.entry(entry.key_hash.clone()).or_insert(entry);
    }
    latest.into_iter().filter(|(_, a)| a.source == Source::Hook && a.result != "ok").map(|(key_hash, a)| (key_hash, a.result)).collect()
}

