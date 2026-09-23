use std::path::Path;
use std::time::Duration;

use super::error::{Error, Result};
use super::github_api;
use super::lock::SyncLock;
use super::secrets::{self, Tokens};

const REFRESH_MARGIN_MINUTES: i64 = 5;

fn expiring(tokens: &Tokens) -> bool {
    tokens.expires_at.is_some_and(|at| at - chrono::Utc::now() < chrono::Duration::minutes(REFRESH_MARGIN_MINUTES))
}

/// Every token write goes through this lock, so an in-flight refresh cannot write old tokens
/// back over a sign-out or a new sign-in.
fn lock(app_dir: &Path, owner: &str) -> Result<SyncLock> {
    SyncLock::acquire_within(&app_dir.join("auth.lock"), owner, Duration::from_secs(30))
}

/// A usable access token, refreshed when about to expire. The GUI, hook workers and the git
/// credential helper may race here; refresh tokens rotate, so only one process may refresh.
pub fn valid_token(app_dir: &Path) -> Result<String> {
    let tokens = secrets::load_tokens()?.ok_or(Error::NotLoggedIn)?;
    if !expiring(&tokens) {
        return Ok(tokens.access);
    }
    let _lock = lock(app_dir, "refresh")?;
    let tokens = secrets::load_tokens()?.ok_or(Error::NotLoggedIn)?;
    if !expiring(&tokens) {
        return Ok(tokens.access); // another process refreshed while we waited
    }
    let refresh_token = tokens.refresh.ok_or(Error::NotLoggedIn)?;
    let fresh = github_api::refresh(&refresh_token)?;
    // The old refresh token is spent: even if storing fails, this operation can still use the new access token.
    if let Err(e) = secrets::save_tokens(&fresh).or_else(|_| secrets::save_tokens(&fresh)) {
        log::error!("cannot store refreshed tokens: {}", e.code());
    }
    Ok(fresh.access)
}

pub fn save_login(app_dir: &Path, tokens: &Tokens) -> Result<()> {
    let _lock = lock(app_dir, "login")?;
    secrets::save_tokens(tokens)
}

pub fn sign_out(app_dir: &Path) -> Result<()> {
    let _lock = lock(app_dir, "logout")?;
    secrets::clear_all()
}
