## Code Review Summary: Phase 4b (dashboard loading speed + skeleton UI), adversarial

### Scope
- Rust: `engine/{git_process,git_batch (new),git_failure (new),store_repo,remote,overview,progress,lock,maintenance}.rs`, `dashboard.rs`, `commands/dashboard.rs`, `watcher.rs`, `lib.rs`, `app_state.rs`, `tests/{batch_reads (new),sync_roundtrip}.rs`
- Frontend: `lib/{use-dashboard,tauri-commands}.ts`, `features/dashboard/{loading-skeleton (new),dashboard-header,project-sidebar,dashboard-page}.tsx`, `styles/design-tokens.css`, `locales/{vi,en}.json`
- ~235 changed + ~150 new LOC. Cross-checked `context.rs` (`with_store`), `sync.rs` (rebuild loop), `evaluate.rs`/`snapshot.rs` (thread safety, streaming), `publish.rs` (store layout), `key_setup.rs` (fetch caller), `tray.rs`, `main.tsx` (StrictMode), `DESIGN.md`.
- Verification: `npx tsc --noEmit -p .` 0 errors · `cargo clippy --all-targets -- -D warnings` clean · `cargo test` all green (1 ignored). Scratch repro (git 2.54, `GIT_CONFIG_GLOBAL=NUL`) of `cat-file --batch` output for absent paths, deleted and corrupt objects, and `ls-tree p/` listing files. GUI not run, no GitHub calls, no Credential Manager access.

### Overall Assessment
The backend core is sound. The batch parser reads exact lengths. The stdin writer runs on its own thread while stdout and stderr are drained, so there is no pipe deadlock, and a timeout kill breaks the pipe. A spawn failure stays `Io`. A missing or corrupt object becomes `Error::Git` and never reads as "empty cloud". Fetch without `ls-remote` maps only the missing-ref case to `None`, and the write guards (`StoreReset`, `RollbackDetected`) still protect pushes and pulls. Parallel evaluation is deterministic (index sort), `Engine` and `Links` are proven `Sync` at compile time, hashing streams, and panics propagate as before. The cache reset keeps `generation`. The measured speedups are real.

Weak spots:
1. The 2-phase effect breaks under React StrictMode (dev).
2. Phase 1 now fills the cache, so the `Busy` fallback in `load` can silently drop the phase-2 fetch result.
3. With fsck gone, an `Error::Git` from `overview::list` has no recovery path, and `read_blobs` is stricter than the store layout warrants.
4. `gc_if_due` repeats recurring bug class 1 (no stale-lock clearing) and can hold `sync.lock` for minutes while the user is active.

**Score: 7/10.** With H1 and M1–M3 fixed: 8.5+.

---

### Critical
None.

### High

**H1. The 2-phase initial load never completes in dev builds (StrictMode): its result is thrown away**
- `use-dashboard.ts:59-74`, the run guard at `use-dashboard.ts:40`, `main.tsx:7` (`<React.StrictMode>`).
- Dev mode runs effects twice. Effect 1 calls `run("list")`, which sets `running.current = "list"` and waits on `localProjects`. Cleanup 1 sets `alive = false`. Effect 2's `run("list")` is refused, because refs survive the StrictMode remount. When `localProjects` resolves, `if (local && alive)` is false, so nothing is shown, and `while (alive)` is false, so phase 2 never runs. `data` stays null and the skeleton stays forever. `onFocus` also bails while `running` is set. `lib.rs` no longer publishes after startup maintenance, and the watcher publishes only when the remote head moved or the app is offline. Nothing rescues the screen.
- Pre-4b worked: the first `listProjects(true)` was sent synchronously while `alive` was still true, and `show` was not gated.
- Release builds are unaffected (one effect run; a real remount gets fresh refs). But every `tauri dev` session, and any puppeteer probe against dev, sees a stuck skeleton.
- Fix: never gate the result on `alive` (setState after unmount is a no-op in React 18+). Only the busy retry should stop:
  ```ts
  const local = await api.localProjects().catch(() => null);
  if (local) show(local);
  for (;;) {
    try { return show(await api.listProjects(true)); }
    catch (e) { if (errorCode(e) !== "busy") throw e; if (!alive) return; await pause(BUSY_RETRY_MS); }
  }
  ```

