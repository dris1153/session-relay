//! Claude Code hooks. `hook-save` runs inside Claude's hook and must return at once: it records a
//! pending marker and starts a detached `hook-worker` (see `hook_worker.rs`).

use std::io::Read;
use std::os::windows::ffi::OsStrExt;
use std::path::{Component, Path};
use std::time::Duration;

use serde::Deserialize;

use crate::app_paths::app_dir;
use crate::commands::settings::is_local_absolute;
use crate::engine::claude_hook_config;
use crate::engine::error::{Error, IoContext, Result};
use crate::engine::logging;
use crate::engine::settings::Settings;
use crate::pending;

/// The Stop payload carries the whole last reply; a cut-off read would lose the turn.
const MAX_INPUT: u64 = 16 * 1024 * 1024;
/// `Stop` fires after every Claude turn: wait for a quiet spell so a busy session saves once.
const STOP_DELAY: Duration = Duration::from_secs(120);
/// `SessionEnd`: Claude may still be writing its last lines while it exits.
const SESSION_END_DELAY: Duration = Duration::from_secs(5);

#[derive(Deserialize)]
struct HookInput {
    transcript_path: std::path::PathBuf,
    hook_event_name: String,
}

/// Always exit code 0 and nothing on stdout: a failing hook must never disturb Claude.
pub fn save() -> i32 {
    logging::init(app_dir().join("logs"), "hook");
    match std::panic::catch_unwind(record_and_spawn) {
        Ok(Err(e)) => log::warn!("hook-save: {} ({e})", e.code()),
        Err(_) => log::error!("hook-save panicked"),
        Ok(Ok(())) => {}
    }
    0
}

fn record_and_spawn() -> Result<()> {
    let mut stdin = std::io::stdin().lock();
    let mut input = Vec::new();
    (&mut stdin).take(MAX_INPUT).read_to_end(&mut input).map_err(|e| Error::Invalid(e.to_string()))?;
    // Read what is left so Claude never sees a broken pipe.
    let _ = std::io::copy(&mut stdin, &mut std::io::sink());
    let hook = parse_input(&input)?;
    let app = app_dir();
    let settings = Settings::load(&app.join("settings.json"))?;
    let enc = project_dir_of(&settings.claude_home.join("projects"), &hook.transcript_path)?;
    let stamp = pending::add(&app, &enc, &hook.hook_event_name)?;
    let delay = if hook.hook_event_name == "SessionEnd" { SESSION_END_DELAY } else { STOP_DELAY };
    spawn_detached(&["hook-worker", "--dir", &enc, "--stamp", &stamp.to_string(), "--delay", &delay.as_secs().to_string()], &app)
}

fn parse_input(bytes: &[u8]) -> Result<HookInput> {
    let text = String::from_utf8_lossy(bytes);
    // Some shells prepend a byte-order mark when piping.
    serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| Error::Invalid(e.to_string()))
}

/// `<projects>/<enc>/…` → `<enc>`. The path comes from a hook payload: anything that does not
/// resolve inside the Claude projects dir is refused (a UNC path is never even probed).
fn project_dir_of(projects: &Path, transcript: &Path) -> Result<String> {
    if !is_local_absolute(transcript) {
        return Err(Error::Invalid("transcript path is not a local absolute path".into()));
    }
    let projects = std::fs::canonicalize(projects).at(projects)?;
    let transcript = std::fs::canonicalize(transcript).at(transcript)?;
    let rest = transcript.strip_prefix(&projects).map_err(|_| Error::Invalid("transcript outside the Claude projects dir".into()))?;
    match rest.components().next() {
        Some(Component::Normal(enc)) if enc.to_str().is_some_and(pending::valid_enc) => Ok(enc.to_string_lossy().into_owned()),
        _ => Err(Error::Invalid("unexpected transcript location".into())),
    }
}

