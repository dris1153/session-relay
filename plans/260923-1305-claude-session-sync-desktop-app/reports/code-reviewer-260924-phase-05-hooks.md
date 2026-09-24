## Code Review Summary: Phase 5 (auto-save through Claude Code hooks), adversarial

### Scope
- Rust (new): `hook.rs`, `pending.rs`, `auto_save.rs`, `engine/claude_hook_config.rs`, `tests/{claude_hook_config,hook_save}.rs`
- Rust (modified): `main.rs`, `lib.rs`, `tray.rs`, `watcher.rs`, `app_state.rs`, `dashboard.rs`, `commands/{settings,setup}.rs`, `engine/{overview,activity,mod}.rs`
- Frontend: `settings-page.tsx`, `step-machine-and-roots.tsx`, `dashboard-page.tsx`, `project-detail-pane.tsx`, `tauri-commands.ts`, `locales/{en,vi}.json`
- Cross-checked: `sync.rs` + `sync_policy.rs` (PushOnly), `lock.rs`, `fs_util.rs`, `links.rs` (`classify_dir`), `project_identity.rs` (`locate` reads files, no git), `git_process.rs` (env sanitising), `git_credential.rs`/`auth.rs` (refresh under `auth.lock`), `storage_check.rs`, `store_repo.rs` (timeouts), `design-tokens.css`, the spike B section, and the Claude Code hooks docs (current Stop input schema).
- Verification: `npx tsc --noEmit` 0 errors · `cargo clippy --all-targets -D warnings` clean · `cargo test` all green (1 ignored) · locale keys 178/178. The GUI, `hook-save`/`hook-worker`, `~/.claude`, GitHub and Credential Manager were not touched.

### Overall Assessment
The core mechanics are well built:
- Exec form with no shell. Std-handle inheritance is cleared before spawning, and job breakaway has a fallback.
- One marker file per turn, so `clear_up_to` can never delete a newer marker, and debounce keys on the newest stamp.
- The transcript path is canonicalised and checked component-wise, with an `enc` whitelist before any disk use.
- A malformed Claude `settings.json` is never written, and `preserve_order` is on.
- PushOnly is preserved end to end: `should_pull` is false and conflicts/diverged files are skipped.

The weak spots are all at the edges:
- The worker bypasses the storage-safety guard.
- The 64 KB stdin cap conflicts with Claude's current Stop payload.
- Hooks are left orphaned after an uninstall or a `claude_home` change.
- Quit is not really bounded.
- Failure surfacing can get stuck on, or be noticed late.

**Score: 7/10.** With H1–H3 and M1–M3 fixed: 8.5+.

---

### Critical
None.

### High

**H1. Hook workers keep pushing after the GUI has stopped syncing because storage is unsafe (public or foreign repo)**
- `hook.rs:115-119` (`cached_engine`), `watcher.rs:71-76`, `commands/setup.rs` `check_storage`.
- When the daily re-check finds `RepoPublic`, `RepoForeign` or similar, the GUI only calls `drop_engine()` in memory. Nothing is persisted. `hook-worker` builds its own engine from `settings.repo` plus the cached identity and never checks storage state. After the user's storage repo turns public (or is transferred), every Claude turn still commits and pushes to it. The content is age-encrypted, but this silently defeats a guard the app treats as mandatory ("stops syncing until onboarding passes again"). `WrongIdentity`/`StoreReset` are caught by `check_identity`; public and foreign are not.
- Fix: persist the block. Write `app_dir/storage-blocked` in `recheck_storage`/`check_storage` when `state != Ready` and delete it on `Ready`. In `hook::worker` and `save_pending`, return early and keep the marker when that file exists. A cheaper alternative with a network cost: have the worker run `storage_check` at most once per N hours, cached in a timestamp file.

**H2. Turns with a long assistant reply are never marked: the 64 KB stdin cap truncates Claude's Stop payload**
- `hook.rs:25,47-49`.
- The current Claude Code Stop input includes `last_assistant_message`, the full text of the response (per the hooks docs; the spike measured ~1 KB payloads before or without this field). Any reply over ~64 KB (fewer characters for Vietnamese or other multi-byte text) produces truncated JSON. `from_str` fails, so no marker is written and no worker starts. If that is the last turn before VS Code closes, the turn is not auto-saved until the next turn or a manual save. The failure goes only to the log. `read_to_string` also errors if the cut lands inside a UTF-8 sequence. Leaving stdin unread can also give Claude an EPIPE on its write (unverified whether Claude surfaces it).
- Fix: raise the cap (e.g. 16 MiB; serde already ignores unknown fields), or deserialise as a stream with `serde_json::from_reader(stdin.take(CAP))` into `HookInput`. Either way, drain the rest with `io::copy(&mut stdin, &mut io::sink())`. Add a unit test with a 200 KB `last_assistant_message`.

