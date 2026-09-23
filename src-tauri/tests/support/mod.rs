#![allow(dead_code)] // shared by several test crates; each uses a subset

//! Two simulated machines sharing one local bare repo as the "GitHub" store.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use age::secrecy::ExposeSecret;
use session_relay_lib::engine::activity::Source;
use session_relay_lib::engine::context::Engine;
use session_relay_lib::engine::crypto::Keys;
use session_relay_lib::engine::overview;
use session_relay_lib::engine::paths::{encode_dir, CoreConfig};
use session_relay_lib::engine::project_identity::ProjectKey;
use session_relay_lib::engine::sync::{self, Request, SyncMode, SyncReport};

pub const SID: &str = "0dc9e69a-c29f-4595-9d8f-ce29ae1c2504";
pub const REMOTE: &str = "github.com/acme/app";

pub struct Machine {
    pub engine: Engine,
    pub checkout: PathBuf,
}

impl Machine {
    pub fn new(root: &Path, name: &str, remote_url: &str, identity: &str) -> Self {
        let base = root.join(name);
        let checkout = base.join("work").join("app");
        std::fs::create_dir_all(checkout.join(".git")).unwrap();
        std::fs::write(checkout.join(".git/config"), "[remote \"origin\"]\n\turl = git@github.com:acme/app.git\n").unwrap();
        let cfg = CoreConfig { app_dir: base.join("appdata"), claude_home: base.join("claude"), machine_name: name.to_string() };
        let engine = Engine::new(cfg, Keys::parse(identity).unwrap(), remote_url.to_string(), None, true);
        Self { engine, checkout }
    }

    /// What the user does once per machine (or discovery does for sessions created here).
    pub fn link(&mut self) {
        overview::link_repo(&self.engine, REMOTE, &self.checkout).unwrap();
    }

    pub fn key(&self) -> ProjectKey {
        ProjectKey { remote: REMOTE.into(), subpath: String::new() }
    }

    pub fn project_dir(&self) -> PathBuf {
        self.dir_of("")
    }

    /// Claude project dir for a cwd below the checkout ("" = repo root).
    pub fn dir_of(&self, subpath: &str) -> PathBuf {
        let mut cwd = self.checkout.clone();
        subpath.split('/').filter(|p| !p.is_empty()).for_each(|p| cwd.push(p));
        self.engine.cfg.projects_dir().join(encode_dir(&cwd.to_string_lossy()))
    }

    pub fn file(&self, rel: &str) -> PathBuf {
        let mut p = self.project_dir();
        rel.split('/').for_each(|s| p.push(s));
        p
    }

    /// JSON-escaped absolute path of this machine's project dir, as Claude writes it.
    pub fn escaped_dir(&self) -> String {
        let json = serde_json::to_string(&self.project_dir().to_string_lossy()).unwrap();
        json[1..json.len() - 1].to_string()
    }

    pub fn write(&self, rel: &str, bytes: &[u8]) {
        let path = self.file(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    pub fn append(&self, rel: &str, text: &str) {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).open(self.file(rel)).unwrap();
        f.write_all(text.as_bytes()).unwrap();
    }

    pub fn sync(&mut self, mode: SyncMode) -> SyncReport {
        self.try_sync(mode, None).expect("sync")
    }

    pub fn try_sync(&mut self, mode: SyncMode, only: Option<&[String]>) -> session_relay_lib::engine::error::Result<SyncReport> {
        self.sync_key(&self.key(), mode, only)
    }

    pub fn sync_key(&self, key: &ProjectKey, mode: SyncMode, only: Option<&[String]>) -> session_relay_lib::engine::error::Result<SyncReport> {
        let req = Request { key, mode, only, source: Source::Gui, lock_wait: Duration::from_secs(5) };
        sync::sync_project(&self.engine, &req)
    }

    /// Sets a file's mtime (seconds since the epoch) to control last-writer-wins.
    pub fn touch(&self, rel: &str, secs: u64) {
        let f = std::fs::OpenOptions::new().write(true).open(self.file(rel)).unwrap();
        f.set_modified(std::time::UNIX_EPOCH + Duration::from_secs(secs)).unwrap();
    }
}

pub fn bare_remote(root: &Path) -> String {
    let dir = root.join("remote.git");
    let ok = Command::new("git").args(["init", "-q", "--bare", "-b", "main"]).arg(&dir).status().unwrap().success();
    assert!(ok);
    format!("file:///{}", dir.to_string_lossy().replace('\\', "/"))
}

pub fn remote_head(root: &Path) -> String {
    let out = Command::new("git").arg("-C").arg(root.join("remote.git")).args(["rev-parse", "refs/heads/main"]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

pub fn identity() -> String {
    Keys::generate().identity_secret().expose_secret().to_string()
}

pub fn transcript_line(kind: &str, cwd: &Path, text: &str) -> String {
    format!("{}\n", serde_json::json!({ "type": kind, "cwd": cwd.to_string_lossy(), "message": { "content": text } }))
}
