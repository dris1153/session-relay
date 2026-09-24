use std::path::{Path, PathBuf};
use std::time::Duration;

use super::error::{Error, IoContext, Result};
use super::fs_util::remove_lock_files;
use super::git_batch;
use super::git_process::{failure, GitEnv, Output};
use super::progress::{Progress, ProgressSink};

const LOCAL: Duration = Duration::from_secs(60);
const WORKTREE: Duration = Duration::from_secs(600);
const NETWORK: Duration = Duration::from_secs(180);
/// Transfers of any size: git is killed only after this long without progress output.
const STALL: Duration = Duration::from_secs(120);
const LAST_SNAPSHOT: &str = "refs/session-relay/last";

/// Local clone of the storage repo. Remote history is always one orphan snapshot commit.
/// Callers must hold `SyncLock` for anything that writes the clone.
pub struct StoreRepo {
    dir: PathBuf,
    remote_url: String,
    git: GitEnv,
    pub progress: ProgressSink,
}

impl StoreRepo {
    pub fn new(dir: PathBuf, remote_url: String, git: GitEnv) -> Self {
        Self { dir, remote_url, git, progress: ProgressSink::default() }
    }

    fn transfer(&self, args: &[&str], step: fn(u8) -> Progress) -> Result<Output> {
        self.git.run_watched(&self.dir, args, STALL, &mut self.progress.git_reporter(step))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// `init` + `remote add` instead of `clone`: works for an empty remote (Spike D).
    pub fn ensure_clone(&self) -> Result<()> {
        std::fs::create_dir_all(&self.git.hooks_dir).at(&self.git.hooks_dir)?;
        if !self.dir.join(".git").exists() {
            std::fs::create_dir_all(&self.dir).at(&self.dir)?;
            self.git.check(&self.dir, &["init", "-q", "-b", "main"], LOCAL)?;
            self.git.check(&self.dir, &["remote", "add", "origin", &self.remote_url], LOCAL)?;
        } else {
            self.git.check(&self.dir, &["remote", "set-url", "origin", &self.remote_url], LOCAL)?;
        }
        Ok(())
    }

    /// Read-only probe: safe without the lock, never touches the clone.
    pub fn remote_head(&self) -> Result<Option<String>> {
        let out = self.git.check(&self.dir, &["ls-remote", "origin", "refs/heads/main"], NETWORK)?;
        Ok(String::from_utf8_lossy(&out).split_whitespace().next().map(str::to_owned))
    }

    /// Fetches the current snapshot; None when the remote has no `main` yet (told by fetch
    /// itself: one round trip to GitHub instead of `ls-remote` + `fetch`).
    pub fn fetch(&self) -> Result<Option<String>> {
        let args = ["fetch", "--progress", "--depth", "1", "--no-tags", "origin", "main"];
        let out = self.transfer(&args, |percent| Progress::Download { percent })?;
        if !out.status_ok && out.stderr.lines().any(|l| l.starts_with("fatal: couldn't find remote ref")) {
            self.remember(None)?;
            return Ok(None);
        }
        if !out.status_ok {
            return Err(failure(&args.join(" "), &out.stderr));
        }
        let sha = self.git.check(&self.dir, &["rev-parse", "FETCH_HEAD"], LOCAL)?;
        let sha = String::from_utf8_lossy(&sha).trim().to_string();
        self.remember(Some(&sha))?;
        Ok(Some(sha))
    }

    /// Records the newest snapshot this clone has seen, fetched or pushed by any process.
    fn remember(&self, sha: Option<&str>) -> Result<()> {
        match sha {
            Some(sha) => self.git.check(&self.dir, &["update-ref", LAST_SNAPSHOT, sha], LOCAL)?,
            None => self.git.check(&self.dir, &["update-ref", "-d", LAST_SNAPSHOT], LOCAL)?,
        };
        Ok(())
    }

    /// Newest snapshot known without the network: statuses stay right offline and after a
    /// save whose follow-up fetch could not run.
    pub fn last_snapshot(&self) -> Option<String> {
        let out = self.git.run(&self.dir, &["rev-parse", "-q", "--verify", &format!("{LAST_SNAPSHOT}^{{commit}}")], LOCAL).ok()?;
        out.status_ok.then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// None only when the path is absent from the snapshot; any git failure is an error
    /// (a read failure must never look like "the cloud has nothing here").
    pub fn show(&self, sha: &str, path: &str) -> Result<Option<Vec<u8>>> {
        let listed = self.git.check(&self.dir, &["ls-tree", sha, "--", path], LOCAL)?;
        if listed.is_empty() {
            return Ok(None);
        }
        self.git.check(&self.dir, &["cat-file", "blob", &format!("{sha}:{path}")], LOCAL).map(Some)
    }

    /// Several blobs of one snapshot through a single `cat-file --batch`. The paths come from a
    /// listing of that snapshot, so an absent one means a damaged clone: an error, never None.
    pub fn read_blobs(&self, sha: &str, paths: &[String]) -> Result<Vec<Vec<u8>>> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        let input: String = paths.iter().map(|p| format!("{sha}:{p}\n")).collect();
        let out = self.git.run_with_input(&self.dir, &["cat-file", "--batch"], input.as_bytes(), LOCAL)?;
        let blobs = out.status_ok.then(|| git_batch::parse(&out.stdout, paths.len())).flatten();
        blobs.ok_or_else(|| Error::Git { args: "cat-file --batch".into(), stderr: format!("unexpected output {}", out.stderr.trim()) })
    }

    pub(super) fn run_git(&self, args: &[&str], timeout: Duration) -> Result<Output> {
        self.git.run(&self.dir, args, timeout)
    }

    /// Makes the worktree exactly the snapshot (or empty), materializing only `sparse` paths:
    /// other projects stay in the index, so the next commit keeps them without a disk copy.
    pub fn prepare_worktree(&self, sha: Option<&str>, sparse: &[String]) -> Result<()> {
        let mut set = vec!["sparse-checkout", "set", "--no-cone"];
        set.extend(sparse.iter().map(String::as_str));
        self.git.check(&self.dir, &set, WORKTREE)?;
        match sha {
            Some(sha) => self.git.check(&self.dir, &["reset", "-q", "--hard", sha], WORKTREE)?,
            None => self.git.check(&self.dir, &["read-tree", "--empty"], LOCAL)?,
        };
        self.git.check(&self.dir, &["clean", "-q", "-ffdx"], WORKTREE)?;
        Ok(())
    }

    /// Orphan commit of the index (no parent keeps the remote at one snapshot).
    pub fn snapshot_commit(&self) -> Result<String> {
        self.git.check(&self.dir, &["add", "-A"], WORKTREE)?;
        let tree = self.git.check(&self.dir, &["write-tree"], WORKTREE)?;
        let tree = String::from_utf8_lossy(&tree).trim().to_string();
        // Generic message: the plaintext commit must not reveal machine names.
        let commit = self.git.check(&self.dir, &["commit-tree", &tree, "-m", "snapshot"], LOCAL)?;
        Ok(String::from_utf8_lossy(&commit).trim().to_string())
    }

    /// Entries (`mode type sha<TAB>name`) directly under `dir` ("" = root) of a snapshot.
    pub fn ls_tree(&self, rev: &str, dir: &str) -> Result<Vec<String>> {
        let spec = if dir.is_empty() { rev.to_string() } else { format!("{rev}:{dir}") };
        let out = self.git.check(&self.dir, &["ls-tree", &spec], LOCAL)?;
        Ok(String::from_utf8_lossy(&out).lines().map(str::to_owned).collect())
    }

    /// Drops the clone and starts over; safe because it is only a cache of the remote.
    pub fn rebuild(&self) -> Result<()> {
        if self.dir.exists() {
            std::fs::remove_dir_all(&self.dir).at(&self.dir)?;
        }
        self.ensure_clone()
    }

    pub fn tree_paths(&self, commit: &str, dir: &str) -> Result<Vec<String>> {
        let out = self.git.check(&self.dir, &["ls-tree", "-r", "--name-only", commit, "--", dir], LOCAL)?;
        Ok(String::from_utf8_lossy(&out).lines().map(str::to_owned).collect())
    }

    /// Compare-and-swap push: fails with `LeaseRejected` if the remote moved since `expected`.
    pub fn push_lease(&self, commit: &str, expected: Option<&str>) -> Result<()> {
        let lease = format!("--force-with-lease=refs/heads/main:{}", expected.unwrap_or(""));
        let target = format!("{commit}:refs/heads/main");
        let out = self.transfer(&["push", "--progress", "--porcelain", &lease, "origin", &target], |percent| Progress::Upload { percent })?;
        let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), out.stderr);
        if out.status_ok {
            self.remember(Some(commit))
        } else if text.contains("stale info") || text.contains("[rejected]") || text.contains("fetch first") {
            Err(Error::LeaseRejected)
        } else {
            Err(failure("push", &out.stderr))
        }
    }

    /// Git `*.lock` files left by a killed run. Only call while holding `SyncLock`: then no git of ours is alive.
    pub fn clear_stale_locks(&self) {
        remove_lock_files(&self.dir.join(".git"), 0);
    }

    /// Rebuilds a broken clone (it is only a cache). Cheap; must run while holding `SyncLock`.
    /// Corruption it cannot see shows up as a git error later, which rebuilds the clone too.
    pub fn recover(&self) -> Result<()> {
        self.clear_stale_locks();
        if self.dir.join(".git").exists() && !self.git.run(&self.dir, &["rev-parse", "--git-dir"], LOCAL)?.status_ok {
            std::fs::remove_dir_all(&self.dir).at(&self.dir)?;
        }
        self.ensure_clone()
    }
}
