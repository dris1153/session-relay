//! The Claude Code version in use here, for the "not tested with this version" notice: the
//! session layout this app reads is undocumented and may change between releases.

use std::path::Path;

use serde::Deserialize;

/// Tested with 2.1.278–2.1.280. Patch releases ship almost daily and are assumed compatible.
const TESTED_MINOR: (u32, u32) = (2, 1);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistryEntry {
    version: String,
    #[serde(default)]
    started_at: u64,
}

/// Version of the most recently started Claude session (`<claude_home>/sessions/<pid>.json`),
/// if its minor version is newer than the tested one.
pub fn untested_version(registry_dir: &Path) -> Option<String> {
    let newest = std::fs::read_dir(registry_dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| serde_json::from_slice::<RegistryEntry>(&std::fs::read(e.path()).ok()?).ok())
        .max_by_key(|e| e.started_at)?;
    let mut parts = newest.version.split('.').map(|p| p.parse::<u32>().ok());
    let minor = (parts.next()??, parts.next()??);
    (minor > TESTED_MINOR).then_some(newest.version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_only_a_newer_minor_of_the_latest_session() {
        let dir = tempfile::tempdir().unwrap();
        let write = |name: &str, body: serde_json::Value| std::fs::write(dir.path().join(name), body.to_string()).unwrap();
        assert_eq!(untested_version(dir.path()), None);
        write("1.json", serde_json::json!({ "version": "2.2.0", "startedAt": 1 }));
        write("2.json", serde_json::json!({ "version": "2.1.300", "startedAt": 2 }));
        write("3.json", serde_json::json!({ "garbage": true }));
        assert_eq!(untested_version(dir.path()), None, "the newest session runs a tested minor");
        write("4.json", serde_json::json!({ "version": "3.0.1", "startedAt": 3 }));
        assert_eq!(untested_version(dir.path()).as_deref(), Some("3.0.1"));
    }
}
