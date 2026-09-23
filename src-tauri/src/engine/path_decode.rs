use std::path::{Path, PathBuf};

use super::paths::encode_dir;

const MAX_DEPTH: usize = 16;

/// Existing directories whose Claude-encoded name is `enc` (case-insensitive).
/// Used when a project dir has no transcripts left to read a `cwd` from (e.g. only `memory/`
/// survived Claude's cleanup). Encoding is lossy, so callers must reject ambiguous results.
pub fn existing_dirs_for(enc: &str) -> Vec<PathBuf> {
    let b = enc.as_bytes();
    // `X:\` encodes to `X--`; names over 200 chars carry a hash suffix and cannot be walked.
    if enc.len() > 200 || b.len() < 3 || !b[0].is_ascii_alphabetic() || &enc[1..3] != "--" {
        return Vec::new();
    }
    let mut out = Vec::new();
    walk(&PathBuf::from(format!("{}:\\", &enc[..1])), &enc[3..], 0, &mut out);
    out
}

fn walk(dir: &Path, rest: &str, depth: usize, out: &mut Vec<PathBuf>) {
    if rest.is_empty() {
        out.push(dir.to_path_buf());
        return;
    }
    if depth >= MAX_DEPTH || out.len() > 1 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())) {
        let name = encode_dir(&entry.file_name().to_string_lossy());
        let Some(head) = rest.get(..name.len()) else { continue };
        if !head.eq_ignore_ascii_case(&name) {
            continue;
        }
        match &rest[name.len()..] {
            "" => walk(&entry.path(), "", depth + 1, out),
            after => {
                if let Some(next) = after.strip_prefix('-') {
                    walk(&entry.path(), next, depth + 1, out);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_existing_dir_from_lossy_name() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("my.app-v2").join("web");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::create_dir_all(tmp.path().join("other")).unwrap();
        let enc = encode_dir(&target.to_string_lossy());
        let found = existing_dirs_for(&enc);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].ends_with(Path::new("my.app-v2").join("web")));

        // `a-b` and `a.b` encode identically: ambiguous → both reported.
        std::fs::create_dir_all(tmp.path().join("a-b")).unwrap();
        std::fs::create_dir_all(tmp.path().join("a.b")).unwrap();
        assert_eq!(existing_dirs_for(&encode_dir(&tmp.path().join("a-b").to_string_lossy())).len(), 2);
        assert!(existing_dirs_for("not-a-drive").is_empty());
    }
}
