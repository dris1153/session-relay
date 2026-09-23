use std::path::PathBuf;

/// Must equal `identifier` in tauri.conf.json: Tauri's `app_local_data_dir()` is
/// `%LOCALAPPDATA%\<identifier>`, and headless subcommands run without a Tauri app handle.
pub const IDENTIFIER: &str = "dev.sessionrelay.desktop";

pub fn app_dir() -> PathBuf {
    PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default()).join(IDENTIFIER)
}

/// `credential.helper` value pointing at this very exe. Git runs it via sh: forward slashes, and
/// single quotes so a `$` or backtick in the profile path stays literal.
pub fn credential_helper() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let path = exe.to_string_lossy().replace('\\', "/").replace('\'', r"'\''");
    Some(format!("!'{path}' git-credential"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn identifier_matches_tauri_config() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(conf["identifier"], super::IDENTIFIER);
    }
}
