use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::error::{Error, IoContext, Result};
use super::fs_util::write_atomic;
use super::path_decode::existing_dirs_for;
use super::paths::encode_dir;
use super::project_identity::{locate, ProjectKey};

/// Machine-local mapping. A dir's identity is recorded here once and never re-derived
/// from restored transcripts, whose `cwd` values belong to another machine.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Links {
    /// remote → local checkout root (one primary checkout per repo).
    pub repos: BTreeMap<String, PathBuf>,
    /// encoded Claude project dir name → key.
    pub dirs: BTreeMap<String, ProjectKey>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirRole {
    Primary(ProjectKey),
    /// Same repo as a linked checkout but another path (worktree or second clone): never synced.
    Secondary(ProjectKey),
    NoRemote,
}

impl Links {
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| Error::Invalid(format!("{}: {e}", path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).at(path),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        write_atomic(path, &serde_json::to_vec_pretty(self).expect("serializable"))
    }

    /// Claude project dir for a key on this machine, if its repo is linked.
    pub fn dir_for(&self, key: &ProjectKey) -> Option<String> {
        let root = self.repos.get(&key.remote)?;
        let mut cwd = root.clone();
        key.subpath.split('/').filter(|p| !p.is_empty()).for_each(|p| cwd.push(p));
        Some(encode_dir(&cwd.to_string_lossy()))
    }

    pub fn link_repo(&mut self, remote: &str, root: &Path) {
        self.repos.insert(remote.to_string(), root.to_path_buf());
    }

    /// Classifies one `projects/<enc>` dir, learning new links from transcripts written on this machine.
    pub fn classify_dir(&mut self, projects_dir: &Path, enc: &str) -> DirRole {
        if let Some(key) = self.dirs.get(enc).cloned() {
            return match self.dir_for(&key) {
                Some(d) if d.eq_ignore_ascii_case(enc) => DirRole::Primary(key),
                _ => DirRole::Secondary(key),
            };
        }
        let from_transcripts = cwd_candidates(&projects_dir.join(enc))
            .into_iter()
            .filter(|cwd| encode_dir(cwd).eq_ignore_ascii_case(enc) && Path::new(cwd).is_dir())
            .find_map(|cwd| locate(Path::new(&cwd)));
        // Only memory/ left (transcripts cleaned up): fall back to an unambiguous name match.
        let from_name = || match existing_dirs_for(enc).as_slice() {
            [only] => locate(only),
            _ => None,
        };
        let Some(loc) = from_transcripts.or_else(from_name) else {
            return DirRole::NoRemote;
        };
        match self.repos.get(&loc.key.remote) {
            Some(root) if !same_path(root, &loc.toplevel) => DirRole::Secondary(loc.key),
            _ => {
                self.link_repo(&loc.key.remote, &loc.toplevel);
                self.dirs.insert(enc.to_string(), loc.key.clone());
                DirRole::Primary(loc.key)
            }
        }
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_ascii_lowercase();
    norm(a) == norm(b)
}

const PROBE_BYTES: u64 = 64 * 1024;

/// Distinct `cwd` values from the head and tail of each transcript, most recent first.
fn cwd_candidates(dir: &Path) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return found };
    for path in entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "jsonl")) {
        for text in head_and_tail(&path).into_iter().rev() {
            for cwd in extract_cwds(&text).into_iter().rev() {
                if !found.contains(&cwd) {
                    found.push(cwd);
                }
            }
        }
    }
    found
}

fn head_and_tail(path: &Path) -> Vec<String> {
    let Ok(mut f) = std::fs::File::open(path) else { return vec![] };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let mut read_at = |offset: u64| {
        let mut buf = Vec::new();
        let ok = f.seek(SeekFrom::Start(offset)).is_ok() && (&mut f).take(PROBE_BYTES).read_to_end(&mut buf).is_ok();
        ok.then(|| String::from_utf8_lossy(&buf).into_owned())
    };
    [Some(0), (len > PROBE_BYTES).then(|| len - PROBE_BYTES)].into_iter().flatten().filter_map(&mut read_at).collect()
}

fn extract_cwds(text: &str) -> Vec<String> {
    text.match_indices("\"cwd\":")
        .filter_map(|(i, m)| {
            let rest = &text[i + m.len()..];
            let end = find_string_end(rest)?;
            serde_json::from_str::<String>(&rest[..end]).ok()
        })
        .collect()
}

/// Byte length of the JSON string literal at the start of `s` (quotes included).
fn find_string_end(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    (*bytes.first()? == b'"').then_some(())?;
    let mut i = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learns_primary_and_flags_secondary_and_restored_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let projects = tmp.path().join("projects");
        let repo = tmp.path().join("work").join("app");
        let clone2 = tmp.path().join("other").join("app");
        for r in [&repo, &clone2] {
            std::fs::create_dir_all(r.join(".git")).unwrap();
            std::fs::write(r.join(".git/config"), "[remote \"origin\"]\n url = https://github.com/acme/app.git\n").unwrap();
        }
        let write_session = |cwd: &Path| {
            let enc = encode_dir(&cwd.to_string_lossy());
            std::fs::create_dir_all(projects.join(&enc)).unwrap();
            let line = serde_json::json!({ "type": "user", "cwd": cwd.to_string_lossy() });
            std::fs::write(projects.join(&enc).join("s.jsonl"), format!("{line}\n")).unwrap();
            enc
        };
        let (enc1, enc2) = (write_session(&repo), write_session(&clone2));
        let mut links = Links::default();
        let key = ProjectKey { remote: "github.com/acme/app".into(), subpath: String::new() };
        assert_eq!(links.classify_dir(&projects, &enc1), DirRole::Primary(key.clone()));
        assert_eq!(links.classify_dir(&projects, &enc2), DirRole::Secondary(key.clone()));

        // A restored dir whose transcripts carry another machine's cwd is known only via `dirs`.
        let restored = encode_dir(&repo.join("pkg").to_string_lossy());
        std::fs::create_dir_all(projects.join(&restored)).unwrap();
        std::fs::write(projects.join(&restored).join("s.jsonl"), "{\"cwd\":\"C:\\\\Users\\\\PC\\\\app\\\\pkg\"}\n").unwrap();
        assert_eq!(links.classify_dir(&projects, &restored), DirRole::NoRemote);
        let pkg = ProjectKey { remote: key.remote.clone(), subpath: "pkg".into() };
        links.dirs.insert(restored.clone(), pkg.clone());
        assert_eq!(links.classify_dir(&projects, &restored), DirRole::Primary(pkg));
    }
}