**H3. Orphaned hooks: uninstalling the app, or changing `claude_home`, leaves hooks that break or silently no-op on every Claude turn**
- No NSIS pre-uninstall cleanup (`tauri.conf.json` bundle has no `installerHooks`). `commands/settings.rs:44,58` lets `claude_home` change without migrating hooks.
- After uninstall, `settings.json` still runs `C:/…/session-relay.exe hook-save`. Claude shows a "Stop hook error" notice on every turn of every session until the user edits JSON by hand. After a `claude_home` change, the old home keeps our hooks. `hook-save` then rejects every transcript, because it validates against the new home. The settings UI shows "off" for the new home and can never remove the old entries.
- Fix: add a headless `hook-uninstall` subcommand (it reads app settings and calls `claude_hook_config::uninstall`) and call it from the NSIS `NSIS_HOOK_PREUNINSTALL` macro. In `save_settings`, when `claude_home` changes and the old status was Installed or StalePath, uninstall from the old home and install into the new one, and report failures.

### Medium

**M1. Silent stale-path "repair" ping-pongs between the dev and installed exe and rewrites the user's Claude config on every launch**
- `auto_save.rs:38-44`, `lib.rs` setup.
- `target/debug/session-relay.exe` and the installed exe share `IDENTIFIER`, so they share app settings and Claude home. Each launch sees the other's path as StalePath and rewrites `settings.json`, adding a new `.bak-*` each time. It can leave hooks pointing at a dev exe that `cargo clean` later deletes, which then produces an error notice every turn. Partial registrations also count as StalePath, e.g. when the user deliberately removed only `SessionEnd` in `/hooks`; startup silently re-adds both.
- Fix: repair only when the registered exe path no longer exists (or its file identity differs), skip repair under `cfg!(debug_assertions)`, and do not auto-repair partial registrations.

**M2. Failure surfacing: the tray can stay in attention for good, and worker failures are noticed late**
- `dashboard.rs:53-54,112`, `activity.rs:58-64`, `watcher.rs:31-46`.
- (a) `needs_attention()` is true whenever `auto_save_failures` is non-empty, even for key hashes absent from `view.projects`. Examples: a hook save that hit `not_linked` right after an unlink, a project removed from the cloud, or keys re-created. No project pane shows the note, and no later success for that key clears it, so the tray attention never clears. Fix: `view.auto_save_failures.retain(|(k, _)| view.projects.iter().any(|p| &p.key_hash == k))` before `set_attention`.
- (b) A worker failure in another process changes neither the remote head nor `last_snapshot`, so the watcher does not publish. For non-retry errors the marker is already cleared, so `retry_stale` has nothing to do either. The tray (the only signal while the window is hidden) stays calm until an unrelated refresh. Fix: have the watcher track the activity file's `(len, mtime)` and publish, or at least recompute failures and call `set_attention`, when it changes.

**M3. "Quit finishes all markers (≤30 s)" is not bounded, not re-entrant, and gives no feedback**
- `tray.rs:78-86`, `auto_save.rs:47-59`.
- The deadline is only checked between projects. Each `save_pending` can take 10 s (classify lock) + 10 s (sync lock) + fetch/push up to the 180 s `NETWORK` timeout (120 s stall). On a bad network, Quit does nothing visible for minutes. A second click starts a second flush thread that competes for the lock.
- Fix: put an `AtomicBool` guard on quit, hide the window or set the tooltip to "Saving…", run the flush on a worker thread and `recv_timeout(30 s)` on a channel, then `app.exit(0)` regardless. Killed git children are cleaned by `clear_stale_locks`, and markers stay for the next start.

