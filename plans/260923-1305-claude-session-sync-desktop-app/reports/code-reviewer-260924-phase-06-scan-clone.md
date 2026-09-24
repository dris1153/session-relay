# Code Review: Phase 6 (workspace scan, clone, conflict dialog)

Date: 2026-09-24 · Scope: uncommitted diff + untracked files (see phase-06 Implementation Notes)
Checks: `npx tsc --noEmit` clean · `cargo clippy --all-targets -D warnings` clean · `cargo test` all green (incl. `tests/git_clone.rs`) · locale parity en/vi: 0 missing keys, placeholders match · all touched files < 200 lines (dashboard-page.tsx 184).

**Score: 7.5/10.** The trust boundary is solid: the remote comes only from the cached view, the root is checked against the Settings list, and the webview never supplies a URL. The weak spots are partial-clone cleanup and shared clone state in the backend, plus a few UX paths that fail with no explanation.

## Critical
None.

## High

**H1. After a cancel, cleanup can silently leave a half clone, and the scan then offers to link it.** `engine/git_clone.rs:51-52`
- Failure: cancel runs `taskkill /F`, then `let _ = remove_dir_all(dest)`. Git's helpers (index-pack, git-remote-https), Defender or the search indexer can still hold a handle to the new pack file without share-delete. I checked this with a scratch program (rustc 1.98): if a handle is open without `FILE_SHARE_DELETE`, `remove_dir_all` returns OS error 32 and the folder stays. `git clone` writes `.git/config` with `remote.origin.url` before it fetches. So the next scan returns the leftover folder as "Found at …" with the [Link & restore] button, which links a broken checkout. A retry of the clone fails with `invalid_data` because the folder exists.
- Fix: retry the removal with backoff (e.g. 0/200/500/1000/2000 ms). If the folder is still there, at least remove `dest\.git` so the scan cannot treat it as a checkout, log it, and return a specific error code. Also: the test only covers a DNS failure, and git's own `remove_junk` cleans that case, so the test passes without our cleanup code. Add a test for cancel.

## Medium

**M1. The backend does not stop two clones from running at once, so the shared `RUNNING` slot breaks cancel and cleanup.** `git_clone.rs:16,38,50,60`
- Failure: clone A starts, then clone B (another IPC call). B overwrites `RUNNING` with its PID. A finishes and takes B's PID. B finishes, finds `None`, treats it as cancelled, deletes its own **successful** clone and returns `Cancelled`. `cancel()` can never reach A. If both calls target the same project, both pass `dest.exists()`; the second git fails with "already exists", and its cleanup runs `remove_dir_all` on the other call's in-progress folder. Today only the UI `running` ref prevents this.
- Fix: guard the whole function (an `AtomicBool`/state enum set with compare-exchange, released by a drop guard, returning `Error::Busy`). Take ownership of the folder atomically: `std::fs::create_dir(dest)` (fails on `AlreadyExists`) and clone into the empty folder we just created (git accepts an empty existing folder). Only a folder we created is ever deleted. This also closes the `exists()` → spawn TOCTOU window at `:24`.

**M2. An existing destination folder is a dead end with no explanation.** `git_clone.rs:24-25`, `commands/workspace.rs:36`
- Failure: `github.com/alice/app` and `github.com/bob/app` both map to `<root>\app`. Other triggers: the user's checkout has `origin` pointing at their fork, a non-git folder named like the repo, or a leftover (H1). Every case shows "Invalid data.", with no hint about what to do.
- Fix: add a dedicated `Error::DestinationExists` → code `destination_exists` with copy like "A folder with this name already exists in that workspace folder. Link it with 'Choose another folder…' or pick another workspace folder." Optionally fall back to `<repo>-<owner>`.

