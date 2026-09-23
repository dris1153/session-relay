use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::error::{Error, IoContext, Result};
use super::fs_util::write_atomic;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoRef {
    pub owner: String,
    pub name: String,
}

impl RepoRef {
    pub fn url(&self) -> String {
        format!("https://github.com/{}/{}.git", self.owner, self.name)
    }
}

/// Machine-local preferences (`settings.json` in the app data dir). Secrets never go here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub repo: Option<RepoRef>,
    pub machine_name: String,
    /// Stored once so the GUI (started from the Start menu) and hook workers (started by Claude,
    /// which may have `CLAUDE_CONFIG_DIR`) agree on the same Claude home.
    pub claude_home: PathBuf,
    #[serde(default)]
    pub workspace_roots: Vec<PathBuf>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default = "default_true")]
    pub autostart: bool,
}

fn default_true() -> bool {
    true
}

impl Settings {
    pub fn defaults() -> Self {
        let claude_home = std::env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(std::env::var_os("USERPROFILE").unwrap_or_default()).join(".claude"));
        let machine_name = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "this-pc".into());
        Self { repo: None, machine_name, claude_home, workspace_roots: Vec::new(), language: None, autostart: true }
    }

    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| Error::Invalid(format!("{}: {e}", path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::defaults()),
            Err(e) => Err(e).at(path),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        write_atomic(path, &serde_json::to_vec_pretty(self).expect("serializable"))
    }
}
