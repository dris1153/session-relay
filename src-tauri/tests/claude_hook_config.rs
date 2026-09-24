use std::path::Path;

use serde_json::{json, Value};
use session_relay_lib::engine::claude_hook_config::{install, repair, status, uninstall, HookStatus};
use session_relay_lib::engine::error::Error;

const EXE: &str = r"C:\Users\me\AppData\Local\session-relay\session-relay.exe";

fn read(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn backups(dir: &Path) -> usize {
    std::fs::read_dir(dir).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with("settings.json.bak-")).count()
}

#[test]
fn install_on_a_fresh_profile_registers_both_events() {
    let dir = tempfile::tempdir().unwrap();
    let settings = dir.path().join("settings.json");
    assert_eq!(status(&settings, Path::new(EXE)), HookStatus::NotInstalled);
    install(&settings, Path::new(EXE)).unwrap();
    assert_eq!(status(&settings, Path::new(EXE)), HookStatus::Installed);
    let hook = &read(&settings)["hooks"]["Stop"][0]["hooks"][0];
    assert_eq!(hook["command"], "C:/Users/me/AppData/Local/session-relay/session-relay.exe");
    assert_eq!(hook["args"], json!(["hook-save"]));
    assert_eq!(read(&settings)["hooks"]["SessionEnd"][0], read(&settings)["hooks"]["Stop"][0]);
}

#[test]
fn other_hooks_and_keys_survive_install_reinstall_and_uninstall() {
    let dir = tempfile::tempdir().unwrap();
    let settings = dir.path().join("settings.json");
    let original = json!({
        "model": "opus",
        "hooks": {
            "Stop": [{ "hooks": [{ "type": "command", "command": "notify.exe" }] }],
            "PreToolUse": [{ "matcher": "Bash", "hooks": [{ "type": "command", "command": "guard.exe" }] }]
        },
        "permissions": { "allow": ["Bash(git status)"] }
    });
    std::fs::write(&settings, serde_json::to_string_pretty(&original).unwrap()).unwrap();

    install(&settings, Path::new(EXE)).unwrap();
    let once = read(&settings);
    install(&settings, Path::new(EXE)).unwrap();
    assert_eq!(read(&settings), once, "reinstall adds nothing");
    assert_eq!(once["hooks"]["Stop"].as_array().unwrap().len(), 2);
    assert_eq!(once["hooks"]["Stop"][0], original["hooks"]["Stop"][0]);

    uninstall(&settings).unwrap();
    assert_eq!(read(&settings), original);
    assert_eq!(status(&settings, Path::new(EXE)), HookStatus::NotInstalled);
    assert!(backups(dir.path()) >= 1, "the previous file is kept");
}

#[test]
fn a_moved_exe_is_stale_and_install_repairs_it() {
    let dir = tempfile::tempdir().unwrap();
    let settings = dir.path().join("settings.json");
    install(&settings, Path::new(r"D:\old\session-relay.exe")).unwrap();
    assert_eq!(status(&settings, Path::new(EXE)), HookStatus::StalePath);
    install(&settings, Path::new(EXE)).unwrap();
    assert_eq!(status(&settings, Path::new(EXE)), HookStatus::Installed);
    assert_eq!(read(&settings)["hooks"]["Stop"].as_array().unwrap().len(), 1);
}

#[test]
fn a_malformed_file_is_never_written() {
    let dir = tempfile::tempdir().unwrap();
    let settings = dir.path().join("settings.json");
    for broken in ["{ \"hooks\": [ }", "[1, 2]", "{ \"hooks\": { \"Stop\": {} } }"] {
        std::fs::write(&settings, broken).unwrap();
        assert_eq!(status(&settings, Path::new(EXE)), HookStatus::Malformed, "{broken}");
        assert!(matches!(install(&settings, Path::new(EXE)), Err(Error::Invalid(_))));
        assert!(matches!(uninstall(&settings), Err(Error::Invalid(_))));
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), broken);
    }
    assert_eq!(backups(dir.path()), 0);
}

#[test]
fn a_bom_is_accepted_repeated_installs_do_not_write_and_backups_are_capped() {
    let dir = tempfile::tempdir().unwrap();
    let settings = dir.path().join("settings.json");
    std::fs::write(&settings, "\u{feff}{ \"model\": \"opus\" }").unwrap();
    install(&settings, Path::new(EXE)).unwrap();
    assert_eq!(status(&settings, Path::new(EXE)), HookStatus::Installed);
    let written = std::fs::metadata(&settings).unwrap().modified().unwrap();
    install(&settings, Path::new(EXE)).unwrap();
    assert_eq!(std::fs::metadata(&settings).unwrap().modified().unwrap(), written, "already installed: untouched");
    for _ in 0..8 {
        uninstall(&settings).unwrap();
        install(&settings, Path::new(EXE)).unwrap();
    }
    assert!((1..=5).contains(&backups(dir.path())), "backups are capped");
    assert_eq!(read(&settings)["model"], "opus");
}

#[test]
fn repair_follows_the_exe_only_when_the_old_one_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let settings = dir.path().join("settings.json");
    let (old, new) = (dir.path().join("session-relay").join("session-relay.exe"), dir.path().join("Session Relay").join("session-relay.exe"));
    for exe in [&old, &new] {
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(exe, b"exe").unwrap();
    }
    assert!(!repair(&settings, &new).unwrap(), "nothing installed, nothing to repair");
    install(&settings, &old).unwrap();
    assert!(!repair(&settings, &new).unwrap(), "the old exe still exists: another install owns the hooks");
    std::fs::remove_file(&old).unwrap();
    assert!(repair(&settings, &new).unwrap(), "0.2 → 0.3 moved the exe to another folder");
    assert_eq!(status(&settings, &new), HookStatus::Installed);
    assert!(!repair(&settings, &new).unwrap());
}
