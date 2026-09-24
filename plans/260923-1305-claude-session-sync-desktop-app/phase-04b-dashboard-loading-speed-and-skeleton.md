---
phase: 4b
title: "Dashboard Loading Speed & Skeleton UI"
status: completed
priority: P1
effort: "1d"
dependencies: [4]
---

# Phase 4b: Dashboard Loading Speed & Skeleton UI

## Context Links
- Brainstorm (approved): `../reports/brainstorm-260924-0740-dashboard-loading-speed-and-skeleton.md`
- Phase 4 review log (cache/lock design): `phase-04-dashboard-ui-and-tray.md`
- Code: `src-tauri/src/{dashboard.rs,watcher.rs,lib.rs}`, `src-tauri/src/engine/{overview,remote,store_repo,git_process,lock,maintenance,progress}.rs`, `src/lib/use-dashboard.ts`, `src/features/dashboard/*`

## Overview
Opening the app is slow every time and the loading screen is misleading (sidebar shows the empty-state text). Show a real list in about 1 s (2-phase load against the snapshot already on disk), make every loading second say what it is doing, and cut per-load costs: startup lock contention, a redundant network round trip, ~50 git spawns, repeated manifest decryption.

## Key Insights (scouted 2026-09-24)
- 24 projects / 1210 files / 391 MB store (1 pack); 34 local dirs / 1.5 GB; transcripts up to 174 MB; bases exist for all projects. One git spawn ≈ 22 ms.
- Startup maintenance holds `sync.lock` (recover, temp sweep, backup prune, weekly fsck + gc) while the first load waits; `acquire_within` polls every 500 ms.
- `fetch` = `ls-remote` + `fetch` → 2 round trips, 2 credential-helper spawns.
- `remote::all_manifests` = `list_dirs` + per project `ls-tree` + `cat-file` (2 spawns each).
- Content hashing only without base / both sides changed; parallelism pays off mainly there (new machine, after "clear local data").
- Debug build leaves our crate unoptimized: measure on release.

## Requirements
- Functional
  - Phase-1 list (`list_projects(false)`, no network) shown before the network refresh; header "Đang kiểm tra GitHub…" until the fresh list arrives.
  - Skeletons in sidebar and detail pane while no data; empty-state texts only after data loaded.
  - Header stage label + bar for: waiting for lock, checking GitHub, download %, evaluating X/Y projects (indeterminate bar when no ratio).
  - Weekly gc runs when idle, never delaying the first load; fsck only on the git-error rebuild path.
- Non-functional (release build, 24 projects)
  - First real list ≤ ~1 s after window shows; fresh statuses ≤ ~2.5 s with no new snapshot on a warm network; focus reload ≤ 300 ms.
  - No behaviour change in sync semantics; all existing tests green.
  - Reduced motion respected (`motion-reduce:animate-none`).

## Architecture
```
startup ─ restore_engine ─ light recovery (clear locks, rev-parse)         (no gc/fsck)
UI ─ run("list"): listProjects(false) ─▶ show  ─▶ listProjects(true) ─▶ show
                     │                               │
                     ▼                               ▼
dashboard::load(fetch) ─ [Waiting] lock ─ [Checking] fetch (no ls-remote, [Download %])
                       ─ manifests: cache hit by sha │ miss → ls-tree p/ + ONE `cat-file --batch`
                       ─ [Evaluate i/n] summarize per project on ≤4 scoped threads (stable order)
watcher (idle ≥10 min, lock free, weekly) ─ gc (reflog expire + gc --prune=now)
```
- `ManifestCache { sha, manifests }` lives in `DashboardCache`; reset with the engine generation.
- `overview::list(engine, sha, manifests: &[Manifest])` (caller supplies manifests) — keeps engine pure, cache stays app-side.

