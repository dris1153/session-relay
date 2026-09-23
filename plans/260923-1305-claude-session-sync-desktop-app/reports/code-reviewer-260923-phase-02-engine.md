## Code Review Summary: Phase 2 Rust sync engine (adversarial)

### Scope
- Files: `src-tauri/src/engine/*.rs` (31 modules, ~2.6k LOC), `src-tauri/tests/{sync_roundtrip,store_recovery,edge_cases,real_data_discovery}.rs`
- Snapshot reviewed: working tree at 2026-09-23 15:54. `sync.rs`, `context.rs`, `error.rs`, `overview.rs` and `tests/edge_cases.rs` were edited during the review; line numbers match the latest version.
- Focus: data-loss invariants, git isolation, crash safety, concurrency, Windows pitfalls
- Verification: `cargo test` green (23 unit + 17 integration, 1 ignored); `cargo clippy --all-targets` 0 warnings. **Four data-loss scenarios were reproduced** with a throwaway crate in the session scratchpad that depends on the engine; no source files were touched. Git behavior (repo discovery, stale ref locks) was checked against Git 2.54 in scratch repos.

### Overall Assessment
The architecture is sound. Chunking is deterministic, the normalized stream keeps chunks machine-independent, pushes are lease-based (compare-and-swap), status reads use `git show`, the manifest has a MAC plus key binding plus an allowlist, and git runs in an isolated env. The code is compact and readable. However, the **Whole-file (memory/*.md) conflict path loses data silently in the default hook flow**, and the "loser backed up" promise from the plan is not implemented. Several fail-open patterns (`unwrap_or_default`, and `show()` treating an error as "absent") and an unguarded git repo discovery need fixing before Phase 5 hooks go live.

**Score: 6/10**. The design is 8/10. The Critical issue and the High issues are localized fixes.

---

### Critical

**C1. Whole-class last-writer-wins (LWW) overwrites the other machine's edit with no backup anywhere (reproduced, default PushOnly hook flow)**
- `sync_policy.rs:35-37`: `needs_cloud_backup` excludes `LocalAhead`, so a both-changed Whole file that wins LWW overwrites the cloud copy with no backup.
- `state.rs:72-75`: LWW ignores the base. If a local file has the *same content as base* but a newer mtime (rewritten with identical bytes, restored or copied), it becomes `LocalAhead` and pushes stale content over a newer cloud version.
- `state.rs:73` vs `sync.rs:140`: local **mtime** is compared against the remote **push time** (`saved_at = Utc::now()`), not the remote file's modification time, so the wrong side can win.
- On the losing machine the pull comes from the quick path (`state.rs:53`, `f.local == None`), so `needs_local_backup` (`sync_policy.rs:30`) is false and the loser is overwritten again with no backup.
- Repro 1: A and B are at v0. B edits and PushOnly pushes. A edits 1 s later and PushOnly pushes. B runs Auto. Result: B's file = A's note, **backups A=0 B=0**, and B's edit exists nowhere.
- Repro 2: B pushes v1. A rewrites identical v0 bytes (mtime bump) and runs PushOnly. B runs Auto. B's v1 is gone everywhere, 0 backups.
- The first-link case hits the same path: both machines already have `memory/MEMORY.md` before the second machine links.
- Fix:
  1. In `with_content` for Whole files: `local.hash == base.hash` → `RemoteAhead`, and `remote.hash == base.hash` → `LocalAhead`. Use LWW only when both sides differ from the base.
  2. Back up the cloud entry whenever pushing over a remote entry whose `hash != base.hash`, i.e. content this machine never saw. Pass the `FileEval` and base to `needs_cloud_backup`.
  3. In `PushOnly`, skip both-changed Whole files (reason `conflict`) and let the GUI decide.
  4. Add `modified_at` (source mtime) to `FileEntry` for LWW. Keep `saved_at` for display.
- Test gap: `tests/edge_cases.rs:130-140` (`memory_conflict_last_writer_wins_loser_backed_up`) only checks backups `if backups_dir.exists()`, so it passes when **no backup was made**. The tester report lists this scenario as covered, which is not true.

---

### High

**H1. Append quick-path `RemoteAhead` pulls non-prefix content with no local backup (reproduced), which breaks invariant #1**
- `state.rs:53`: `L==B, R!=B → RemoteAhead` without checking that R extends B. After a `ForceLocal` on another machine, or delete+re-push, the new R is not an extension of B. `needs_local_backup` is false for Append `RemoteAhead`.
- Repro: A and B diverge. A PushOnly pushes. B runs `ForceLocal` (the cloud backup lands only on B's disk). A runs Auto and pulls B's version: A's line `"only":"A"` is gone from A, and A has no backup.
- Fix: for Append with a local file present, return `None` from `quick` when `R != B`. This forces the content check (prefix-verified `RemoteAhead`, else `Diverged`) and costs one read only when the cloud changed. Alternatively, in `transfer::pull`, stream-compare the local bytes against the expanded tmp file and back up if they are not a prefix.

**H2. Git repo discovery can escape the store and act on a user's parent repo (verified with Git 2.54)**
- `git_process.rs:69` runs `git -C <store>` with no `GIT_DIR`, `GIT_WORK_TREE` or `GIT_CEILING_DIRECTORIES`. If `store/.git` exists but is invalid (for example `objects/` missing after a partial `remove_dir_all` in `recover()` at `store_repo.rs:122`, or a zero-length `HEAD` after power loss), git walks up to any ancestor repo. Checked: `git -C store rev-parse --show-toplevel` printed the parent repo.
- Chain if `C:\Users\<name>` is a dotfiles repo (or any ancestor of `%LOCALAPPDATA%\dev.sessionrelay.desktop\store`):
  1. `recover()`: `fsck` succeeds on the *parent*, so it looks healthy.
  2. `reflog expire --expire=now --all` and `gc --prune=now` run on the parent and destroy its reflog safety net.
  3. `ensure_clone` runs `remote set-url origin <storage>` on the parent.
  4. Sync runs `reset --hard <snapshot>` on the parent worktree, destroying uncommitted work.
- `tests/store_recovery.rs:285` simulates exactly this corruption (deletes `.git/objects`). It is only safe because the temp dir has no ancestor repo.
- Fix: in `GitEnv::configure` set `GIT_DIR=<store>\.git` and `GIT_WORK_TREE=<store>` (or `GIT_CEILING_DIRECTORIES=<store parent>`). In `recover`, treat "not a git repository" as unhealthy.

**H3. A missing manifest is treated as a brand-new project, which can shrink the cloud copy silently**
- `store_repo.rs:58-61`: `show()` maps *any* non-zero exit (bad object, unreadable pack, unknown sha) to `None`, the same as "path not in tree".
- `sync.rs:82`: the rollback check only runs when a manifest exists. `manifest == None` with `base.generation_seen > 0` is accepted.
- `sync.rs:131` then builds `Manifest::new(key)` with only the pushed files, and `commit_and_push` (`sync.rs:164-168`) deletes every other chunk of the project from the snapshot.
- Unchanged local files stay `RemoteDeleted` (base kept), so **they are never re-uploaded**. The same happens if the storage repo is recreated with the same key.
- Fix:
  - Make `show` distinguish "absent" from "error": use `git cat-file -e`, or check stderr for `does not exist`/`exists on disk, but not in`, and return `Err` otherwise.
  - In `sync_once`, if the manifest is `None` and `base.generation_seen > 0`, fail with a new code (`remote_missing`) and offer "re-upload all", which clears the base.

---

### Medium

**M1. A literal `{{SR_PROJECT_DIR}}` in a transcript is silently rewritten on restore (reproduced)**
- Files: `normalize.rs:7`, `normalize.rs:29-42`.
- Repro: A saves a line containing the literal text. B restores it as `C:\\Users\\...\\B\\claude\\projects\\...`. The hash check passes, because it runs over normalized chunks, so the file is corrupted with no signal.
- This project's own dev transcripts contain that literal.
- A related issue: the match has no boundary, so `…\\projects\\d--Work-app` also matches inside `…\\projects\\d--Work-app-v2\\…`.
- Fix:
  - Use a placeholder that valid JSONL cannot contain, for example with raw control bytes (`\x01SR_PROJECT_DIR\x01`), which JSON forbids unescaped inside strings. Or escape pre-existing occurrences on save.
  - Require the match to be followed by `\\` or `"`: capture the suffix and re-emit it.

**M2. A failed local backup still overwrites**
- `sync.rs:100`: `std::fs::read(&target).unwrap_or_default()`. If the read fails (sharing violation, permissions), an *empty* backup is saved and the pull overwrites the file.
- Fix: propagate the error with `?` and skip the file with reason `backup_failed`.

**M3. A stale `refs/remotes/origin/main.lock` permanently breaks fetch (verified)**
- `store_repo.rs:10`: `LOCK_FILES` omits it. Fetch then fails with exit 1 and "cannot lock ref" on every run.
- `recover()` does not fix it, because `fsck` still passes.
- The lock can be left behind by our own timeout `kill_tree` during fetch/push, or by a logoff kill. Push still returns 0 in this state.
- Fix: while holding SyncLock, walk `.git` and delete every `*.lock`.

**M4. One bad file aborts the whole project sync, and also the whole dashboard**
- Error sites:
  - `snapshot.rs:68-72`: a Whole file changing during the read is a hard error. The plan says "retry once else skip".
  - `evaluate.rs:62`: any `snapshot::take` error, such as a file deleted between listing and open.
  - `sync.rs:101-103` and `sync.rs:139`: pull/push IO or decrypt errors.
- Those errors propagate with `?` out of `sync_once`. Because pulls run first, one corrupt remote chunk blocks all pushes in Auto.
- `overview.rs:74` and `overview.rs:87` propagate them too, so one project's error fails `list` for every project.
- A hook PushOnly that races with Claude writing a tool-results or memory file fails the whole save.
- Fix: isolate errors per file (push to `report.skipped` with a reason and continue), retry Whole files once, and keep per-project errors in `ProjectSummary`.

**M5. Corrupt `base.json`/`links.json` silently resets to empty, and writes are not durable**
- `base.rs:30` and `links.rs:34` use `unwrap_or_default` on parse errors.
- `fs_util.rs:16`: `write_atomic` has no `sync_all` before the rename, so power loss can leave a zero-length file.
- An empty base turns every `LocalDeleted` file into `RemoteOnly`, and Auto then resurrects every session Claude cleaned up (possibly GBs). `RemoteDeleted` files get re-uploaded, and `generation_seen = 0` disables anti-rollback.
- Fix: call `File::sync_all` on the temp file before renaming. On a parse error, rename the file to `*.corrupt`, log it, and return an error. Do not default silently.

**M6. Links are lost between the GUI and the hook worker**
- `sync.rs:61` and `overview.rs:68` save each process's in-memory `Links` wholesale. A worker that loaded links before the user linked a repo in the GUI overwrites the new link on disk.
- Fix: reload `links.json` after acquiring SyncLock, merge, and save only when something changed.

**M7. Startup maintenance is heavy and runs at every login (autostart)**
- `store_repo.rs:120-126`: `fsck` + `reflog expire` + `gc --prune=now` rewrite the whole store pack (~420 MB estimated) on each launch while holding SyncLock. Hook workers that fire during this window time out.
- Fix: gc only above a pack-count or loose-object threshold, or `prune` only. Run it off the hot path.

**M8. Worktree cost and short local timeouts**
- `prepare_worktree` materializes *every* project's chunks. Disk use is about 2× the store (objects + checkout).
- `reset --hard`, `add -A` and `clean` use `LOCAL = 60 s` (`store_repo.rs:7`). On a slow disk with AV scanning, the first checkout can exceed that. The kill then leaves `index.lock` behind and the same timeout repeats on the next run.
- Fix: raise these timeouts to the PUSH class, or drop the worktree: build the tree with `read-tree <sha>` + `hash-object -w` + `update-index --cacheinfo` + `write-tree`. That also makes `clean -ffdx` unnecessary.

**M9. The live-session guard fails open when the registry schema changes, and logs nothing**
- `live_sessions.rs:25` drops any registry file that does not parse. A numeric `procStart` or a renamed field makes every session look "not live", so restores can land on a live session (Red Team #6).
- The plan requires a warning; none is logged.
- Fix: log the parse failure, accept `procStart` as either a string or a number, and when any registry file is unparsable, treat sessions with a recent mtime as live.

**M10. Backups load entire files into memory**
- `sync.rs:100`, `remote.rs:46-51`, `backup.rs:17`: the file, the zstd output and the age output are all held at once, so peak memory is about 3× a large transcript.
- Fix: stream the file through `zstd::Encoder` and `age::Encryptor` straight to disk.

---

### Low
- **L1** `transfer.rs:32-37`: an early `?` after the rename skips tmp cleanup at line 42, so an `.sr-tmp` is left until the next startup. A rename sharing violation (indexer/AV) aborts the whole sync.
- **L2** `sync.rs:139-143`: when only the mtime changed, the file is pushed with an identical hash, which costs a commit and a generation bump. Compare `snap.hash` with the old hash and update the base only.
- **L3** `overview.rs:105-119`: `delete_remote_session` has no rollback check, pushes even when nothing matched, and writes no activity entry.
- **L4** `sync.rs:54-60`: pulls done in an attempt that ended `LeaseRejected` are missing from the final `SyncReport`, because the report is rebuilt on each attempt.
- **L5** `manifest.rs:51-78`: the MAC is computed over a re-serialized struct, so a field added by a newer app version reads as `ManifestTampered` on older clients. `v` is never checked, and chunk-name/hash formats are not validated (`^[0-9a-f]{32}$`).
- **L6** `sync.rs:170`: the plaintext commit message leaks the machine name. The commit times also reveal when the user is active.
- **L7** `paths.rs:58`: the long-path (>200) hash suffix is case-sensitive. A link root spelled `D:` versus Claude's `d:` gives a different directory.
- **L8** `title.rs:24-25`: if the first user message content is an array (the VS Code form), every later `"type":"user"` line (tool results) is JSON-parsed on every snapshot. Try the first user line once and handle arrays of text blocks.
- **L9** `transfer.rs:72`: each pushed file is read twice (evaluate + push). The plan asked for a single pass, and a hook save of a 180 MB session reads 360 MB.
- **L10** `sync.rs:159-160`: the `.gitattributes` byte string spans a raw newline. With `core.autocrlf=true` in this repo it becomes CRLF on checkout. Use `\n`.
- **L11** `activity.rs:37-44`: `activity.jsonl` grows without bound, and `recent()` reads the whole file.
- **L12**: two plan security items are not implemented: refusing reparse points in parent directories on restore, and a component-wise `starts_with` check after `canonicalize`. Risk is low because the manifest is MAC'd and the allowlist is tight.
- **L13** `crypto.rs:71-74`: the scrypt work factor is not pinned; the plan requires logN ≥ 18. A very fast machine could calibrate above `MAX_SCRYPT_WORK_FACTOR = 22`, and the other machine then reports "wrong passphrase".
- **L14** `sync_policy.rs:10`: `ForceRemote` without a file filter resurrects every `LocalDeleted` file. Confirm this is intended.

### Test quality
- `edge_cases.rs:130-140`: the backup assertion is vacuous and hides C1.
- `edge_cases.rs:223-226`: `is_empty() || !is_empty()` is always true. The `LEASE_RETRIES` loop in `sync_project` is never exercised; no concurrent push is injected between fetch and push.
- `edge_cases.rs:9-45`: the test only asserts `is_ok()`. It writes into the root-key dir and then syncs the subpath key, so nothing is exercised.
- Plan fact-check (`phase-02-rust-sync-core.md`): some Success Criteria are ticked `[x]` without evidence. There is no 180 MB / 1.4 GB performance test. "Token never in argv/env" cannot be verified until the Phase 3 credential helper exists. The plan's "retry once else skip" (snapshot) and "parse failure → warning" (live_sessions) are also not implemented as written.

### Edge cases found while scouting
- A hard cut splitting a line during expansion is handled correctly (carry up to the last `\n`). It is tested by `single_line_over_32_mib_with_path_straddling_the_hard_cut`.
- Path allowlist: `..`/`.` are rejected via the trailing-dot rule, `:` is excluded (so no ADS), and CON/PRN/AUX/NUL/COM0-9/LPT0-9 are rejected. No traversal bypass found. Two entries differing only in case (`A.txt` vs `a.txt`) collide on NTFS and oscillate (cosmetic).
- `probe_for`/`extends` index math is safe: the `m == 0` and `n == 0` short-circuits guard the `len() - 1` cases.
- Concurrency: every git write takes the SyncLock (sync, refresh, list, delete, maintenance). Holding the lock across all of `list` delays hook saves but is correct.

### Positive observations
- Deterministic append chunking, and the normalized stream gives identical chunks across machines. Appends really do re-upload only the tail (tested).
- Orphan-snapshot plus `--force-with-lease` compare-and-swap. Every referenced chunk is checked in the new tree before push.
- Thorough git isolation: `GIT_*`/`SSH_*` env scrubbed, config passed via `GIT_CONFIG_COUNT`, an empty hooks dir, protocol allowlist, schannel.
- Manifest MAC, key binding and allowlist are checked at `open`. Chunk name and length are re-verified after decrypt. The new `check_identity` marker stops a second key from writing a parallel project set.
- Real git and real crypto in tests, no mocks. Small, focused modules; clippy is clean.

### Recommended actions (priority order)
1. C1: base-aware Whole rule, cloud backup whenever overwriting unseen content, PushOnly skips Whole conflicts, and a real assertion in the edge test.
2. H1: remove the quick `RemoteAhead` for Append when a local file exists, or add a prefix check with backup in `pull`.
3. H2: set `GIT_DIR`/`GIT_WORK_TREE` in `GitEnv::configure`.
4. H3: make `show` fail closed, and treat a missing manifest with `generation_seen > 0` as an error.
5. M2, M3, M5: remove the fail-open defaults, clean `*.lock` files properly, fsync before rename.
6. M1: collision-proof placeholder with a boundary check.
7. M4, M6-M10: per-file error isolation, merging links, lighter maintenance, streamed backups, logging registry parse failures.

### Metrics
- Tests: 40 pass, 1 ignored (real data). Coverage not measured.
- Linting: 0 clippy warnings.
- Reproductions: 4 data-loss scenarios confirmed (C1 ×2, H1, M1). 2 git behaviors confirmed (H2, M3).

### Unresolved questions
1. The real `~/.claude/sessions/<pid>.json` schema (type and unit of `procStart`) could not be checked: reading the registry was denied as PII. Please confirm it is a FILETIME string.
2. Is `ForceRemote` with no file filter meant to resurrect all `LocalDeleted` files?
3. Backups are encrypted and there is no restore command. Which phase owns "restore from backup"? Until one exists, "backed up" is not recoverable by the user.
4. Should the hook (PushOnly) ever push a Whole file when both sides changed?

---

## Re-verification (2026-09-23, after the fixes)

Method: the scratch crate was ported to the new API (`sync_project(engine, req)`, `overview::link_repo`, `prepare_worktree(sha, sparse)`). All original repros were re-run and new probes added (12 scenarios total). No source files were edited. `cargo test` passes (41 + 1 ignored) and clippy reports 0 warnings.

### Original repros
| # | Scenario | Result |
|---|---|---|
| C1a | Edit race: B pushes, A edits later | **Fixed.** A's PushOnly skips the conflict. A's Auto pushes and backs up B's version (A backups = 1). |
| C1b | Content unchanged, mtime bumped | **Fixed.** Nothing is pushed; B keeps v1. |
| H1 | ForceLocal from B, then Auto on A | **Fixed.** A reports `diverged` and keeps its line. An explicit ForceRemote backs up first (backups 0 → 1). An unchanged local file is still pulled when the cloud really extends it. |
| H2 | Parent repo with `origin`, a commit and uncommitted work; store `.git/objects` deleted | **Fixed.** Sync fails with `git_failed` and does not escape the store. `on_startup` rebuilds the clone and the next sync pushes. The parent's worktree, `origin` URL and reflog are untouched. |
| H3 | Clone objects corrupted (read-only bit cleared first) | **Fixed (fails closed).** `remote::manifest` returns an error; sync fails at fetch and the remote does not move. A vanished manifest gives `RollbackDetected` (existing test). |
| M1 | Literal `{{SR_PROJECT_DIR}}` and `\u0001SR_PROJECT_DIR\u0001` in content | **Fixed.** Restored byte-for-byte. |

The other listed fixes were checked by reading the code. Nothing wrong was found in M2, M3, M5, M6, M9 (logging only) or L1-L6, L8, L10-L14. The vacuous tests are gone, and `tests/conflicts.rs` asserts on decrypted backup content.

### Sparse checkout + `add -A`: are other projects' entries kept?
Yes, in every path tested on Git 2.54:
- Two projects pushed in turn from one machine: both manifests plus the marker and `.gitattributes` are in the tree.
- A fresh second machine pushes a different project: the first project is kept.
- The first push to an unborn remote: `read-tree --empty` + `clean -ffdx` publish only this project plus the marker, which is correct because the remote is empty.
- **Dirty leftovers:** a failed push of P1 left a garbage `manifest.age` and an untracked chunk. The next push of P2 left P1's blobs byte-identical in the published tree, and P1 then synced normally.

Reasons it holds:
- `add -A` does not stage deletions of skip-worktree entries.
- `reset --hard` rewrites the whole index from `sha`.
- `clean -ffdx` removes untracked leftovers.

Residual risks (Low):
1. `sparse-checkout set --no-cone` needs a newer Git than the plan's ≥ 2.32. Cone mode became the default and non-cone mode was deprecated in Git 2.37. Put the real minimum into the Phase 3 git preflight.
2. Skip-worktree handling has changed between Git versions, for example how files present despite skip-worktree are treated.
3. A cheap guard would make this independent of Git version. Before `push_lease`, compare `git ls-tree <sha>` with `git ls-tree <commit>`, and `ls-tree <sha> p/` with `ls-tree <commit> p/`, excluding `p/<key_hash>`. Fail closed if any other entry differs.
4. `sparse-checkout set` still runs with the 60 s `LOCAL` timeout (`store_repo.rs:79`), but it also materializes files. Use `WORKTREE`.

### New / remaining issues
**N1 (Medium, regression from M7): a corrupt clone stays broken for up to 7 days.**
- Repro: sync → `on_startup` (writes the gc stamp) → corrupt objects → sync = `git_failed` → `on_startup` = Ok → sync = `git_failed` again.
- Cause: `recover()` (`store_repo.rs:128-142`) only rebuilds when `rev-parse --git-dir` fails. fsck now runs weekly, and a clone with valid structure but corrupt objects passes `rev-parse`.
- Fix: when fetch, reset or cat-file fails with object errors (`corrupt`, `unable to unpack`, `bad object`, `inflate`), delete the stamp or rebuild the clone right away. The clone is only a cache.

**N2 (Medium, M4 only partly done): pull and push errors still abort the whole project.**
- `sync.rs:116` (`transfer::pull(...)?`), `sync.rs:147` (cloud backup) and `sync.rs:150` (`transfer::push(...)?`) propagate errors.
- One corrupt remote chunk, one hash mismatch, or one rename sharing violation (indexer/AV) stops all remaining pulls. Because pulls come first, it also stops every push in Auto.
- Fix: push per-file failures to `report.skipped` with a reason and continue. Keep only store/git-level errors fatal.

**N3 (Low, question): ForceLocal now pushes `RemoteDeleted`** (`sync_policy.rs:24`).
- This is needed to recover a vanished manifest. But a project-level "overwrite cloud" (without `only`) also re-uploads every session the user deliberately deleted from the cloud on any machine.
- This mirrors the L14 decision the other way round. Either limit it to the vanished-manifest case (`manifest == None`), or state it in the UI copy.

**N4 (Low): corrupt `base/<hash>.json` or `links.json` is now fail-closed (good), but there is no in-app way out.**
- A corrupt `links.json` blocks `list` and every sync.
- Offer "reset local state" in the UI, or move the file aside as `*.corrupt` after an explicit user action.

**N5 (Low, informational): a backup lives only on the machine that resolved the conflict.**
- In C1a, B's edit is backed up on A only; B pulls A's version without a local backup, because B's copy equals its base.
- This is acceptable, but a restore tool needs to say which machine holds the backup (open question 3).

**N6 (Low): performance.**
- `show()` now spawns two git processes per blob (`ls-tree` + `cat-file`), so pulling a 180 MB transcript (about 45 chunks) costs about 90 process spawns. Chunks may use `cat-file` directly, since a missing chunk is an error anyway, or `cat-file --batch`.
- Still open by agreement: M10 (backups in memory), L7, L9.

**N7 (Low, test gap): the `LEASE_RETRIES` loop in `sync_project` is still never exercised end to end.** Only `push_lease` is tested directly.

### Status
All Critical and High findings are fixed and verified by repro. No data-loss path remains reproducible. One new Medium regression (N1) and one Medium carry-over (N2) are left, both small, localized fixes.

**Score: 8/10** (was 6/10). It reaches 9 after N1 + N2 and the sparse tree guard.

Sources: [Highlights from Git 2.37 (GitHub Blog)](https://github.blog/open-source/git/highlights-from-git-2-37/), [git-sparse-checkout docs](https://git-scm.com/docs/git-sparse-checkout)
