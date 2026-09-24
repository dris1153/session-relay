## Code Review Summary: Phase 4 (dashboard, linking, tray, i18n, autostart), adversarial

### Scope
- Rust: `lib.rs`, `app_state.rs`, `dashboard.rs`, `tray.rs`, `watcher.rs`, `autostart.rs`, `commands/{dashboard,settings,mod}.rs`, `engine/{git_process,store_repo,progress,sync}.rs` (Phase 4 hunks only), `tauri.conf.json`, `Cargo.toml`
- Frontend: `app.tsx`, `features/dashboard/*`, `features/settings/settings-page.tsx`, `features/onboarding/onboarding-flow.tsx`, `lib/{use-dashboard,status-copy,sessions,format,tauri-commands,i18n}.ts`, `components/{confirm-dialog,folder-fields,onboarding-card}.tsx`, `locales/{vi,en}.json`
- ~2.1k LOC new/changed. Cross-checked against `overview.rs`, `state.rs`, `evaluate.rs`, `links.rs`, `lock.rs`, `maintenance.rs`, `context.rs`, `publish.rs`, auto-launch 0.5.0 source.
- Verification: `npx tsc --noEmit -p .` 0 errors · `cargo clippy --all-targets -- -D warnings` 0 warnings · `cargo test` 77 passed / 1 ignored (includes tester files being added concurrently: `tests/git_error_classification.rs`, `tests/phase_04_*`). GUI not run, no GitHub calls, no Credential Manager access.

### Overall Assessment
Solid structure: every command resolves `key_hash` through the cached view, the stall-based git runner is correct (no lost output, raw bytes kept, UTF-8-safe final stderr), Network/Timeout never rebuild the clone, all new `sync.lock` holders clear stale git locks, no lock-order inversion, CSP/capabilities unchanged, files < 200 lines, locale keys in parity.

Weak spots are in **state freshness around `sync.lock` contention and the dashboard cache**: a deterministic startup race leaves the first screen on a `busy` error, the tray never reflects statuses computed at launch, background progress sticks in the header, and a busy post-save refresh makes just-saved sessions look "deleted from the cloud". Phase 5 hooks will make `Busy` far more common, so these matter before hooks land.

**Score: 7/10.** H1/H2 are small fixes (publish after maintenance, tray update in `load`). With M1–M4 fixed, 8.5+.

---

### Critical
None.

### High

**H1. Startup maintenance and the first `list_projects(true)` race on `sync.lock`; loser fails silently**
- `lib.rs:71-86` spawns `maintenance::on_startup` (holds `sync.lock` via `try_acquire`: `recover()` + weekly `fsck`/`gc --prune=now` up to 600 s each, temp sweep, backup prune) right in `setup`. The webview's first call is `run("list", listProjects(true))` (`use-dashboard.ts:45`).
- `dashboard.rs:76` / `:81`: `Busy` is only tolerated `if cache.view.is_some()`. At startup the cache is empty → `Err(busy)`.
- Scenario (deterministic on gc days, random otherwise): app launches → maintenance runs gc on a 400 MB store → UI shows "Another sync is running…" + "Loading projects…" with no retry. Nothing re-publishes when maintenance ends (watcher only publishes on head change, `watcher.rs:29`). Hidden autostart window: same, invisible.
- Reverse case: UI wins the lock → `on_startup` logs `busy` and is **skipped for the whole run** (no weekly gc, no temp sweep, no backup prune) — on an always-autostarted machine this can repeat every boot.
- Fix: in the `restore` thread, after `on_startup`, call `dashboard::publish(&app, &state)`; make maintenance `acquire_within(…, 60 s)` instead of `try_acquire`; in `load`, when `Busy` and no cached view, `acquire_within` a few seconds (or UI retries `busy` on the initial load with backoff).

**H2. Tray attention is never set from statuses computed at launch or on focus**
- Only `publish` calls `tray::set_attention` (`dashboard.rs:100-104`). `list_projects` (`commands/dashboard.rs:22-25`) and the focus reload never touch the tray.
- The UI's initial `list_projects(true)` sets `cache.sha = head`, so the watcher's `head != known` (`watcher.rs:27-29`) is false → no publish.
- Scenario: machine B off overnight, A saves 5 sessions. B autostarts minimized → hidden webview fetches, project is `remote_ahead` → tray icon stays plain until some *other* remote change happens. Breaks the success criterion "remote save … flips tray to attention" for the most common case (changes made while offline/asleep).
- Fix: update the tray inside `load` (via `state.app.get()`) whenever a view is produced, or have `list_projects` call `set_attention`. H1's publish-after-maintenance also covers startup.