## Related Code Files
- Modify
  - `src-tauri/src/engine/git_process.rs` — `run_with_input` (stdin bytes) for `cat-file --batch`.
  - `src-tauri/src/engine/store_repo.rs` — `fetch` without `ls-remote` (map "couldn't find remote ref main" → `Ok(None)` + forget last snapshot); `read_blobs(sha, paths)` via `cat-file --batch`; split `recover()` into light startup part + `gc_if_due()`.
  - `src-tauri/src/engine/remote.rs` — `all_manifests` uses `read_blobs`.
  - `src-tauri/src/engine/overview.rs` — `list` takes manifests; parallel `summarize` with `std::thread::scope`; progress `Evaluate`.
  - `src-tauri/src/engine/progress.rs` — `Waiting`, `Checking`, `Evaluate { current, total }`.
  - `src-tauri/src/engine/lock.rs` — poll 50 ms.
  - `src-tauri/src/engine/maintenance.rs` — startup = light recovery + temp sweep + prune (no gc/fsck).
  - `src-tauri/src/dashboard.rs` — timing logs per phase, manifest cache, progress stages.
  - `src-tauri/src/watcher.rs` — idle weekly gc.
  - `src/lib/use-dashboard.ts` — 2-phase initial load; phase-1 errors other than busy fall through to phase 2.
  - `src/features/dashboard/{dashboard-page,dashboard-header,project-sidebar}.tsx`, `src/lib/tauri-commands.ts`, `src/locales/{vi,en}.json`.
- Create
  - `src/features/dashboard/loading-skeleton.tsx` (sidebar + detail skeletons).
  - `src-tauri/tests/batch_manifests.rs` (or extend an existing test file if it stays < 200 lines).

## Implementation Steps
1. **Measure (before)**: add `log::info!("dashboard load: wait={}ms fetch={}ms manifests={}ms evaluate={}ms total={}ms fetch={}", …)` in `dashboard::load`, and timing for startup maintenance. Build release (`npx tauri build --no-bundle`), open app 3×, record numbers in this file.
2. `lock.rs`: poll interval 500 → 50 ms.
3. Maintenance split: `StoreRepo::recover()` → light (clear locks, rev-parse check, ensure_clone); `StoreRepo::gc_if_due()` holds weekly fsck-free `reflog expire` + `gc --prune=now` + stamp. `maintenance::on_startup` calls only light recovery + sweeps. Watcher: after ≥10 min uptime, every loop, `try_acquire` lock → `gc_if_due()`. Rebuild-on-git-error paths keep their fsck-free rebuild.
4. `fetch` without `ls-remote`: run `fetch --progress --depth 1 --no-tags origin main`; stderr "couldn't find remote ref" → `remember(None)`, `Ok(None)`. Keep `remote_head()` for the watcher and key setup only.
5. Batch reads: `GitEnv::run_with_input(dir, args, stdin, timeout)`; `StoreRepo::read_blobs(sha, &[path]) -> Vec<Option<Vec<u8>>>` via `cat-file --batch` (parse `<oid> <type> <size>\n` + exactly `size` bytes + `\n`; `<spec> missing\n` → None). `remote::all_manifests` = `list_dirs` + `read_blobs`.
6. Manifest cache: `DashboardCache.manifests: Option<(String, Arc<Vec<Manifest>>)>`; `load` reuses when `sha` equal; `overview::list` takes `&[Manifest]` (callers in tests pass `remote::all_manifests`).
7. Parallel evaluation: in `overview::list`, after sequential link classification, evaluate projects with `std::thread::scope` over `available_parallelism().min(4)` chunks; keep result order; report `Progress::Evaluate { current, total }` (throttled sink).
8. Progress stages: `dashboard::load` reports `Waiting` before lock wait (only if the first try is busy), `Checking` before fetch.
9. UI: `loading-skeleton.tsx` (6 sidebar rows, detail title + 4 lines; Chalk bars, `animate-pulse motion-reduce:animate-none`); sidebar empty texts only when `data` loaded; header maps new steps to labels (`progress.waiting`, `progress.checking`, `progress.evaluate`), indeterminate bar when no ratio.
10. `use-dashboard.ts`: initial `run("list")` = `listProjects(false)` → `show` (ignore `busy`), then `listProjects(true)` with busy-retry (existing loop). Header shows "Đang kiểm tra GitHub…" via `Checking` event.
11. Locales vi/en for new keys; parity check.
12. **Measure (after)** on release; compare with step 1; puppeteer probe for no page overflow and skeleton visible before data.

## Todo List
- [x] Timing logs + baseline numbers (release)
- [x] Lock poll 50 ms
- [x] Maintenance split + idle weekly gc in watcher (fsck dropped: git errors already rebuild the clone)
- [x] Fetch without ls-remote
- [x] Batch manifest reads (`cat-file --batch`)
- [x] Manifest cache per snapshot sha
- [x] Parallel project evaluation + Evaluate progress
- [x] Waiting/Checking progress stages
- [x] Skeletons + header stage labels + empty-state fix
- [x] 2-phase initial load (`local_projects`: None before any snapshot)
- [x] Locales + tests + after numbers

