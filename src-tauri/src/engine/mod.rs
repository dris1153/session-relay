//! Sync engine: pure Rust, no Tauri. Maps Claude Code project dirs to GitHub repos and
//! syncs them through an encrypted snapshot repo.

pub mod activity;
pub mod backup;
pub mod base;
mod chunker;
pub mod context;
pub mod crypto;
pub mod error;
pub mod evaluate;
pub mod file_set;
pub mod fs_util;
pub mod git_process;
pub mod links;
pub mod live_sessions;
pub mod lock;
pub mod logging;
pub mod maintenance;
pub mod manifest;
pub mod normalize;
pub mod overview;
pub mod path_decode;
pub mod paths;
pub mod project_identity;
mod publish;
pub mod remote;
pub mod snapshot;
pub mod state;
pub mod store_repo;
pub mod sync;
mod sync_policy;
pub mod title;
pub mod transfer;