---

### Medium

**M1. `AppState.dashboard` mutex held across network fetch and full list**
- `dashboard.rs:63-90`: the guard spans `overview::refresh` (`ls-remote` ≤180 s + stall-based fetch with no upper bound) and `overview::list` (decrypt all manifests, hash local files).
- Blocked meanwhile: `project_key` (`:111`) → `sync_project`, `link_project`, `delete_remote_session`; `open_project_folder` (`commands/dashboard.rs:122`, also calls the opener while holding it); `clear_local_data` (`:142`); `install_engine` (`app_state.rs:74`) → `save_settings` with machine/claude_home change; watcher (`watcher.rs:21,27`).
- Scenario: machine B first run, ~420 MB store at 1 MB/s ≈ 7 min. User opens Settings, renames the machine, clicks Save → "Working…" for 7 min. "Open folder" in ⋯ (not disabled by `busy`) hangs the same way.
- No deadlock: `load` only `try_acquire`s `sync.lock`, and no `sync.lock` holder takes the dashboard mutex. Verified all paths.
- Fix: lock only to read `(sha, view)` and to write results; serialize refreshes with a separate gate (or rely on `sync.lock`); add a cache generation counter bumped by `install_engine` so results from a replaced engine are dropped.

**M2. Background progress sticks in the header; per-file progress is unthrottled**
- `use-dashboard.ts:46` sets `progress` on every `sync-progress`, but only `run()`'s `finally` clears it (`:38`). Watcher/tray/settings publishes (`publish` → fetch) emit Download events with no `run` active.
- Scenario: another machine saves → watcher fetch → header shows "Downloading storage… 100%" and a full bar (`dashboard-header.tsx:18-36`) indefinitely, until the user runs any action.
- `sync.rs:110-111,163-164` emits one event per file, unthrottled; `done` is reported before the item, so it never reaches `total`. Restoring 2k files = 2k IPC events, each re-rendering `DashboardPage` (progress state lives there) incl. `groupSessions` over all files → UI jank.
- Fix: clear progress when not busy (ignore events when `busy === null`, or clear on `projects-changed`); throttle in `ProgressSink` (percent change or ≥100 ms); report `done + 1` after each item; move progress state into the header.

**M3. After a save, a busy/failed refresh leaves `cache.sha` behind the pushed commit → just-saved files misreported**
- `commands/dashboard.rs:56-58`: `publish` after sync → `load(true)`. If `refresh` hits `Busy` (a hook worker / tray save took the lock right after), `dashboard.rs:76` returns the **old** view → UI still says "Not saved yet". 
- Next window focus → `load(false)` evaluates the *new* base against the *pre-push* manifest (`cache.sha`, and FETCH_HEAD is also pre-push). Per `state.rs:53-64`: newly saved sessions → `(local, base, !remote) unchanged` → **`remote_deleted` ("Deleted from the cloud")**; edited whole files → `remote_ahead` ("cloud is newer").
- Offline restart makes it sticky: `last_fetched()` (`store_repo.rs:72-75`) returns the pre-push FETCH_HEAD.
- Online it self-heals within ~60 s (watcher sees the new head); no data harm (every action fetches first). With Phase 5 hooks, the Busy window is common.
- Fix: have `sync_project` / `delete_remote_session` return the snapshot sha they ended on (pushed commit or fetched sha) and store it in `cache.sha`; or record the pushed commit in a local ref (`refs/session-relay/last`) that `last_fetched` prefers; optionally treat `manifest.generation < base.generation_seen` in `overview::list` as "snapshot stale" instead of computing states.

