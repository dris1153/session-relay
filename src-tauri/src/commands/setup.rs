use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use age::secrecy::ExposeSecret;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use super::{blocking, CmdResult};
use crate::app_state::AppState;
use crate::engine::claude_hook_config::HookStatus;
use crate::engine::crypto::Keys;
use crate::engine::error::Error;
use crate::engine::lock::SyncLock;
use crate::engine::settings::RepoRef;
use crate::engine::storage_check::{self, StorageCheck, StorageState};
use crate::engine::{auth, github_api, key_setup, secrets};
use crate::login::{spawn_poll, AuthChanged};

const MIN_GIT: (u32, u32) = (2, 35);

#[derive(Serialize)]
pub struct AppStateDto {
    git_version: Option<String>,
    git_ok: bool,
    has_client_id: bool,
    signed_in: bool,
    identity_unlocked: bool,
    repo: Option<RepoRef>,
    machine_name: String,
    claude_home: PathBuf,
    workspace_roots: Vec<PathBuf>,
    language: Option<String>,
    autostart: bool,
    hooks: HookStatus,
}

fn git_version() -> Option<(String, bool)> {
    let out = std::process::Command::new("git").arg("--version").creation_flags(0x0800_0000).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let mut nums = text.split_whitespace().nth(2)?.split('.').filter_map(|n| n.parse::<u32>().ok());
    let version = (nums.next()?, nums.next()?);
    Some((text, version >= MIN_GIT))
}

pub(super) fn app_state_dto(state: &AppState) -> crate::engine::error::Result<AppStateDto> {
    let s = state.settings();
    let git = git_version();
    Ok(AppStateDto {
        git_ok: git.as_ref().is_some_and(|(_, ok)| *ok),
        git_version: git.map(|(v, _)| v),
        has_client_id: github_api::client_id().is_ok() && github_api::app_slug().is_ok(),
        signed_in: secrets::load_tokens()?.is_some(),
        identity_unlocked: state.engine().is_some(),
        repo: s.repo,
        machine_name: s.machine_name,
        claude_home: s.claude_home,
        workspace_roots: s.workspace_roots,
        language: s.language,
        autostart: s.autostart,
        hooks: crate::auto_save::status(state),
    })
}

#[tauri::command]
pub async fn get_app_state(state: State<'_, Arc<AppState>>) -> CmdResult<AppStateDto> {
    let state = Arc::clone(&state);
    blocking(move || app_state_dto(&state)).await
}

#[derive(Serialize)]
pub struct LoginCode {
    user_code: String,
    verification_uri: String,
    expires_in: u64,
}

#[tauri::command]
pub async fn start_login(app: AppHandle, state: State<'_, Arc<AppState>>) -> CmdResult<LoginCode> {
    let state = Arc::clone(&state);
    let code = blocking(github_api::request_device_code).await?;
    let attempt = state.login_attempt.fetch_add(1, Ordering::SeqCst) + 1;
    let shown = LoginCode { user_code: code.user_code.clone(), verification_uri: code.verification_uri.clone(), expires_in: code.expires_in };
    spawn_poll(app, state, code, attempt);
    Ok(shown)
}

#[tauri::command]
pub async fn logout(app: AppHandle, state: State<'_, Arc<AppState>>) -> CmdResult<()> {
    let state = Arc::clone(&state);
    state.login_attempt.fetch_add(1, Ordering::SeqCst);
    state.drop_engine();
    let app_dir = state.app_dir.clone();
    blocking(move || auth::sign_out(&app_dir)).await?;
    let _ = app.emit("auth-changed", AuthChanged { signed_in: false, reason: None });
    Ok(())
}

#[tauri::command]
pub async fn check_storage(state: State<'_, Arc<AppState>>) -> CmdResult<StorageCheck> {
    let state = Arc::clone(&state);
    blocking(move || {
        let check = checked_repo(&state)?;
        let ready = check.state == StorageState::Ready;
        // Public, gone or re-keyed: stop syncing (hook workers too) until onboarding passes again.
        crate::hook_worker::set_storage_blocked(&state.app_dir, !ready);
        if ready && state.engine().is_none() {
            let identity = secrets::load_identity()?.ok_or(Error::NotLoggedIn)?;
            state.install_engine(Keys::parse(identity.expose_secret())?)?;
        } else if !ready {
            state.drop_engine();
        }
        Ok(check)
    })
    .await
}

/// Runs the storage check and remembers the repo only once it is a usable (private, own) store.
fn checked_repo(state: &AppState) -> crate::engine::error::Result<StorageCheck> {
    let check = storage_check::check(&state.app_dir, &state.settings())?;
    if let (Some(repo), true) = (&check.repo, check.key_files.is_some()) {
        if state.settings().repo.as_ref() != Some(repo) {
            state.update_settings(|s| s.repo = Some(repo.clone()))?;
        }
    }
    Ok(check)
}

/// 0–4 strength for the passphrase meter (zxcvbn; creating a key requires 3+).
#[tauri::command]
pub fn passphrase_strength(passphrase: String) -> u8 {
    u8::from(zxcvbn::zxcvbn(&passphrase, &[]).score())
}

#[tauri::command]
pub async fn create_key(state: State<'_, Arc<AppState>>, passphrase: String) -> CmdResult<()> {
    let state = Arc::clone(&state);
    blocking(move || {
        let check = checked_repo(&state)?;
        match check.state {
            StorageState::NeedsNewKey => {}
            StorageState::NeedsUnlock | StorageState::Ready => return Err(Error::KeyExists),
            _ => return Err(Error::Invalid("the storage repository is not usable".into())),
        }
        let store = state.store_for(&state.repo()?);
        let _lock = SyncLock::acquire_within(&state.app_dir.join("sync.lock"), "key-setup", Duration::from_secs(30))?;
        remember_key(&state, key_setup::create_key(&store, &passphrase)?)
    })
    .await
}

#[tauri::command]
pub async fn unlock_key(state: State<'_, Arc<AppState>>, passphrase: String) -> CmdResult<()> {
    let state = Arc::clone(&state);
    blocking(move || {
        let check = checked_repo(&state)?;
        let files = check.key_files.filter(|_| matches!(check.state, StorageState::NeedsUnlock | StorageState::Ready));
        let files = files.ok_or_else(|| Error::Invalid("the storage repository has no key to unlock".into()))?;
        remember_key(&state, key_setup::unlock_key(&files, &passphrase)?)
    })
    .await
}

fn remember_key(state: &AppState, keys: Keys) -> crate::engine::error::Result<()> {
    secrets::save_identity(&keys.identity_secret())?;
    state.install_engine(keys)
}