---

### Medium

**M1. When `list` is Busy, the phase-1 view silently replaces the phase-2 fetch result**
- `dashboard.rs:83-98`, `overview.rs:85`.
- Phase 1 now fills `cache.view`. Phase 2 `load(true)`: `refresh` succeeds (new snapshot fetched, `refs/session-relay/last` moved) and releases the lock. Another holder takes it: tray "Save all" (`tray.rs:49-56`) today, hook workers in Phase 5, or the weekly gc. `list` waits 2 s and gets `Busy`, so `Err(Busy) if cached.is_some() => return Ok(view)` returns the phase-1 projects with `offline=false, fetched_at=now`.
  - The UI treats that as fresh: "Checking GitHub…" disappears, and the frontend busy retry never fires because the call returned `Ok`.
  - The tray is not updated, and the watcher will not publish (`head == last_snapshot`).
  - The view stays stale until the next focus reload.
- Related: `engine.repo.last_snapshot()` (`dashboard.rs:95`) is evaluated *before* `list` takes the lock, so a push or a gc in between can leave it stale.
- Fix: for `fetch=true`, hold one lock for the whole operation: `overview::refresh_and_list(engine, wait, cache)` acquires it (reporting Waiting), fetches in `with_store`, reads `last_snapshot()` under the same guard, then lists. Minimal alternative: when `refresh` returned `Ok` and `list` is `Busy`, return `Err(Busy)` so the UI retries.

**M2. `Error::Git` on the dashboard read path has no recovery, and `read_blobs` treats a harmless layout variation as corruption**
- `store_repo.rs:104-112`, `:118-121`, `:188` (comment); `remote.rs:18-31`; `git_batch.rs:8`.
- (a) `list_dirs(sha, "p")` returns files as well as dirs (repro: `p/README` listed). Two things now fail the *whole* list with `Error::Git "unexpected output"`, because git answers `<spec> missing`:
  - any non-project entry under `p/`;
  - a project dir without `manifest.age` (manual edit on GitHub, another tool or version).

  Pre-4b, `show` returned `None` and skipped it. The comment "paths come from a listing … an absent one means a damaged clone" is inaccurate: the listing is of `p/`, and the manifest paths are derived from it.
- (b) fsck was dropped. The `recover()` comment says "corruption it cannot see shows up as a git error later, which rebuilds the clone too". That is only true inside `with_store` or `sync_project`; `overview::list` is not wrapped.
  - A missing or corrupt manifest object (repro: corrupt loose object → `error: inflate…` + `<oid> missing`, exit 0) gives a **permanent** `git` error on every load.
  - `fetch` does not heal it: the tip is already reachable from existing refs, so the connectivity check is trivial.
  - Escapes: Settings → clear local data, or syncing a project. Syncing is impossible when `data` is null (see M3).
- git 2.54 prints `<spec> missing` both for "path absent" and for "blob object deleted" (verified), so the output format cannot tell them apart.
- Fix, in order:
  1. Stop misreading the layout. Either list dirs only (`ls-tree -d --name-only`) and read manifests by oid: one `ls-tree -r <sha> -- p/`, keep `*/manifest.age` entries, then `cat-file --batch` on the oids, where `<oid> missing` unambiguously means damage and an absent manifest simply is not listed (pre-4b semantics). Or use the simpler plan-step-5 shape `Vec<Option<Vec<u8>>>` and skip missing entries with a warning (the sync path stays strict through `show`).
  2. Then add recovery: in `dashboard::load` (fetch path), on `Err(Error::Git)` from `list`, run `rebuild()` + refresh + list once under the lock, like `sync_project`.

  Do step 1 first. Otherwise a layout quirk turns into a 391 MB re-download on every load.