**M4. `failure()` classification gaps (clone rebuild / misleading "offline")**
- 403 → Network: `git_process.rs:156-157`. "fatal: unable to access '…': The requested URL returned error: 403" (+ `remote: Permission to … denied`) hits `unable to access` → `Error::Network`. Scenario: App permission reduced to read-only, suspended installation, or secondary rate limit → push fails as "Cannot reach GitHub", and `load` marks the dashboard **Offline** while online. Phase 3 M3 recommended 401/403 → Auth. Fix: add `"returned error: 403"`, `"Permission to"` to `AUTH` (checked first).
- Spawn failure still rebuilds: `git_process.rs:93,120` map `spawn()` / `try_wait()` errors to `Error::Git` → `sync.rs:66-69` / `context.rs:60-63` call `rebuild()` → `remove_dir_all(store)`. Scenario: Git for Windows upgrade replaces `git.exe` mid-sync, or AV blocks spawn → healthy clone deleted → full re-download. Fix: map spawn/wait errors to `Error::Io` (or a new non-rebuild variant).
- Locale hardening (conditional): `configure()` never pins the message language. The local Git 2.54 install ships no `share/locale`, so not reproducible here, but a localized git build (MSYS2/Cygwin, `LANG=vi_VN` set) translates "could not read Username", "Authentication failed", "early EOF" → `Error::Git` → rebuild on sign-out. Fix: `cmd.env("LC_ALL", "C")` (store paths are ASCII, porcelain unaffected).

**M5. Watcher neither marks offline nor surfaces auth loss**
- `watcher.rs:31`: `Network`/`GitTimeout` from `ls-remote` are ignored; `offline` is only set by `load(true)`. Focus reloads use `fetch=false`, so no fetch happens. Scenario: app starts online, Wi-Fi drops → no "Offline" badge; watcher keeps polling every 60 s (black-holed network: each `ls-remote` up to 180 s). Spec: "silent backoff, status Ngoại tuyến".
- `watcher.rs:32` and `dashboard.rs:106`: `auth_rejected` / `not_logged_in` only logged every minute. User isn't routed to sign-in until they click something.
- Fix: on network failure set `cache.view.offline = true` + emit `projects-changed`; on `Auth`/`NotLoggedIn` emit `auth-changed{signed_in:false, reason}` (or `storage-changed`) once.

**M6. Every window focus runs a full `overview::list`, with no in-flight guard**
- `use-dashboard.ts:48` → `load(false)` → `overview::list` → per project `ls-tree` + `cat-file` (`remote.rs:20-21`, 2 git spawns each) + local evaluation, holding `sync.lock` (`overview.rs:73`) which hook workers wait on.
- 50 projects ≈ 100+ git spawns (~2-4 s on Windows) per focus; alt-tabbing queues N lists behind the mutex (M1). `sync_project` also fetches twice (own fetch + `publish` fetch).
- Fix: cache decrypted manifests per `sha` in `DashboardCache` (immutable per snapshot); skip a focus reload while one is in flight or <5 s old.

**M7. UI state machine holes**
- Silent click: `use-dashboard.ts:19-23` routes `STORAGE_ERRORS` to `advance()` without setting an error. If the recheck comes back `ready` (e.g., git credential failure while REST works), Save "does nothing" with no feedback, repeatably. Fix: also `setError(code)` (cleared when onboarding leaves the dashboard).
- Single `busy` slot: `run()` overwrites `busy`; "Open folder" / "Copy resume" stay enabled while busy (`project-detail-pane.tsx:65-66`). The quicker run's `finally` sets `busy=null` while the long run continues → primary buttons re-enabled mid-sync. Fix: counter or `disabled: busy` for those items.
- Auto-selection jump: `dashboard-page.tsx:25` picks "first needing attention" while `selected` is null. After syncing it, the pane jumps to another project and the user's report disappears from view. Fix: `setSelected(p.key_hash)` when an action runs (or on first render).

**M8. Autostart re-applied at every launch fights the OS setting**
- `lib.rs:33` → `autostart.rs:13-16`. auto-launch 0.5 `is_enabled()` returns false when the user disabled the entry in Task Manager; `enable()` rewrites both `Run` and `StartupApproved\Run` (verified in `~/.cargo/registry/.../auto-launch-0.5.0/src/windows.rs:37-51`).
- Scenario: user disables Session Relay in Task Manager → next launch silently re-enables it. It also registers on the very first launch, before onboarding finishes (spec: "enabled at end of onboarding"). And if the exe moved, `is_enabled()` stays true for the stale path and it is never fixed.
- Fix: call `apply` only when the setting changes and at onboarding finish. At startup, if the setting is on but the OS says disabled, set the setting to false instead of re-enabling.

