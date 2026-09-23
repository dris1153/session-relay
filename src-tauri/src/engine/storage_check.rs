use std::path::Path;

use age::secrecy::ExposeSecret;
use serde::Serialize;

use super::auth;
use super::context::MARKER_FILE;
use super::crypto::Keys;
use super::error::Result;
use super::github_api::{self, User};
use super::key_setup::{self, KeyFiles, StoreKeyState, IDENTITY_FILE};
use super::secrets;
use super::settings::{RepoRef, Settings};

pub const DEFAULT_REPO: &str = "claude-sessions";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageState {
    NoInstallation,
    RepoMissing,
    RepoPublic,
    RepoForeign,
    NeedsNewKey,
    NeedsUnlock,
    Ready,
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageCheck {
    pub state: StorageState,
    pub user: User,
    pub repo: Option<RepoRef>,
    pub install_url: String,
    pub create_repo_url: String,
    /// Set once a private repo of the user was reached; `unlock_key` reuses it.
    #[serde(skip)]
    pub key_files: Option<KeyFiles>,
}

/// Walks the onboarding preconditions in order: app installed → private repo of the user
/// reachable → store key present and unlocked on this machine. REST only: a second machine
/// must not download the whole store before it can even unlock.
pub fn check(app_dir: &Path, settings: &Settings) -> Result<StorageCheck> {
    let token = auth::valid_token(app_dir)?;
    let user = github_api::user(&token)?;
    let slug = github_api::app_slug()?;
    let wanted = settings.repo.as_ref().map_or(DEFAULT_REPO, |r| r.name.as_str()).to_string();
    let mut out = StorageCheck {
        state: StorageState::NoInstallation,
        user: user.clone(),
        repo: None,
        install_url: format!("https://github.com/apps/{slug}/installations/new"),
        create_repo_url: format!("https://github.com/new?name={wanted}&visibility=private"),
        key_files: None,
    };
    if !github_api::installations(&token)?.iter().any(|i| i.app_slug == slug) {
        return Ok(out);
    }
    let Some(repo) = github_api::repo(&token, &user.login, &wanted)?.filter(|r| r.owner.login.eq_ignore_ascii_case(&user.login)) else {
        out.state = StorageState::RepoMissing;
        return Ok(out);
    };
    let repo_ref = RepoRef { owner: repo.owner.login, name: repo.name };
    out.repo = Some(repo_ref.clone());
    if !repo.private {
        out.state = StorageState::RepoPublic;
        return Ok(out);
    }
    let files = remote_key_files(&token, &repo_ref)?;
    out.state = match key_setup::store_key_state(&files) {
        StoreKeyState::Foreign => StorageState::RepoForeign,
        StoreKeyState::NeedsNewKey => StorageState::NeedsNewKey,
        StoreKeyState::NeedsUnlock => match secrets::load_identity()? {
            Some(id) => match Keys::parse(id.expose_secret()) {
                Ok(keys) if key_setup::marker_matches(&files, &keys)? => StorageState::Ready,
                _ => StorageState::NeedsUnlock,
            },
            None => StorageState::NeedsUnlock,
        },
    };
    out.key_files = Some(files);
    Ok(out)
}

fn remote_key_files(token: &str, repo: &RepoRef) -> Result<KeyFiles> {
    let root = github_api::root_names(token, repo)?;
    let read = |entry: &str, path: &str| if root.iter().any(|n| n == entry) { github_api::file(token, repo, path) } else { Ok(None) };
    Ok(KeyFiles { marker: read(MARKER_FILE, MARKER_FILE)?, identity: read("keys", IDENTITY_FILE)?, root })
}