/// Starts this exe detached from Claude (Spike B): inherits no handle at all (Claude waits for
/// inherited pipes to close), no console, its own process group, out of Claude's job when
/// allowed (closing VS Code must not kill the save), and not sitting in the user's project dir.
fn spawn_detached(args: &[&str], cwd: &Path) -> Result<()> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{CreateProcessW, CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, PROCESS_INFORMATION, STARTUPINFOW};
    let exe = std::env::current_exe().map_err(|e| Error::Invalid(e.to_string()))?;
    // Args are our own tokens (validated dir names, numbers): no quoting beyond the exe path.
    let mut line: Vec<u16> = format!("\"{}\" {}", exe.display(), args.join(" ")).encode_utf16().chain([0]).collect();
    let cwd: Vec<u16> = cwd.as_os_str().encode_wide().chain([0]).collect();
    let base = CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP;
    for flags in [base | CREATE_BREAKAWAY_FROM_JOB, base] {
        // SAFETY: all pointers point to live, NUL-terminated buffers or zeroed structs for the call.
        unsafe {
            let mut startup: STARTUPINFOW = std::mem::zeroed();
            startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
            let mut process: PROCESS_INFORMATION = std::mem::zeroed();
            use std::ptr::null;
            if CreateProcessW(null(), line.as_mut_ptr(), null(), null(), 0, flags, null(), cwd.as_ptr(), &startup, &mut process) != 0 {
                CloseHandle(process.hProcess);
                CloseHandle(process.hThread);
                return Ok(());
            }
        }
    }
    Err(std::io::Error::last_os_error()).at(&exe)
}

/// `post-install`: an update may have moved the exe to another folder (0.2 → 0.3 renamed it).
/// Points the auto-save hooks and the start-with-Windows entry here before anything calls them.
pub fn post_install() -> i32 {
    logging::init(app_dir().join("logs"), "post-install");
    let settings = Settings::load(&app_dir().join("settings.json")).unwrap_or_else(|_| Settings::defaults());
    let repaired = std::env::current_exe().map_err(|e| Error::Invalid(e.to_string())).and_then(|exe| claude_hook_config::repair(&settings.claude_home.join("settings.json"), &exe));
    if let Err(e) = repaired {
        log::warn!("post-install: {}", e.code());
    }
    crate::autostart::refresh();
    0
}

/// `hook-uninstall`: the installer removes our hooks before deleting the exe, or Claude would
/// report a failing hook after every turn.
pub fn uninstall() -> i32 {
    logging::init(app_dir().join("logs"), "hook-uninstall");
    let settings = Settings::load(&app_dir().join("settings.json")).unwrap_or_else(|_| Settings::defaults());
    if let Err(e) = claude_hook_config::uninstall(&settings.claude_home.join("settings.json")) {
        log::warn!("hook-uninstall: {}", e.code());
    }
    0
}

#[cfg(test)]
mod tests {
    use super::{parse_input, project_dir_of};

    #[test]
    fn only_transcripts_inside_the_projects_dir_are_accepted() {
        let home = tempfile::tempdir().unwrap();
        let projects = home.path().join("projects");
        let transcript = projects.join("d--work-app").join("s.jsonl");
        std::fs::create_dir_all(transcript.parent().unwrap()).unwrap();
        std::fs::write(&transcript, b"{}").unwrap();
        std::fs::write(home.path().join("outside.jsonl"), b"{}").unwrap();
        assert_eq!(project_dir_of(&projects, &transcript).unwrap(), "d--work-app");
        assert!(project_dir_of(&projects, &projects.join("d--work-app").join("..").join("..").join("outside.jsonl")).is_err());
        assert!(project_dir_of(&projects, &home.path().join("outside.jsonl")).is_err());
        assert!(project_dir_of(&projects, std::path::Path::new(r"\\host\share\s.jsonl")).is_err());
    }

    #[test]
    fn a_long_reply_in_the_payload_still_parses() {
        let reply = "Xin chào ".repeat(40_000); // ~400 KB of multi-byte text
        let payload = serde_json::json!({ "transcript_path": "C:/x.jsonl", "hook_event_name": "Stop", "last_assistant_message": reply });
        let parsed = parse_input(format!("\u{feff}{payload}").as_bytes()).unwrap();
        assert_eq!(parsed.hook_event_name, "Stop");
    }
}
