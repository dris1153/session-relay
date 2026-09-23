use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A synced unit: one GitHub repo plus the cwd subfolder Claude was started in ("" = repo root).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ProjectKey {
    pub remote: String,
    pub subpath: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoLocation {
    pub key: ProjectKey,
    pub toplevel: PathBuf,
}

/// `github.com/owner/repo` (lowercase) for any common GitHub remote URL form; None for other hosts.
pub fn normalize_remote(url: &str) -> Option<String> {
    let url = url.trim();
    let rest = url
        .strip_prefix("git@github.com:")
        .or_else(|| url.strip_prefix("ssh://git@github.com/"))
        .or_else(|| url.strip_prefix("git://github.com/"))
        .or_else(|| {
            let after_scheme = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
            let host_and_path = after_scheme.rsplit_once('@').map_or(after_scheme, |(_, h)| h);
            host_and_path.strip_prefix("github.com/")
        })?;
    let rest = rest.trim_end_matches('/');
    let rest = rest.strip_suffix(".git").unwrap_or(rest);
    let mut parts = rest.split('/');
    let (owner, repo) = (parts.next()?, parts.next()?);
    let valid = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    (parts.next().is_none() && valid(owner) && valid(repo)).then(|| format!("github.com/{owner}/{repo}").to_lowercase())
}

/// Finds the enclosing git checkout of `path` and its `origin` GitHub remote.
pub fn locate(path: &Path) -> Option<RepoLocation> {
    if path.to_string_lossy().starts_with(r"\\") {
        return None; // UNC paths could leak NTLM credentials when probed.
    }
    let toplevel = path.ancestors().find(|p| p.join(".git").exists())?.to_path_buf();
    let url = origin_url(&toplevel.join(".git"))?;
    let remote = normalize_remote(&url)?;
    let subpath = path
        .strip_prefix(&toplevel)
        .ok()?
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/");
    Some(RepoLocation { key: ProjectKey { remote, subpath }, toplevel })
}

fn origin_url(dot_git: &Path) -> Option<String> {
    let config = if dot_git.is_dir() {
        dot_git.join("config")
    } else {
        // Worktrees and submodules: `.git` is a file pointing at the real git dir.
        let pointer = std::fs::read_to_string(dot_git).ok()?;
        let git_dir = PathBuf::from(pointer.trim().strip_prefix("gitdir:")?.trim());
        let git_dir = if git_dir.is_absolute() { git_dir } else { dot_git.parent()?.join(git_dir) };
        match std::fs::read_to_string(git_dir.join("commondir")) {
            Ok(common) => git_dir.join(common.trim()).join("config"),
            Err(_) => git_dir.join("config"),
        }
    };
    parse_origin_url(&std::fs::read_to_string(config).ok()?)
}

fn parse_origin_url(config: &str) -> Option<String> {
    let mut in_origin = false;
    for line in config.lines().map(str::trim) {
        if line.starts_with('[') {
            in_origin = line.replace(' ', "").eq_ignore_ascii_case("[remote\"origin\"]");
        } else if in_origin {
            if let Some((k, v)) = line.split_once('=') {
                if k.trim().eq_ignore_ascii_case("url") {
                    return Some(v.trim().trim_matches('"').to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_remote_forms() {
        let want = Some("github.com/owner/repo".to_string());
        for url in [
            "git@github.com:Owner/Repo.git",
            "https://github.com/owner/repo",
            "https://github.com/owner/repo.git/",
            "https://user:x@github.com/owner/repo.git",
            "ssh://git@github.com/owner/repo.git",
        ] {
            assert_eq!(normalize_remote(url), want, "{url}");
        }
        assert_eq!(normalize_remote("https://gitlab.com/owner/repo"), None);
        assert_eq!(normalize_remote("https://github.com/owner"), None);
        assert_eq!(normalize_remote("https://github.com/owner/repo/extra"), None);
    }

    #[test]
    fn locates_repo_worktree_and_subpath() {
        let tmp = tempfile::tempdir().unwrap();
        let main = tmp.path().join("main");
        std::fs::create_dir_all(main.join(".git/worktrees/wt")).unwrap();
        std::fs::create_dir_all(main.join("apps/web")).unwrap();
        std::fs::write(main.join(".git/config"), "[core]\n\tbare = false\n[remote \"origin\"]\n\turl = git@github.com:Acme/App.git\n").unwrap();
        let loc = locate(&main.join("apps/web")).unwrap();
        assert_eq!(loc.key, ProjectKey { remote: "github.com/acme/app".into(), subpath: "apps/web".into() });
        assert_eq!(loc.toplevel, main);

        let wt = tmp.path().join("wt");
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(wt.join(".git"), format!("gitdir: {}", main.join(".git/worktrees/wt").display())).unwrap();
        std::fs::write(main.join(".git/worktrees/wt/commondir"), "../..\n").unwrap();
        assert_eq!(locate(&wt).unwrap().key.remote, "github.com/acme/app");
        assert!(locate(Path::new(r"\\server\share\repo")).is_none());
    }
}