---

### Low

- **L1. `cache.sha == None` is overloaded** (`dashboard.rs:65-67`): after a successful fetch of an empty/reset store (`Ok(None)`), every `load(false)` calls `last_fetched()` and resurrects the old FETCH_HEAD → statuses flip between focus and publish. Fix: `sha: Option<Option<String>>` or a `loaded` flag.
- **L2. UNC probe before guard**: `commands/dashboard.rs:87` `local_root.is_dir()` and `commands/settings.rs:58` probe webview-supplied paths; `locate()` deliberately refuses `\\…` to avoid NTLM leaks (`project_identity.rs:40-42`). Fix: reject `\\` prefix before `is_dir()` (a compromised webview is required, hence Low).
- **L3. `clear_local_data` ordering** (`commands/dashboard.rs:136-142`): the cache is reset after `sync.lock` is released. A focus `load(false)` in between uses the old `cache.sha` against a deleted clone → `git_failed` banner. Also `remove_dir_all(store)` can race the lock-free watcher `ls-remote` (its CWD is the store → sharing violation → half-deleted clone, later rebuilt). Fix: reset the cache inside the lock scope; let the watcher skip while a clear holds the lock (e.g., `try_acquire` briefly).
- **L4. Mutex poisoning**: `lock().expect(..)` everywhere. One panic inside `load` (in `spawn_blocking`) poisons `dashboard`, so every dashboard command returns `internal` and the watcher thread dies silently. Fix: `unwrap_or_else(PoisonError::into_inner)`.
- **L5. Tray locale parse panics** (`tray.rs:17`): `HashMap<String,String>` + `expect`. Any non-string value added to a locale file crashes `setup`. Fix: parse to `serde_json::Value` or add a unit test that loads both files and asserts `tray.*` keys.
- **L6. Progress parser**: takes any `%` incl. `remote: Counting objects: 100%` and chunk-split lines (`progress.rs:43-47`) → bar jumps 100→3→100. Fix: only `Receiving objects`/`Writing objects` lines, keep a carry buffer.
- **L7. a11y**: `role="menu"` without arrow-key navigation or focus move (`project-overflow-menu.tsx:37-53`); `<dialog>` without `aria-labelledby` (`confirm-dialog.tsx:16`); no explicit `focus-visible` ring (browser default only; spec asks for focus rings).
- **L8. DESIGN**: several 4 px spacings (`gap-1`, `py-1`, `mt-1`, `px-2 py-1`) off the 8 px base; the detail pane sits directly on the canvas rather than on Paper White cards. Otherwise compliant: monochrome, Clay only in the tray dot, serif only for 24/30 px headings, cursor rule global, status badge style correct.
- **L9. `copyResume` quoting** (`dashboard-page.tsx:61`): PowerShell expands `$`/backtick inside `"…"` → wrong `cd` for folders like `C:\work\$tmp`. Use single quotes.
- **L10. Open folder vs resume path**: `open_project_folder` opens the repo root (`commands/dashboard.rs:124`), while `copyResume` uses root + subpath. Pick one.
- **L11. Activity row for `DeleteRemote`** renders "App · 0 saved, 0 restored" (`activity-log.tsx:27`; `action` is not shown).
- **L12. Tray "Save all" gives no feedback**: errors are only logged (`tray.rs:54-56`). The spec's "error toasts only when window hidden" (step 6) and per-row "Đang kiểm tra…" are not implemented. Record them as deferred in the phase file.
- **L13. Quit during an in-flight sync**: `app.exit(0)` (`tray.rs:60`) does not kill the git child. The orphaned push/reset keeps running in the store while the next instance takes `sync.lock`. Fix: kill tracked children on exit, or a job object with KILL_ON_JOB_CLOSE.
- **L14. Doc drift**: `plan.md:62` still says `sync-progress{key_hash, stage, done, total}` and "tray labels use a small Rust locale map", which contradicts the updated table and code. `phase-04` todo list is unchecked and the status is Pending.

