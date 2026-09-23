use serde::Serialize;

use super::base::BaseEntry;
use super::error::Result;
use super::file_set::FileClass;
use super::manifest::FileEntry;
use super::snapshot::Snapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileState {
    InSync,
    LocalOnly,
    RemoteOnly,
    LocalAhead,
    RemoteAhead,
    Diverged,
    /// Claude deleted it locally (cleanupPeriodDays) and the cloud copy is unchanged: do not resurrect.
    LocalDeleted,
    /// Removed from the cloud on purpose and unchanged here since: do not re-upload.
    RemoteDeleted,
}

/// A state plus whether both sides changed a whole file (last-writer-wins picked a side,
/// so the losing side must be backed up and the hook must leave it alone).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decision {
    pub state: FileState,
    pub conflict: bool,
}

impl From<FileState> for Decision {
    fn from(state: FileState) -> Self {
        Self { state, conflict: false }
    }
}

pub struct Facts<'a> {
    pub class: FileClass,
    /// Current local length and mtime, if the file exists.
    pub local_meta: Option<(u64, i64)>,
    pub base: Option<&'a BaseEntry>,
    pub remote: Option<&'a FileEntry>,
}

/// Decides without reading content when the 3-way facts allow it; None means "read the file".
pub fn quick(f: &Facts) -> Option<FileState> {
    let unchanged = match (f.local_meta, f.base) {
        (Some((len, mtime)), Some(b)) => len == b.raw_len && mtime == b.mtime_ns,
        _ => false,
    };
    let remote_same_as_base = matches!((f.remote, f.base), (Some(r), Some(b)) if r.hash == b.hash);
    Some(match (f.local_meta.is_some(), f.base.is_some(), f.remote.is_some()) {
        (false, true, true) if remote_same_as_base => FileState::LocalDeleted,
        (false, true, true) => FileState::RemoteAhead,
        (false, false, true) => FileState::RemoteOnly,
        (true, false, false) => FileState::LocalOnly,
        (true, true, false) if unchanged => FileState::RemoteDeleted,
        (true, true, false) => FileState::LocalAhead,
        (true, true, true) if unchanged && remote_same_as_base => FileState::InSync,
        // A transcript is only replaced after proving the cloud copy extends it (a force-push
        // from another machine may have dropped lines this machine still has).
        (true, true, true) if unchanged && f.class == FileClass::Whole => FileState::RemoteAhead,
        (true, true, true) if remote_same_as_base => FileState::LocalAhead,
        _ => return None,
    })
}

/// Probe range to request from `snapshot::take` so `with_content` can test "cloud ⊆ local".
pub fn probe_for(remote: Option<&FileEntry>) -> Option<(u64, u64)> {
    let last = remote?.chunks.last()?;
    Some((remote?.size - last.len, last.len))
}

/// Full decision once the local content is known. `remote_extends_local` decrypts at most
/// one cloud chunk to confirm the local tail is a prefix of it.
pub fn with_content(f: &Facts, local: &Snapshot, remote_extends_local: impl FnOnce() -> Result<bool>) -> Result<Decision> {
    let Some(remote) = f.remote else { return Ok(FileState::LocalOnly.into()) };
    if local.hash == remote.hash {
        return Ok(FileState::InSync.into());
    }
    if f.class == FileClass::Whole {
        return Ok(match f.base.map(|b| b.hash.as_str()) {
            Some(h) if h == local.hash => FileState::RemoteAhead.into(),
            Some(h) if h == remote.hash => FileState::LocalAhead.into(),
            _ => {
                let remote_ns = remote.modified_at.unwrap_or(remote.saved_at).timestamp_nanos_opt().unwrap_or(i64::MAX);
                let state = if local.mtime_ns > remote_ns { FileState::LocalAhead } else { FileState::RemoteAhead };
                Decision { state, conflict: true }
            }
        });
    }
    let (n, m) = (remote.chunks.len(), local.chunks.len());
    let cloud_in_local = n == 0
        || (remote.size <= local.size
            && m >= n
            && local.chunks[..n - 1] == remote.chunks[..n - 1]
            && local.probe_name.as_deref() == Some(remote.chunks[n - 1].name.as_str()));
    if cloud_in_local {
        return Ok(FileState::LocalAhead.into());
    }
    let local_in_cloud = local.size < remote.size && n >= m && (m == 0 || local.chunks[..m - 1] == remote.chunks[..m - 1]);
    if local_in_cloud && (m == 0 || remote_extends_local()?) {
        return Ok(FileState::RemoteAhead.into());
    }
    Ok(FileState::Diverged.into())
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