## Measurements (release, 24 projects, 3 opens each, 2026-09-24)
| | Before | After |
|---|---|---|
| Startup maintenance | 94–99 ms | 53–101 ms (never gc/fsck) |
| First load | 4.6–5.1 s (fetch 2.7 s + list 1.8–2.3 s) | phase 1 (no network) 0.16–0.23 s |
| Fresh load | same load | 0.2–1.7 s (fetch one round trip ~1.6 s; list 58–63 ms with cached manifests) |
| Extra load at startup | 9.1–10.6 s (duplicate publish waiting on the lock) | removed |
| Seen by the user (from launch) | list after ~5 s | skeleton 0.33–0.74 s, list 0.57–1.04 s, fresh 1.2–2.3 s |
| Manifest reads | ~50 git spawns | list_dirs + 1 `cat-file --batch` |

Evaluation of local files takes 20–40 ms with bases present: parallelism matters only for projects without a base.

## Success Criteria
- [ ] Release: first list ≤ ~1 s, fresh ≤ ~2.5 s (no new snapshot), focus reload ≤ 300 ms (numbers logged here)
- [ ] No empty-state text while loading; each wait shows a stage label
- [ ] Startup never runs gc/fsck; weekly gc observed from the watcher when idle
- [ ] Tests: batch reads equal per-file `show` (incl. missing object); fetch on empty remote → `None` and forgets last snapshot; parallel list gives the same statuses/order as before (existing sync/conflict tests); all green, clippy clean, tsc + build OK

## Risk Assessment
- `cat-file --batch` parsing errors → exact-length reads, test with binary blobs and a missing path; fall back to per-file `show` on protocol mismatch? (no — fail loudly; tests guard).
- Threads × git spawns for rare content decisions → cap at 4 threads; engine types must be `Sync` (compile-time check).
- Phase-1 statuses stale for a few seconds → header "Đang kiểm tra GitHub…" until phase 2 completes.
- gc deferred → store grows until an idle window; weekly cadence unchanged, `recover()` rebuild path unchanged.
- Fetch error text for a missing ref may vary by git version → match "couldn't find remote ref" (LC_ALL=C already set) and test against local git.

## Security Considerations
- No new inputs from the webview; progress payloads carry counts only.

## Next Steps
- Then Phase 5 (hooks). Hook workers will contend for `sync.lock` more often; the 50 ms poll and Waiting stage help there too.

## Review Log (2026-09-24)
Code review 7/10 (`reports/code-reviewer-260924-phase-04b-loading.md`). Fixed:
- H1: the initial load no longer depends on the effect that started it (StrictMode double effects in dev left the skeleton forever); only the busy retry stops on unmount, or once a publish brought data.
- M1: `list` reads the last snapshot under its own lock and, after a fetch, waits as long as the fetch did; a busy list after a fetch is an error (retry), never the old view marked fresh. The cache records the snapshot of the shown view; the watcher also refreshes when the clone knows a newer one (e.g. a hook save).
- M2: manifest paths come from one recursive listing (`p/<hash>/manifest.age` only), so stray entries under `p/` are ignored and a missing object is a real error; a git error while reading rebuilds the clone (the next fetch downloads it again).
- M3: a failed first load shows a message and "Check again" instead of an endless skeleton.
- M4: weekly gc clears stale git locks, uses `pack.window=0` (encrypted chunks never delta), writes the stamp only on success, runs after 2 min uptime (measured 1.3–1.5 s on a copy of the 391 MB store).
- Lows: background progress no longer shows during a user action (only transfer steps), stricter batch parser (blob type, trailing newline), exact missing-ref match, `list` reports Waiting, always-mounted live region, progress bar label, `aria-busy` on the list.
- Tests: `tests/batch_reads.rs` also covers stray entries under `p/`, project order with two projects, manifest cache invalidation on a new snapshot.
- Not done: `Arc` for cached manifests (negligible at 24 projects), duplicate hashing before the first sync of a new machine (pre-existing).
- After fixes (release): list 0.54–0.93 s after launch, fresh 2.2–2.6 s, no page overflow.