**M3. The skeleton keeps pulsing after a failed initial load**
- `dashboard-page.tsx:92,108`, `project-sidebar.tsx:38-39`.
- `loading={data === null}` ignores `busy`. If the phase-1 failure is swallowed and phase 2 fails with a non-busy error (git/io/internal, e.g. M2), the run ends and the error banner shows, but the skeletons keep pulsing as if loading. After Dismiss only skeletons remain, with no retry.
- Fix: use `loading = data === null && busy !== null`. When `data === null && busy === null`, render an error/empty state with a Retry button (`run("list", async () => show(await api.listProjects(true)))`).

**M4. Weekly gc: no stale-lock clearing, the stamp is written on failure, and it can block user saves**
- `maintenance.rs:28-40`, `watcher.rs:49-54`.
- There is no `engine.repo.clear_stale_locks()` after `try_acquire`. This is the recurring rule that every `sync.lock` holder running git must clear the `*.lock` files that taskkill leaves behind. A leftover `packed-refs.lock` or `index.lock` makes reflog expire and gc fail, `let _` ignores the failure, and the stamp is written anyway, so there is no gc for another week.
- The stamp is also written when gc timed out (600 s) or failed.
- "When idle" is not actually checked, only "lock free + uptime > 10 min". On the 391 MB store of age ciphertext, `gc --prune=now` repacks everything and tries deltas on incompressible data (duration not measured). Meanwhile:
  - `sync_project` waits only 30 s, so a user save fails with `busy`;
  - `list` waits 2 s, so the view goes stale (M1);
  - Phase 5 hook workers wait or skip.
- Short sessions (< 10 min, autostart off) never gc, so the store grows without bound (`gc.auto=0`).
- Fix:
  - call `clear_stale_locks()` first;
  - check `status_ok` of both commands, stamp only on success, and log otherwise;
  - use `-c pack.window=0` (deltas are useless on ciphertext) and measure the duration;
  - optionally skip while a GUI action ran in the last N minutes.

---

### Low
- **L1. Background progress leaks into the user's header.** `refresh` reports `Waiting`/`Checking`/`Evaluate` (`overview.rs:61,66,128`) from watcher and tray publishes too. The UI shows any `sync-progress` while `running.current` is set (`use-dashboard.ts:78`), so during a user save the header can flip to "Waiting for another task…". Fix: report these stages only for foreground loads (a flag or origin tag on the event).
- **L2. The retry loop lost its stop condition.** `lastLoad.current === 0` became `alive`, so a `projects-changed` that arrives during busy retries no longer ends phase 2. The comment "its end publishes, and we keep asking meanwhile" is now stale. Fix: `const seen = lastLoad.current` after phase 1, and loop while `lastLoad.current === seen`.
- **L3. The parser skips two sanity checks** (`git_batch.rs:8-11`). It does not verify that the byte after the body is `\n`, and it does not check that the type is `blob` (a tree at `manifest.age` is parsed as content, then skipped as an "unreadable manifest"). Add `out.get(end + 1 + size) == Some(&b'\n')` and a `blob` check.
- **L4. The missing-ref match is too loose** (`store_repo.rs:63`). It matches the substring anywhere in stderr, including server `remote:` lines. Match a line starting with `fatal: couldn't find remote ref`. Data impact is limited by `StoreReset`/`RollbackDetected`.
- **L5. Accessibility gaps.**
  - The `aria-live` span mounts together with its text (`dashboard-header.tsx:20-24`), so screen readers often don't announce it.
  - `Evaluate` counts update about every 100 ms, which is chatty.
  - The progressbar has no accessible name.
  - The skeletons are `aria-hidden` and there is no `aria-busy` on the list.

  Fix: keep the live region always mounted and announce stage changes only, add `aria-label` on the bar, and put `aria-busy` on the sidebar list.
