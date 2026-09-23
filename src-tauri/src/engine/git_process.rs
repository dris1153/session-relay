use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::error::{Error, Result};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// How the store clone talks to git. Everything the user configured globally is ignored
/// (Spike D): hooks, fsmonitor, `url.insteadOf`, proxies-by-accident and credential helpers.
#[derive(Debug, Clone)]
pub struct GitEnv {
    pub hooks_dir: PathBuf,
    /// Command for `credential.helper` (our own exe); None for local test remotes.
    pub credential_helper: Option<String>,
    /// Only for tests against `file://` bare repos.
    pub allow_file_protocol: bool,
}

pub struct Output {
    pub status_ok: bool,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

impl GitEnv {
    fn configure(&self, cmd: &mut Command) {
        for (key, _) in std::env::vars_os() {
            let k = key.to_string_lossy().to_ascii_uppercase();
            if k.starts_with("GIT_") || k.starts_with("SSH_") {
                cmd.env_remove(&key);
            }
        }
        let hooks = self.hooks_dir.to_string_lossy().into_owned();
        let mut config: Vec<(&str, String)> = vec![
            ("http.sslBackend", "schannel".into()),
            ("core.hooksPath", hooks),
            ("core.fsmonitor", "false".into()),
            ("core.autocrlf", "false".into()),
            ("core.longpaths", "true".into()),
            ("gc.auto", "0".into()),
            ("protocol.allow", "never".into()),
            ("protocol.https.allow", "always".into()),
            ("user.name", "session-relay".into()),
            ("user.email", "session-relay@localhost".into()),
            ("credential.helper", String::new()),
        ];
        if let Some(helper) = &self.credential_helper {
            config.push(("credential.helper", helper.clone()));
        }
        if self.allow_file_protocol {
            config.push(("protocol.file.allow", "always".into()));
        }
        cmd.env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "NUL")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "never")
            .env("GIT_CONFIG_COUNT", config.len().to_string());
        for (i, (k, v)) in config.iter().enumerate() {
            cmd.env(format!("GIT_CONFIG_KEY_{i}"), k).env(format!("GIT_CONFIG_VALUE_{i}"), v);
        }
    }

    pub fn run(&self, dir: &Path, args: &[&str], timeout: Duration) -> Result<Output> {
        let label = args.first().copied().unwrap_or("git").to_string();
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(dir).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).creation_flags(CREATE_NO_WINDOW);
        self.configure(&mut cmd);
        // Pin the repository: with a broken `store/.git`, git would otherwise walk up into a parent repo.
        cmd.env("GIT_DIR", dir.join(".git")).env("GIT_WORK_TREE", dir);
        if let Some(parent) = dir.parent() {
            cmd.env("GIT_CEILING_DIRECTORIES", parent);
        }
        let mut child = cmd.spawn().map_err(|e| Error::Git { args: label.clone(), stderr: e.to_string() })?;
        let (mut out, mut err) = (child.stdout.take().expect("piped"), child.stderr.take().expect("piped"));
        let out_thread = std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = out.read_to_end(&mut buf);
            buf
        });
        let err_thread = std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = err.read_to_end(&mut buf);
            buf
        });
        let deadline = Instant::now() + timeout;
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|e| Error::Git { args: label.clone(), stderr: e.to_string() })? {
                break status;
            }
            if Instant::now() > deadline {
                kill_tree(child.id());
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::GitTimeout { args: label });
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let stdout = out_thread.join().unwrap_or_default();
        let stderr = String::from_utf8_lossy(&err_thread.join().unwrap_or_default()).into_owned();
        Ok(Output { status_ok: status.success(), stdout, stderr })
    }

    /// Like `run`, but a non-zero exit is an error.
    pub fn check(&self, dir: &Path, args: &[&str], timeout: Duration) -> Result<Vec<u8>> {
        let out = self.run(dir, args, timeout)?;
        if out.status_ok {
            Ok(out.stdout)
        } else {
            Err(failure(&args.join(" "), &out.stderr))
        }
    }
}

/// A refused credential (signed out, grant revoked) is not a broken clone: it must reach the
/// user as "sign in again" and must never trigger a clone rebuild.
pub fn failure(args: &str, stderr: &str) -> Error {
    let stderr = stderr.trim().to_string();
    if ["could not read Username", "Authentication failed", "returned error: 401"].iter().any(|s| stderr.contains(s)) {
        Error::Auth(stderr)
    } else {
        Error::Git { args: args.into(), stderr }
    }
}

/// git-remote-https and credential helpers are children of git; kill them too.
fn kill_tree(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/T", "/F", "/PID", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refused_credentials_are_auth_errors_not_broken_clones() {
        let refused = "fatal: could not read Username for 'https://github.com': terminal prompts disabled";
        assert!(matches!(failure("fetch", refused), Error::Auth(_)));
        assert!(matches!(failure("fetch", "fatal: Authentication failed for 'https://github.com/me/s.git/'"), Error::Auth(_)));
        assert!(matches!(failure("fetch", "fatal: bad object HEAD"), Error::Git { .. }));
    }
}
