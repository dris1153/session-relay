//! Finds local checkouts of GitHub repos under the user's workspace folders, so a machine can
//! link the cloud projects it already has cloned.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::project_identity::locate;

const MAX_DEPTH: usize = 4;
/// Folders visited per scan: a root like the user profile must not walk for minutes.
const MAX_VISITS: usize = 20_000;
const SKIP: [&str; 5] = ["node_modules", "target", "dist", "build", "AppData"];

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Checkout {
    /// `github.com/owner/repo`, the same form as a project key's remote.
    pub remote: String,
    pub path: PathBuf,
}

pub fn scan(roots: &[PathBuf]) -> Vec<Checkout> {
    let (mut found, mut budget) = (Vec::new(), MAX_VISITS);
    for root in roots {
        walk(root, 0, &mut budget, &mut found);
    }
    found.sort();
    found.dedup();
    found
}

fn walk(dir: &Path, depth: usize, budget: &mut usize, found: &mut Vec<Checkout>) {
    if *budget == 0 {
        return;
    }
    *budget -= 1;
    if dir.join(".git").exists() {
        if let Some(location) = locate(dir) {
            found.push(Checkout { remote: location.key.remote, path: dir.to_path_buf() });
        }
        // Folders inside a checkout belong to it (submodules, vendored repos), unless the root
        // itself is one (a home folder kept in git).
        if depth > 0 {
            return;
        }
    }
    if depth >= MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // `is_dir` of a symlink or junction is false: no loops through links.
        if !name.starts_with(['.', '$']) && !SKIP.contains(&name.as_str()) && entry.file_type().is_ok_and(|t| t.is_dir()) {
            walk(&entry.path(), depth + 1, budget, found);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(dir: &Path, url: &str) {
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(dir.join(".git").join("config"), format!("[remote \"origin\"]\n\turl = {url}\n")).unwrap();
    }

    #[test]
    fn finds_checkouts_and_skips_what_is_not_a_project() {
        let root = tempfile::tempdir().unwrap();
        let r = root.path();
        repo(&r.join("app"), "git@github.com:Acme/App.git");
        repo(&r.join("app").join("vendored"), "https://github.com/other/lib.git");
        repo(&r.join("clients").join("site"), "https://github.com/acme/site");
        repo(&r.join("web").join("node_modules").join("pkg"), "https://github.com/npm/pkg.git");
        repo(&r.join(".cache").join("x"), "https://github.com/hidden/x.git");
        repo(&r.join("a").join("b").join("c").join("d").join("deep"), "https://github.com/too/deep.git");
        repo(&r.join("gitlab"), "https://gitlab.com/acme/app.git");
        let found = scan(&[r.to_path_buf()]);
        let remotes: Vec<&str> = found.iter().map(|c| c.remote.as_str()).collect();
        assert_eq!(remotes, vec!["github.com/acme/app", "github.com/acme/site"]);
        assert_eq!(found[0].path, r.join("app"));

        let home = tempfile::tempdir().unwrap();
        repo(home.path(), "https://github.com/me/dotfiles");
        repo(&home.path().join("proj"), "https://github.com/me/proj");
        repo(&home.path().join("$Recycle.Bin").join("old"), "https://github.com/me/old");
        let found: Vec<String> = scan(&[home.path().to_path_buf()]).into_iter().map(|c| c.remote).collect();
        assert_eq!(found, vec!["github.com/me/dotfiles", "github.com/me/proj"], "a root that is a checkout is still walked");
    }

    /// Manual timing on a real folder: `SR_SCAN_ROOT=D:\Workspace cargo test scan_real -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn scan_real() {
        let root = PathBuf::from(std::env::var("SR_SCAN_ROOT").expect("SR_SCAN_ROOT"));
        let started = std::time::Instant::now();
        let found = scan(&[root]);
        println!("{} checkouts in {:?}", found.len(), started.elapsed());
    }
}
