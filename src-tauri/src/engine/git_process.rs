use std::io::{Read, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::error::{Error, IoContext, Result};
pub use super::git_failure::failure;

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

#[derive(Clone, Copy)]
enum Limit {
    Total(Duration),
    Stall(Duration),
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
        // English messages: `failure()` classifies errors by their text.
        cmd.env("LC_ALL", "C")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "NUL")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "never")
            .env("GIT_CONFIG_COUNT", config.len().to_string());
        for (i, (k, v)) in config.iter().enumerate() {
            cmd.env(format!("GIT_CONFIG_KEY_{i}"), k).env(format!("GIT_CONFIG_VALUE_{i}"), v);
        }
    }

    pub fn run(&self, dir: &Path, args: &[&str], timeout: Duration) -> Result<Output> {
        self.exec(dir, args, &[], Limit::Total(timeout), &mut |_| {})
    }

    /// Feeds `input` to stdin (`cat-file --batch`).
    pub fn run_with_input(&self, dir: &Path, args: &[&str], input: &[u8], timeout: Duration) -> Result<Output> {
        self.exec(dir, args, input, Limit::Total(timeout), &mut |_| {})
    }

    /// For transfers of any size: killed only after `stall` without output. Pass `--progress`
    /// so git keeps writing; each stderr chunk also goes to `on_stderr`.
    pub fn run_watched(&self, dir: &Path, args: &[&str], stall: Duration, on_stderr: &mut dyn FnMut(&str)) -> Result<Output> {
        self.exec(dir, args, &[], Limit::Stall(stall), on_stderr)
    }

    fn exec(&self, dir: &Path, args: &[&str], input: &[u8], limit: Limit, on_stderr: &mut dyn FnMut(&str)) -> Result<Output> {
        let label = args.first().copied().unwrap_or("git").to_string();
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(dir).args(args).stdin(if input.is_empty() { Stdio::null() } else { Stdio::piped() }).stdout(Stdio::piped()).stderr(Stdio::piped()).creation_flags(CREATE_NO_WINDOW);
        self.configure(&mut cmd);
        // Pin the repository: with a broken `store/.git`, git would otherwise walk up into a parent repo.
        cmd.env("GIT_DIR", dir.join(".git")).env("GIT_WORK_TREE", dir);
        if let Some(parent) = dir.parent() {
            cmd.env("GIT_CEILING_DIRECTORIES", parent);
        }
        // Not `Error::Git`: a git that cannot start (upgrade, antivirus) says nothing about the clone.
        let mut child = cmd.spawn().at(dir)?;
        if let Some(mut stdin) = child.stdin.take() {
            // Written from a thread: git may fill its stdout pipe before reading all of stdin.
            let input = input.to_vec();
            std::thread::spawn(move || stdin.write_all(&input));
        }
        let (mut out, mut err) = (child.stdout.take().expect("piped"), child.stderr.take().expect("piped"));
        let out_thread = std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = out.read_to_end(&mut buf);
            buf
        });
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let err_thread = std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(n @ 1..) = err.read(&mut buf) {
                if tx.send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
        });
        let mut stderr = Vec::new();
        let mut take = |chunk: Vec<u8>| {
            on_stderr(&String::from_utf8_lossy(&chunk));
            stderr.extend(chunk);
        };
        let (started, mut last_output) = (Instant::now(), Instant::now());
        let status = loop {
            for chunk in rx.try_iter() {
                last_output = Instant::now();
                take(chunk);
            }
            if let Some(status) = child.try_wait().at(dir)? {
                break status;
            }
            let expired = match limit {
                Limit::Total(d) => started.elapsed() > d,
                Limit::Stall(d) => last_output.elapsed() > d,
            };
            if expired {
                kill_tree(child.id());
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::GitTimeout { args: label });
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let _ = err_thread.join();
        rx.try_iter().for_each(&mut take);
        let stdout = out_thread.join().unwrap_or_default();
        Ok(Output { status_ok: status.success(), stdout, stderr: String::from_utf8_lossy(&stderr).into_owned() })
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

/// git-remote-https and credential helpers are children of git; kill them too.
fn kill_tree(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/T", "/F", "/PID", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}
