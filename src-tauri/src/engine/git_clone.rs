//! Clones a project repo with the user's own git setup (their credentials, e.g. Git Credential
//! Manager): the app's token only covers the storage repo, so the isolated `GitEnv` is not used.

use std::io::{ErrorKind, Read};
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use super::error::{Error, IoContext, Result};
use super::progress::transfer_percent;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// One clone at a time. `pid` is set while git runs; `cancelled` also covers a cancel before that.
#[derive(Default)]
struct Slot {
    busy: bool,
    pid: Option<u32>,
    cancelled: bool,
}

static SLOT: Mutex<Slot> = Mutex::new(Slot { busy: false, pid: None, cancelled: false });

fn slot() -> MutexGuard<'static, Slot> {
    SLOT.lock().unwrap_or_else(PoisonError::into_inner)
}

struct Release;

impl Drop for Release {
    fn drop(&mut self) {
        *slot() = Slot::default();
    }
}

/// `remote` is a normalized key (`github.com/owner/repo`), never user text.
pub fn clone(remote: &str, dest: &Path, on_percent: &mut dyn FnMut(u8)) -> Result<()> {
    clone_url(&format!("https://{remote}.git"), dest, on_percent)
}

pub fn clone_url(url: &str, dest: &Path, on_percent: &mut dyn FnMut(u8)) -> Result<()> {
    {
        let mut s = slot();
        if s.busy {
            return Err(Error::Busy);
        }
        s.busy = true;
    }
    let _release = Release;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).at(parent)?;
    }
    // Claim the folder first: only a folder this call created is ever deleted.
    match std::fs::create_dir(dest) {
        Err(e) if e.kind() == ErrorKind::AlreadyExists => return Err(Error::DestinationExists),
        other => other.at(dest)?,
    }
    let result = run_git(url, dest, on_percent);
    if result.is_err() {
        remove_partial(dest);
    }
    result
}

fn run_git(url: &str, dest: &Path, on_percent: &mut dyn FnMut(u8)) -> Result<()> {
    let mut child = Command::new("git")
        .args(["clone", "--progress", "--", url])
        .arg(dest)
        // No terminal to answer a prompt; Git Credential Manager still shows its own window.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C") // progress and error lines are parsed
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .at(dest)?;
    let cancelled_early = {
        let mut s = slot();
        s.pid = Some(child.id());
        s.cancelled
    };
    if cancelled_early {
        kill(child.id());
    }
    let mut stderr = child.stderr.take().expect("piped");
    let (mut buf, mut log, mut last) = ([0u8; 4096], String::new(), None);
    while let Ok(n @ 1..) = stderr.read(&mut buf) {
        let text = String::from_utf8_lossy(&buf[..n]);
        if let Some(percent) = transfer_percent(&text).filter(|p| last != Some(*p)) {
            last = Some(percent);
            on_percent(percent);
        }
        log.push_str(&text);
    }
    let status = child.wait().at(dest);
    // Cleared while `child` still holds the process handle, so `cancel` never kills a reused pid.
    let cancelled = {
        let mut s = slot();
        s.pid = None;
        s.cancelled
    };
    if cancelled {
        return Err(Error::Cancelled);
    }
    if !status?.success() {
        let lines: Vec<&str> = log.split(['\r', '\n']).filter(|l| l.starts_with("fatal:") || l.starts_with("error:") || (l.starts_with("remote:") && !l.contains('%'))).collect();
        return Err(Error::Clone(lines[lines.len().saturating_sub(5)..].join(" ")));
    }
    Ok(())
}

/// Git's helpers, antivirus or the indexer can hold a new pack file open for a moment after a kill.
fn remove_partial(dest: &Path) {
    for wait in [0, 200, 500, 1000, 2000] {
        std::thread::sleep(Duration::from_millis(wait));
        match std::fs::remove_dir_all(dest) {
            Err(e) if e.kind() != ErrorKind::NotFound => continue,
            _ => return,
        }
    }
    // Without its config the leftover has no origin, so a scan never offers it as a checkout.
    let _ = std::fs::remove_file(dest.join(".git").join("config"));
    log::warn!("partial clone left at {}", dest.display());
}

/// Stops the running clone (and git's helpers); `clone` then removes the partial folder.
pub fn cancel() {
    let pid = {
        let mut s = slot();
        if !s.busy {
            return;
        }
        s.cancelled = true;
        s.pid
    };
    if let Some(pid) = pid {
        kill(pid);
    }
}

fn kill(pid: u32) {
    let _ = Command::new("taskkill").args(["/T", "/F", "/PID", &pid.to_string()]).stdout(Stdio::null()).stderr(Stdio::null()).creation_flags(CREATE_NO_WINDOW).status();
}
