use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

pub const TEMP_SUFFIX: &str = ".sr-tmp";

/// `Append` files only grow (transcripts); `Whole` files are replaced as a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileClass {
    Append,
    Whole,
}

static ALLOWED: LazyLock<[Regex; 4]> = LazyLock::new(|| {
    [
        r"^[0-9a-f-]{36}\.jsonl$",
        r"^[0-9a-f-]{36}/subagents/agent-[A-Za-z0-9]{1,64}\.(jsonl|meta\.json)$",
        r"^[0-9a-f-]{36}/tool-results/[A-Za-z0-9._-]{1,160}$",
        r"^memory/[A-Za-z0-9._-]{1,128}\.md$",
    ]
    .map(|p| Regex::new(p).expect("static regex"))
});

/// Relative paths (with `/`) we are willing to read or write inside a Claude project dir.
/// Guards restore against path traversal from a tampered manifest.
pub fn classify(rel: &str) -> Option<FileClass> {
    if !ALLOWED.iter().any(|re| re.is_match(rel)) || rel.split('/').any(is_unsafe_component) {
        return None;
    }
    Some(if rel.ends_with(".jsonl") { FileClass::Append } else { FileClass::Whole })
}

fn is_unsafe_component(part: &str) -> bool {
    const RESERVED: [&str; 4] = ["con", "prn", "aux", "nul"];
    let stem = part.split('.').next().unwrap_or("").to_ascii_lowercase();
    let numbered = (stem.starts_with("com") || stem.starts_with("lpt"))
        && stem.len() == 4
        && stem.as_bytes()[3].is_ascii_digit();
    part.is_empty() || part.ends_with('.') || RESERVED.contains(&stem.as_str()) || numbered
}

/// Syncable files under a project dir; memory is included only for the repo-root key.
pub fn list(project_dir: &Path, include_memory: bool) -> Vec<(String, FileClass)> {
    let mut out = Vec::new();
    collect(project_dir, "", 0, &mut out);
    out.retain(|(rel, _)| include_memory || !rel.starts_with("memory/"));
    out.sort();
    out
}

fn collect(dir: &Path, prefix: &str, depth: usize, out: &mut Vec<(String, FileClass)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() && depth < 2 {
            collect(&entry.path(), &rel, depth + 1, out);
        } else if kind.is_file() && !name.ends_with(TEMP_SUFFIX) {
            if let Some(class) = classify(&rel) {
                out.push((rel, class));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SID: &str = "0dc9e69a-c29f-4595-9d8f-ce29ae1c2504";

    #[test]
    fn allowlist_and_classes() {
        assert_eq!(classify(&format!("{SID}.jsonl")), Some(FileClass::Append));
        assert_eq!(classify(&format!("{SID}/subagents/agent-a1bb4f8c31c8031b1.jsonl")), Some(FileClass::Append));
        assert_eq!(classify(&format!("{SID}/subagents/agent-a1bb4f8c31c8031b1.meta.json")), Some(FileClass::Whole));
        assert_eq!(classify(&format!("{SID}/tool-results/mcp-claude_ai_Figma-get_metadata-1789.txt")), Some(FileClass::Whole));
        assert_eq!(classify("memory/MEMORY.md"), Some(FileClass::Whole));
        for bad in ["../settings.json", "memory/../../x.md", "memory/a.md/..", "x.jsonl", &format!("{SID}/tool-results/CON"),
            &format!("{SID}/tool-results/nul.txt"), &format!("{SID}/tool-results/a."), &format!("{SID}/tool-results/a:b"), "C:/x.jsonl"]
        {
            assert_eq!(classify(bad), None, "{bad}");
        }
    }

    #[test]
    fn lists_only_allowed_files() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        std::fs::create_dir_all(d.join(SID).join("tool-results")).unwrap();
        std::fs::create_dir_all(d.join("memory")).unwrap();
        for f in [format!("{SID}.jsonl"), format!("{SID}/tool-results/a.txt"), "memory/MEMORY.md".into(), "notes.txt".into(), format!("{SID}.jsonl{TEMP_SUFFIX}")] {
            std::fs::write(d.join(&f), b"x").unwrap();
        }
        let names: Vec<_> = list(d, true).into_iter().map(|(r, _)| r).collect();
        assert_eq!(names, vec![format!("{SID}.jsonl"), format!("{SID}/tool-results/a.txt"), "memory/MEMORY.md".to_string()]);
        assert_eq!(list(d, false).len(), 2);
    }
}
