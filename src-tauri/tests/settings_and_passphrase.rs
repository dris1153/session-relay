use session_relay_lib::engine::error::Error;
use session_relay_lib::engine::key_setup::check_strength;
use session_relay_lib::engine::settings::{RepoRef, Settings};

#[test]
fn settings_default_when_missing_and_error_when_corrupt() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("settings.json");
    let loaded = Settings::load(&path).unwrap();
    assert_eq!(loaded.repo, None);
    assert!(loaded.autostart);

    // A corrupt file is reported, not silently replaced (the GUI decides to fall back).
    std::fs::write(&path, b"{corrupted: invalid json").unwrap();
    assert!(matches!(Settings::load(&path), Err(Error::Invalid(_))));
}

#[test]
fn settings_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("settings.json");
    let original = Settings {
        repo: Some(RepoRef { owner: "github".into(), name: "sessions".into() }),
        machine_name: "test-machine".into(),
        claude_home: "C:/Users/me/.claude".into(),
        workspace_roots: vec!["D:/work/one".into(), "D:/work/two".into()],
        language: Some("en".into()),
        autostart: false,
    };
    original.save(&path).unwrap();
    let loaded = Settings::load(&path).unwrap();
    assert_eq!(loaded.repo, original.repo);
    assert_eq!((loaded.machine_name, loaded.claude_home), (original.machine_name, original.claude_home));
    assert_eq!(loaded.workspace_roots, original.workspace_roots);
    assert_eq!((loaded.language, loaded.autostart), (original.language, original.autostart));
}

#[test]
fn passphrase_strength_gate() {
    for weak in ["", "password", "password123"] {
        assert!(matches!(check_strength(weak), Err(Error::WeakPassphrase)), "{weak:?} must be rejected");
    }
    check_strength("correct horse battery staple ferry").unwrap();
}