**M4. Writing Claude's `settings.json` replaces a symlink with a plain file, and backups pile up**
- `claude_hook_config.rs:124-132` (`write_atomic` renames over the path).
- Users who keep `~/.claude/settings.json` as a symlink into a dotfiles repo lose the link: the rename replaces the link, not its target. From then on, dotfile edits no longer reach Claude and our edit never reaches the dotfiles.
- Backups: every write adds `settings.json.bak-%Y%m%d-%H%M%S` to `~/.claude` with no pruning. Two writes in the same second overwrite the first backup, e.g. tray on/off quickly, or onboarding's `setAutoSave(true)` straight after a repair.
- Fix: if `symlink_metadata().is_symlink()`, write to `canonicalize(path)` (or refuse with a clear error). Keep a single `settings.json.session-relay.bak`, or the newest 3.

**M5. Retry policy: the GUI retry overlaps live workers and retries permanent errors every minute**
- `auto_save.rs:14`, `hook.rs:28,124-129`, `overview.rs:67-73`.
- A worker's legitimate lifetime is 120 s sleep + up to 60 s classify-lock wait + up to 60 s sync-lock wait + push. That is longer than `STALE_AFTER` = 180 s, so the GUI regularly races a live worker. The two are serialised by `SyncLock`, so there is no corruption, but the result is double fetches and extra lock waits.
- `Io`/`Auth`/`Secrets` are "retry" classes with no backoff. If a token is revoked, or an I/O error persists, the watcher retries every 60 s forever. Each attempt fetches, and each attempt appends a `hook` failure line to `activity.jsonl`. At ~1 line/min, other projects' history is trimmed away within about a day.
- Fix: set `STALE_AFTER` above the worker's worst case (e.g. 6 min), or hold one lock across classify and sync. Add per-enc backoff (e.g. a `pending/<enc>/.last-try` file, doubling up to 30 min). Skip the activity append when the new entry equals the previous one for that key and source.

**M6. Settings page: toggling auto-save throws away unsaved form edits, and the tray and page disagree**
- `settings-page.tsx:62-65`, `tray.rs:37,71-77`.
- `setApp(await api.setAutoSave(...))` replaces the whole form state with the server DTO, so typed but unsaved machine name, folders or autostart vanish. Fix: `const next = await api.setAutoSave(v); setApp((a) => a && { ...a, hooks: next.hooks })`.
- StalePath shows as checked on the page but unchecked in the tray (`== Installed`). A tray toggle emits nothing, so an open settings page shows a stale checkbox. Fix: use the same predicate in both places and emit an event (e.g. `app-state-changed`) from the tray handler.

### Low
- **L1** `pending.rs:49` vs `add` (`pending.rs:24`, `write_atomic`): `clear_up_to`'s `remove_dir` can land between `add`'s `create_dir_all` and `File::create`. The tmp create then fails with NotFound, so that turn gets no marker and no worker. Fix: retry `add` once on NotFound, or never remove enc dirs.
- **L2** `hook.rs:83-85`: the worker inherits Claude's cwd (the user's project folder) for at least 2 minutes, so the folder cannot be renamed or deleted meanwhile. Fix: `cmd.current_dir(app_dir())`.
- **L3** `hook.rs:37-43`: a panic exits with 101, and Claude shows a hook error. Wrap `record_and_spawn` in `catch_unwind` to keep the exit code 0 guarantee. `hook.rs:60-62`: reuse `is_local_absolute` before `canonicalize`, so a `\\host\share` transcript path is never probed (the NTLM concern already handled in settings).
- **L4** `claude_hook_config.rs:78-88`: `install` always rewrites and backs up, even when already Installed (e.g. onboarding re-run), and moves our group to the end of `Stop`. Return early when `status == Installed`.
- **L5** `pending.rs:35,54-59`: a marker with a future stamp (clock set back, and its worker got a retry-class error) makes every later worker exit (`latest != stamp`). `stale()` never picks it (`s <= cutoff` is false), so auto-save for that project is silently dead until the wall clock catches up. Treat stamps beyond `now + 1 min` as stale.
- **L6** `hook.rs:54`: a `SessionEnd` delay of 0 s can miss the final lines Claude writes while it exits. A 3–5 s delay is cheap.
- **L7** Continuous activity (turns less than 2 min apart) postpones every save, and `stale()` only looks at the newest marker. Consider a max-wait: save when the oldest marker is over 10 min old.
- **L8** `claude_hook_config.rs:35-51`: `Malformed` also covers I/O errors (file locked, access denied) and a BOM-prefixed file, but the UI text says "not valid JSON". The disabled auto-save checkbox label still gets `cursor: pointer` (`design-tokens.css:69`; add `label:has(> input:disabled)` to the not-allowed rule). The onboarding checkbox is enabled or disabled from the initial `app.hooks`, not the `claudeHome` picked on that screen (`step-machine-and-roots.tsx:14,39`).
- **L9** Defence in depth: Rust spawns with `bInheritHandles=TRUE`, and only the three std handles are cleared. Any other inheritable handle Claude or VS Code passes to `hook-save` is inherited by a worker that lives for minutes. The spike shows current Claude is fine. A direct `CreateProcessW(bInheritHandles=FALSE)` would make this independent of Claude versions.
- **L10** Naming and small items: `cached_engine` caches nothing (suggest `engine_from_saved_identity`). `overview::classify_dir` rewrites `links.json` with fsync on every save even when unchanged. The success criteria in the phase file (VS Code round trip, kill-mid-save, byte-identical uninstall) are still unchecked. Tests compare `Value` equality, not bytes, and a CRLF or 4-space user file will not round-trip byte-identically.
- **L11** Onboarding now sends `autostart: app.autostart` (default `true`) with no autostart choice shown on that screen. Confirm that this is the intended explicit choice.

