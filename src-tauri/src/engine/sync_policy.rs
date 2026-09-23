use super::evaluate::FileEval;
use super::state::FileState;
use super::sync::SyncMode;

/// `explicit`: the user picked these files (conflict dialog, "restore this session").
pub(super) fn should_pull(mode: SyncMode, f: &FileEval, explicit: bool) -> bool {
    use FileState::*;
    let deleted_here = explicit && f.state == LocalDeleted;
    match mode {
        SyncMode::Auto => matches!(f.state, RemoteAhead | RemoteOnly) || deleted_here,
        // "Overwrite local" must not resurrect every session Claude cleaned up.
        SyncMode::ForceRemote => matches!(f.state, RemoteAhead | RemoteOnly | Diverged | LocalAhead) || deleted_here,
        SyncMode::PushOnly | SyncMode::ForceLocal => false,
    }
}

pub(super) fn should_push(mode: SyncMode, f: &FileEval) -> bool {
    use FileState::*;
    match mode {
        SyncMode::Auto => matches!(f.state, LocalAhead | LocalOnly),
        // The hook never settles a two-sided whole-file edit; the GUI does, with backups.
        SyncMode::PushOnly => matches!(f.state, LocalAhead | LocalOnly) && !f.conflict,
        SyncMode::ForceLocal => matches!(f.state, LocalAhead | LocalOnly | Diverged | RemoteAhead),
        SyncMode::ForceRemote => false,
    }
}

/// Overwriting local content that is not simply an older prefix of the cloud copy.
pub(super) fn needs_local_backup(f: &FileEval) -> bool {
    f.local_meta.is_some() && (f.conflict || matches!(f.state, FileState::Diverged | FileState::LocalAhead))
}

/// Overwriting cloud content this machine never had.
pub(super) fn needs_cloud_backup(f: &FileEval) -> bool {
    f.conflict || matches!(f.state, FileState::Diverged | FileState::RemoteAhead)
}
