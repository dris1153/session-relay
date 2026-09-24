use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, OnceLock};

use age::secrecy::ExposeSecret;
use tauri::{AppHandle, Emitter};

use crate::app_paths::{app_dir, credential_helper};
use crate::dashboard::DashboardCache;
use crate::engine::context::Engine;
use crate::engine::crypto::Keys;
use crate::engine::error::{Error, Result};
use crate::engine::git_process::GitEnv;
use crate::engine::paths::CoreConfig;
use crate::engine::secrets;
use crate::engine::settings::{RepoRef, Settings};
use crate::engine::store_repo::StoreRepo;

/// Process-wide state behind every command. The engine exists only once the key is unlocked.
pub struct AppState {
    pub app_dir: PathBuf,
    settings: Mutex<Settings>,
    engine: Mutex<Option<Arc<Engine>>>,
    /// Bumped by each login attempt so an older polling loop stops.
    pub login_attempt: AtomicU64,
    pub dashboard: Mutex<DashboardCache>,
    /// Set in `setup`; lets the engine report progress to the window.
    pub app: OnceLock<AppHandle>,
}

impl AppState {
    pub fn load() -> Self {
        let app_dir = app_dir();
        // A hand-edited, broken file must not keep the app from starting; the next save rewrites it.
        let settings = Settings::load(&app_dir.join("settings.json")).unwrap_or_else(|e| {
            log::error!("cannot load settings, using defaults: {e}");
            Settings::defaults()
        });
        Self { app_dir, settings: Mutex::new(settings), engine: Mutex::new(None), login_attempt: AtomicU64::new(0), dashboard: Mutex::default(), app: OnceLock::new() }
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().expect("settings lock").clone()
    }

    pub fn update_settings(&self, change: impl FnOnce(&mut Settings)) -> Result<Settings> {
        let mut guard = self.settings.lock().expect("settings lock");
        let mut next = guard.clone();
        change(&mut next);
        next.save(&self.app_dir.join("settings.json"))?;
        *guard = next.clone();
        Ok(next)
    }

    pub fn store_for(&self, repo: &RepoRef) -> StoreRepo {
        let git = GitEnv { hooks_dir: self.app_dir.join("hooks-empty"), credential_helper: credential_helper(), allow_file_protocol: false };
        StoreRepo::new(self.app_dir.join("store"), repo.url(), git)
    }

    pub fn repo(&self) -> Result<RepoRef> {
        self.settings().repo.ok_or_else(|| Error::Invalid("no storage repository selected".into()))
    }

    pub fn install_engine(&self, keys: Keys) -> Result<()> {
        let settings = self.settings();
        let repo = self.repo()?;
        let cfg = CoreConfig { app_dir: self.app_dir.clone(), claude_home: settings.claude_home, machine_name: settings.machine_name };
        let engine = Engine::new(cfg, keys, repo.url(), credential_helper(), false);
        if let Some(app) = self.app.get().cloned() {
            engine.repo.progress.set(move |p| {
                let _ = app.emit("sync-progress", p);
            });
        }
        let mut cache = crate::dashboard::cache(self);
        cache.view = None;
        cache.generation += 1;
        drop(cache);
        *self.engine.lock().expect("engine lock") = Some(Arc::new(engine));
        Ok(())
    }

    /// Unlocks from the cached identity without any network: an offline start still reaches the
    /// dashboard. Also rebuilds the engine after settings it depends on changed.
    pub fn restore_engine(&self) -> Result<bool> {
        let (Some(_), Some(identity)) = (self.settings().repo, secrets::load_identity()?) else { return Ok(false) };
        self.install_engine(Keys::parse(identity.expose_secret())?)?;
        Ok(true)
    }

    pub fn engine(&self) -> Option<Arc<Engine>> {
        self.engine.lock().expect("engine lock").clone()
    }

    pub fn drop_engine(&self) {
        *self.engine.lock().expect("engine lock") = None;
    }
}