### Edge cases found by scouting
- Stop payload `last_assistant_message` (H2); dev and release builds sharing one app dir (M1); `claude_home` change (H3); symlinked settings (M4); unlink racing a hook save → `not_linked` failure for a key absent from the view (M2a); a live worker still running past 180 s (M5); future-dated stamps (L5); `remove_dir` vs `add` (L1).
- Checked and fine: two sessions in one enc dir (the shared debounce covers the whole dir); SessionEnd after Stop (the Stop worker exits, SessionEnd saves); the GUI flush clearing a sleeping worker's marker (the worker sees `latest == None` and exits); `enc` case consistency (canonicalize and `read_dir` both give the on-disk case); Windows reserved names (enc always starts `X--`); worker git env (`GIT_*`/`SSH_*` stripped); credential refresh across processes (`auth.lock`); temp files `.sr-tmp` ignored by `stamps()`.

### Positive Observations
- Spike B lessons are applied precisely: exec form, handle-inheritance clearing, null stdio, breakaway with fallback, and markers so a killed worker's save is still finished later.
- The per-turn marker design removes the classic "delete marker if unchanged" race, and a unit test covers it.
- `project_dir_of` uses canonicalize + `strip_prefix` (component-wise) + the `valid_enc` whitelist, and the worker re-validates. This is a correct trust boundary for a payload-supplied path.
- `claude_hook_config`: validated before any write; `expect`s are provably safe; uninstall removes only our entries and containers that became empty; tests cover foreign hooks, idempotency, stale paths and malformed files (no write, no backup).
- `save_pending` is shared by the worker and the GUI with an explicit retry-class list; PushOnly never pulls and never pushes conflicts.
- Clean dispatch before Tauri (no window and no single-instance involvement for `hook-*`). Files are under 200 lines, locale parity holds, and comments are sparse and purposeful.

### Recommended Actions
1. H1: persist the storage-blocked state and have the worker honour it.
2. H2: raise or stream the stdin cap, drain stdin, and add a large-payload test.
3. H3: NSIS pre-uninstall `hook-uninstall`, and migrate hooks on `claude_home` change.
4. M3: a bounded, single-shot quit with feedback.
5. M1: repair only when the registered exe is missing; never in debug builds.
6. M2: filter failures to visible projects, and have the watcher react to activity changes.
7. M5: `STALE_AFTER` above the worker's worst case, plus backoff and de-duplicated activity lines.
8. M4, M6, then the Lows.

### Metrics
- Type coverage: TS strict, `tsc` 0 errors; Rust fully typed.
- Tests: all green. New: 4 `claude_hook_config`, 2 `hook_save`, 2 unit tests (`pending`, `project_dir_of`). Missing: debounce exit, retry-class keeps marker (Busy), large stdin, symlinked settings, group holding ours plus foreign hooks.
- Lint: clippy 0 warnings.

### Unresolved Questions
- Does Claude Code log or surface EPIPE when a hook exits before reading all of stdin? This decides whether draining is required or only nice to have.
- Is pushing ciphertext to a repo that has turned public acceptable by design? H1 assumes it is not, based on the Phase 3/4 guard.
- Intended behaviour on a `claude_home` change: migrate the hooks, or warn only?
- Should onboarding show the autostart choice explicitly?