---

### Edge Cases Found by Scout
- `sync.lock` contention points: startup maintenance, UI initial fetch, post-sync publish, tray save-all thread, `delete_remote_session`/`refresh` (`try_acquire`, immediate `busy`), and Phase 5 hook workers → H1, M3.
- Cache/sha lifecycle: `install_engine` resets the cache (settings change, re-unlock) → `project_key` fails `invalid_data` until the next load succeeds. `cache.sha` is not updated by sync/delete → M3, L1.
- Events without an initiator: `sync-progress` from background fetches → M2; tray state only via `publish` → H2.
- Remote classification: 403, spawn errors, localized stderr → M4.
- Webview inputs: `key_hash` (resolved via cache ✓), `files` (filter only ✓), `session_id` (filter only; `""` is a no-op ✓), `local_root` (absolute + dir ✓, UNC probe L2), `mode` (no PushOnly from UI ✓).

### Positive Observations
- `exec`: stderr reader + unbounded mpsc, no pipe deadlock. Drains before and after `join`, so no lost output. Raw bytes kept → final stderr is UTF-8-correct even when chunks split code points. Stall-limit only for transfers; `run` keeps the total limit.
- Network/Timeout/Auth never reach `rebuild()`. `load` degrades to offline with `last_fetched()` statuses.
- All new `sync.lock` holders (`refresh`, `delete_remote_session`, `sync_project`) clear stale git locks. `clear_local_data` deletes under the lock.
- Trust boundary: every command maps `key_hash` through the cached view. The UI cannot pass arbitrary paths except the link root. `open_path` runs from Rust on a linked dir only. Capabilities and CSP unchanged.
- Offline start: engine from cached identity at `setup`, storage check behind the dashboard, `SIGN_IN_AGAIN` routing kept.
- Tray labels come from the same locale JSON (`include_str!`). The first run persists the detected language so tray and UI match; `set_language` rebuilds the menu live.
- Debug builds never register autostart. Single-instance is registered first. Close hides the window. The credential helper exits before Tauri/single-instance starts.
- Native `<dialog>` confirm with Cancel focused first; destructive actions confirmed; link mismatch requires explicit "Link anyway".
- Files < 200 lines, comments sparse and purposeful, vi/en keys in parity, no hard-coded UI strings besides the brand name.

### Recommended Actions
1. H1: publish after startup maintenance; `acquire_within` for maintenance; tolerate/retry `busy` on the first load.
2. H2: update tray attention whenever `load` produces a view.
3. M3: store the post-sync/delete snapshot sha in the cache (or a local ref used by `last_fetched`).
4. M1: stop holding the dashboard mutex across refresh/list; add a cache generation counter.
5. M2: ignore/clear progress when idle; throttle per-file progress.
6. M4: 403 → Auth; spawn errors → non-rebuild variant; `LC_ALL=C` for git.
7. M5–M8, then Lows. Update `phase-04` todo/status and `plan.md:62`.

### Metrics
- Type check: 0 errors (tsc). Clippy: 0 warnings with `-D warnings`.
- Tests: 77 passed, 1 ignored. No frontend tests. No Rust tests for `dashboard.rs`/`watcher.rs`/`tray.rs` logic (Busy paths, cache sha lifecycle).
- Linting issues: 0.

### Unresolved Questions
1. Should `load` return a partial "loading" view instead of `busy` when there is no cache, or should the UI own the retry?
2. Is `clear_local_data` wiping `base/` (and with it `generation_seen` rollback protection) intended? After it, every file with both sides present needs a content compare, and differing files show as diverged.
3. Intended semantics of the `std::time::Instant`-based 24 h storage re-check across sleep/hibernate (`watcher.rs:19,34`): wall-clock or uptime?

**Status:** DONE_WITH_CONCERNS
**Summary:** Phase 4 is structurally sound (trust boundary, git runner, lock hygiene, i18n parity; tsc/clippy/tests clean), but `sync.lock` contention and dashboard-cache freshness produce a startup `busy` race (H1) and a tray that never reflects launch-time statuses (H2).
**Concerns/Blockers:** H1/H2 should be fixed before Phase 5 hooks increase lock contention; M3 (post-save misreport) and M1 (mutex across network) are next.
