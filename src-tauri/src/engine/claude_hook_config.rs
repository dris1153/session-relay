use std::path::Path;

use serde::Serialize;
use serde_json::{json, Value};

use super::error::{Error, IoContext, Result};
use super::fs_util::write_atomic;

/// `Stop` ends every Claude turn (the only reliable trigger in VS Code, Spike B); `SessionEnd`
/// saves right away when a CLI session exits.
const EVENTS: [&str; 2] = ["Stop", "SessionEnd"];
const ARG: &str = "hook-save";
const TIMEOUT_S: u64 = 10;
const KEEP_BACKUPS: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HookStatus {
    Installed,
    NotInstalled,
    /// Registered for another exe path, or for one event only.
    StalePath,
    /// `settings.json` cannot be read or is not JSON of the expected shape: never written.
    Malformed,
}

fn exe_command(exe: &Path) -> String {
    exe.to_string_lossy().replace('\\', "/")
}

fn is_ours(hook: &Value) -> bool {
    hook["args"] == json!([ARG]) && hook["command"].as_str().is_some_and(|c| c.to_ascii_lowercase().ends_with("session-relay.exe"))
}

/// None when the file does not exist yet.
fn load(path: &Path) -> Result<Option<Value>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).at(path),
    };
    let root: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| Error::Invalid(format!("{}: {e}", path.display())))?;
    let hooks_ok = match root.get("hooks") {
        None => true,
        Some(Value::Object(events)) => events.values().all(Value::is_array),
        Some(_) => false,
    };
    if !root.is_object() || !hooks_ok {
        return Err(Error::Invalid(format!("{}: unexpected shape", path.display())));
    }
    Ok(Some(root))
}

/// Commands of our hooks registered for `event`.
fn ours(root: &Value, event: &str) -> Vec<String> {
    let groups = root["hooks"][event].as_array().into_iter().flatten();
    groups.flat_map(|g| g["hooks"].as_array().into_iter().flatten()).filter(|h| is_ours(h)).filter_map(|h| h["command"].as_str().map(str::to_owned)).collect()
}

/// Exe paths our hooks point at, over both events.
pub fn registered(settings: &Path) -> Vec<String> {
    load(settings).ok().flatten().map_or_else(Vec::new, |root| EVENTS.iter().flat_map(|e| ours(&root, e)).collect())
}

pub fn status(settings: &Path, exe: &Path) -> HookStatus {
    let root = match load(settings) {
        Ok(Some(root)) => root,
        Ok(None) => return HookStatus::NotInstalled,
        Err(_) => return HookStatus::Malformed,
    };
    let want = exe_command(exe).to_ascii_lowercase();
    let found: Vec<Vec<String>> = EVENTS.iter().map(|e| ours(&root, e)).collect();
    if found.iter().all(Vec::is_empty) {
        HookStatus::NotInstalled
    } else if found.iter().all(|c| c.len() == 1 && c[0].to_ascii_lowercase() == want) {
        HookStatus::Installed
    } else {
        HookStatus::StalePath
    }
}

/// Registers `exe hook-save` for both events, replacing any older registration of ours.
/// Everything else in the file is kept as is; nothing is written when already installed.
pub fn install(settings: &Path, exe: &Path) -> Result<()> {
    if status(settings, exe) == HookStatus::Installed {
        return Ok(());
    }
    let mut root = load(settings)?.unwrap_or_else(|| json!({}));
    remove_ours(&mut root);
    let group = json!({ "hooks": [{ "type": "command", "command": exe_command(exe), "args": [ARG], "timeout": TIMEOUT_S }] });
    let hooks = root.as_object_mut().expect("checked by load").entry("hooks").or_insert_with(|| json!({}));
    for event in EVENTS {
        let list = hooks.as_object_mut().expect("checked by load").entry(event).or_insert_with(|| json!([]));
        list.as_array_mut().expect("checked by load").push(group.clone());
    }
    write(settings, &root)
}

/// Hooks that point at an exe that no longer exists (the app moved to another folder) are
/// pointed at `exe`. True when the file was rewritten.
pub fn repair(settings: &Path, exe: &Path) -> Result<bool> {
    let gone = |command: &String| !Path::new(command).exists();
    if status(settings, exe) != HookStatus::StalePath || !registered(settings).iter().all(gone) {
        return Ok(false);
    }
    install(settings, exe)?;
    Ok(true)
}

pub fn uninstall(settings: &Path) -> Result<()> {
    let Some(mut root) = load(settings)? else { return Ok(()) };
    if remove_ours(&mut root) {
        write(settings, &root)?;
    }
    Ok(())
}

/// Drops our hooks, then any group, event list or `hooks` object that only held them.
fn remove_ours(root: &mut Value) -> bool {
    let Some(events) = root.get_mut("hooks").and_then(Value::as_object_mut) else { return false };
    let mut changed = false;
    for event in EVENTS {
        let Some(groups) = events.get_mut(event).and_then(Value::as_array_mut) else { continue };
        let mut removed = false;
        groups.retain_mut(|group| {
            let Some(hooks) = group.get_mut("hooks").and_then(Value::as_array_mut) else { return true };
            let before = hooks.len();
            hooks.retain(|h| !is_ours(h));
            removed |= hooks.len() != before;
            !(hooks.is_empty() && hooks.len() != before)
        });
        if groups.is_empty() && removed {
            events.remove(event);
        }
        changed |= removed;
    }
    if events.is_empty() && changed {
        root.as_object_mut().expect("checked by load").remove("hooks");
    }
    changed
}

/// Keeps the previous file next to it (the last few): this is the user's Claude configuration.
/// A symlinked `settings.json` (dotfiles) is written through, not replaced by a plain file.
fn write(path: &Path, root: &Value) -> Result<()> {
    let target = if path.is_symlink() { std::fs::canonicalize(path).at(path)? } else { path.to_path_buf() };
    if target.exists() {
        let backup = path.with_file_name(format!("settings.json.bak-{}", chrono::Local::now().format("%Y%m%d-%H%M%S%3f")));
        std::fs::copy(&target, &backup).at(&backup)?;
        prune_backups(path);
    }
    let mut text = serde_json::to_vec_pretty(root).expect("serializable");
    text.push(b'\n');
    write_atomic(&target, &text)
}

fn prune_backups(path: &Path) {
    let Some(dir) = path.parent() else { return };
    let is_backup = |p: &Path| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("settings.json.bak-"));
    let mut backups: Vec<_> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| is_backup(p)).collect();
    backups.sort();
    for old in backups.iter().rev().skip(KEEP_BACKUPS) {
        let _ = std::fs::remove_file(old);
    }
}