- **L6. No Waiting stage in `list`.** It waits 2 s without reporting `Waiting` (`overview.rs:85`), so phase 1 during startup maintenance shows a generic "Working…".
- **L7. Double clone.** `ManifestCache` is deep-cloned twice per load (`dashboard.rs:80`, `remote.rs:48`); the plan had `Arc<Vec<Manifest>>`. Negligible at 24 projects.
- **L8. Double hashing without bases.** Between the first fetch on a new machine and the first sync of its linked projects, every `list` re-hashes the content (list never persists `in_sync_entries`), and the 2-phase open now does it twice while holding `sync.lock`. The design predates 4b; phase 1 only doubles the cost in that window.
- **L9. Test gaps:**
  - `ManifestCache` invalidation on a sha change;
  - result order of parallel evaluation across several projects (existing tests use one);
  - gc stamp logic;
  - the `local_projects` → `None` path;
  - `p/` containing a non-project entry.
- **L10. File size cap.** `overview.rs` is 195 lines and `store_repo.rs` 196, right at the 200-line cap. The next change must split them (e.g. move `evaluate_all` to its own module).

### Edge Cases Checked (no issue)
- Stdin writer: when git exits early or is killed, `write_all` gets a broken pipe and the thread ends. No handle-inheritance leak (std serializes Windows spawns). Input is empty only for regular calls (stdin = null).
- `fetch` on an empty remote or a deleted `main` → `None` + forget ref (tested). Auth and network errors are still classified (`Auth`/`Network`, never `Git`). `create_key` on a new repo still works (`key_setup` tests).
- Cache: reset on `install_engine`/`clear_local_data`, keyed by sha, write-back guarded by generation. A rebuild keeps the same sha, which is still valid.
- Parallel evaluation: 4 threads each spawning git for rare `remote::chunk` reads is safe for concurrent read-only access. Progress may arrive out of order only if > 100 ms separate `fetch_add` and `report`; the last event always passes.
- Reduced motion: skeleton `motion-reduce:animate-none`; the indeterminate bar becomes a static, full-width, faded bar. Tokens are from DESIGN.md (chalk, graphite, `rounded-control`). Locale keys are in parity; the removed `dashboard.loading` is not referenced anywhere.

### Positive Observations
- An exact-length, binary-safe parser with a NUL/newline/0xFF unit test, and an integration test comparing batch reads with `show`.
- A missing object is never mapped to `None` on the batch path, so damage cannot read as an empty cloud. The missing-ref path is tested and forgets the last snapshot.
- Parallel evaluation: atomic work index, sorted results, poison-tolerant, bounded threads, streaming hashing (memory bounded with 174 MB transcripts).
- Startup no longer runs fsck/gc, the duplicate startup publish is gone, the lock polls every 50 ms, and the timing logs carry counts and ms only (no PII).
- Every file stays under 200 lines, and comments are mostly limited to real constraints.

### Recommended Actions
1. H1: stop gating phase results on `alive` in `use-dashboard.ts`.
2. M1: hold one lock across refresh + read of `last_snapshot` + list, or return `Busy` after a successful fetch.
3. M2: `ls-tree -d` + tolerant or oid-based manifest reads, then rebuild on `Error::Git` in `load`. Fix the two inaccurate comments.
4. M3: skeleton only while busy, plus an error/retry state.
5. M4: `clear_stale_locks`, stamp only on success, `pack.window=0`, measure gc time.
6. L1–L10 as convenient; add the L9 tests with the fixes.

### Metrics
- Type coverage: strict TS, typed `Progress` union; Rust clippy `-D warnings` clean.
- Tests: all green (unit + integration); coverage not measured.
- Linting issues: 0.

### Unresolved Questions
1. How long does gc take on the real 391 MB store, with and without `pack.window=0`? This decides how much M4's blocking matters.
2. Should phase 2 block actions at all? `sync_project` fetches under the lock itself, so phase-1 data could be actionable while phase 2 runs, with only a header indicator. A captive-portal hang can currently disable the buttons for up to 120 s (fetch stall limit).
3. Was the step-12 puppeteer probe run against `tauri dev`? If so, H1 should have shown up; please confirm manually.
4. Keep strict manifest reads, or go back to pre-4b tolerance for `p/*` entries without a manifest?