**M3. The scan and roots go stale after a Settings change (the plan requires a rescan when roots change).** `src/lib/use-workspace-scan.ts:14-16`
- Failure: the hook scans only when `checkouts === null`, i.e. once per dashboard lifetime. `DashboardPage` stays mounted while Settings is open. A user who sees "Add workspace folders in Settings…", adds one and comes back still sees the same message and no clone button, until they click "Scan again".
- Fix: add an `invalidate()` that sets `checkouts` back to `null`. Call it when `page` switches back to `"projects"` (a `useEffect` on `page` in dashboard-page.tsx), or after `save_settings` succeeds.

**M4. The conflict dialog hides per-file skip reasons, so a click can do nothing with no explanation.** `conflict-dialog.tsx:34-44`, `dashboard-page.tsx:157-163`
- Failure: with `force_remote` on a session that is open in Claude, the engine skips it as `session_open` (sync.rs:115). `backup_failed` and other codes are skipped the same way. The file stays in the list, and the reason is rendered in `ProjectDetailPane` notes behind the modal.
- Fix: pass `reports[project.key_hash]` into the dialog. Under each row, show `report.skipped.find(([r]) => r === file.rel)` using the same skip copy the pane uses (`skip.*` → `errorText`).

## Low
- **L1. Defense in depth on the git argv and the destination.** `git_clone.rs:28-29`, `workspace.rs:36`: add `"--"` before url/dest. Check that `project.name` is a single `Component::Normal` and that `dest.parent() == Some(&root)`. Check `normalize_remote(&format!("https://{remote}")) == Some(remote)`. A manifest remote is covered by the MAC but its format is never validated, and `project_view` takes `name` from `splitn('/')`, so a `\` inside it would escape the root.
- **L2. `cancel_clone` is a sync command.** `workspace.rs:47-50`: in Tauri v2 it runs on the main thread, and it spawns and waits on `taskkill` while holding the `RUNNING` lock. Make it `async` and move the work into `blocking`.
- **L3. The `cloning` flag is page-wide, and it stays on during the restore sync after the clone.** `dashboard-page.tsx:63-71`, `link-project-panel.tsx:48`: "Cancel clone" is shown on any other not_linked project's panel and during the restore sync, where it does nothing. Clicking before the PID is registered (`git_clone.rs:38`) also does nothing. Fix: call `setCloning(false)` right after `cloneProject` resolves, store the key hash (`cloning: string | null`), and record a cancel request that `clone_url` checks right after spawn.
- **L4. A user cancel shows as an error banner.** `use-dashboard.ts` `fail`: a user cancel shows up as "Cancelled.". Ignore the code `cancelled` in `clone`.
- **L5. Progress copy is wrong during a clone.** `workspace.rs:38`: the header says "Downloading storage… N%" while cloning the project repo, and stays at 100% through resolving deltas and checkout. Background storage fetches (also `download`) can mix into the same line. Consider a separate `clone` step.
- **L6. Git environment differs from `GitEnv`.** `git_clone.rs:27-35`: `GitEnv` sets `LC_ALL=C` because parsing depends on English output. The clone doesn't, so a localized git (MSYS2/Cygwin builds; this machine's Git for Windows 2.54 ships no locales) breaks `transfer_percent` and the `fatal:` filter. The error detail also drops `error:` lines (e.g. "RPC failed", "Filename too long"); keep the last few non-progress lines instead. Optionally add `git -c core.longpaths=true clone`.
- **L7. Quitting the app mid-clone leaves `git`/GCM orphaned.** Nothing kills the process tree on exit. Putting the child in a Job Object with `KILL_ON_JOB_CLOSE` would also make `taskkill` unnecessary.
- **L8. Scan cost and coverage.** `workspace_scan.rs:40-47`: there is no visit budget. A root like `C:\Users\me` walks `AppData`, `$Recycle.Bin`, `venv`/`vendor`. A root that is itself a checkout (home-dir dotfiles repo) stops the whole scan (`:31-35`). Overlapping roots with different casing produce duplicates that `dedup` misses. Cap the walk (e.g. 20k dirs), skip `AppData` and names starting with `$`, and only stop at a checkout below depth 0.
- **L9. The clone-root selection can go stale.** `link-project-panel.tsx:18`: `root` can hold a root that has since been removed from Settings; the backend then rejects it with `invalid_data`. Use `root && roots.includes(root) ? root : roots[0] ?? null`. The label `${cloneRoot}\\${name}` shows `D:\\app` for a drive root.
- **L10. Conflict dialog focus and target.** `conflict-dialog.tsx:14`: `showModal()` puts focus on the first "Keep this machine's" button, so Enter immediately forces local. Put `autoFocus` on Close. `resolving` is a boolean, so if the selected project changes (e.g. it disappears from the list) the dialog switches to another project's files. Store the key hash instead.
- **L11. Linking after a clone can overwrite a newer link.** `workspace.rs:41`: `link_repo` runs minutes after the cached `NotLinked` check. If `classify_dir` learned a link for this remote during the clone (the user opened Claude in another checkout), that link is overwritten and the other folder silently becomes Secondary. Re-check `links.repos` under the lock before linking.
- **L12. Cosmetic.** The clone folder name is the lowercased key (`App` → `app`). The header "Link…" duplicates the panel's "Choose another folder…".

## Edge cases checked (OK)
- The webview cannot choose the remote (key_hash → cached view). `root` is allow-listed, and Settings rejects non-absolute and UNC roots (`is_local_absolute`), so dest never starts with `-`.
- No `--recurse-submodules`. `GIT_TERMINAL_PROMPT=0` plus stdin null plus `CREATE_NO_WINDOW`: git and https never hang on a prompt. GCM can still show its GUI, and a hang there can be cancelled.
- `taskkill /T` is required and correct: `cmd\git.exe` is a launcher that spawns `mingw64\bin\git.exe`. PID reuse is impossible because the `Child` handle outlives the moment `RUNNING` is cleared.
- The stderr loop cannot deadlock (stdout goes to null). The dedup on `last` percent is fine.
- Scan: junctions and symlinks are not followed (`FileType::is_dir` is false for name-surrogate reparse points), permission errors are skipped silently, and the scan never descends into a checkout. Worktree and submodule `.git` files work through `locate`.
- Per-file force: `only=[rel]` limits the sync to that file. `needs_local_backup`/`needs_cloud_backup` back up the discarded side, and a backup failure skips the file instead of overwriting it. `sync_project` always publishes, so the dialog list refreshes.
- Monorepo subpath keys share one repo link (`dir_for`), so a clone links every subpath project at once.

## Positive observations
- The clone module is small and readable, with comments that explain *why* (GCM, the half-clone comment, the UNC note).
- `FileRow.local_size/local_modified` reuse `local_meta` from the evaluation, so no extra IO.
- The UI follows existing patterns (`ConfirmDialog` structure, `run` busy handling, DESIGN tokens: soft-stone nested card, 8px controls, caption headings, the global `select` cursor rule).
- Plan and IPC contract are updated (`clone_project`, `cancel_clone`, `Checkout`).

## Plan compliance
- Todo items match the code. The tray rule is correctly marked deferred. "Rescan when panel opens or roots change" is **not** met (M3). The planned `tests/workspace_scan.rs` became a unit test (fine). The success criteria are still manual and unchecked.

## Recommended actions (order)
1. H1 + M1 together: create `dest` ourselves, add a single-clone guard, retry the removal, and neutralize `.git` if it remains; add a cancel test.
2. M2 `destination_exists` code + copy (en/vi).
3. M4 skip reasons inside the dialog.
4. M3 rescan on return from Settings.
5. L1–L4 (cheap).

## Unresolved questions
- What should happen when `<root>\<repo>` exists: offer `<repo>-<owner>`, or offer to link the existing folder?
- Should a user cancel be silent?
- Should the clone force `LC_ALL=C` and `core.longpaths=true`, or keep the user's git fully untouched?
- Should https→ssh `insteadOf` setups be supported (`BatchMode` would be needed to guarantee fail-fast)?
