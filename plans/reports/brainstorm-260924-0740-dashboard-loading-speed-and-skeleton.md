# Brainstorm: dashboard loading speed + loading UI

Date: 2026-09-24 · Scope: Phase 4 follow-up (dashboard) · Status: approved by user

## Problem
- Opening the app feels slow **every time** (user); first screen waits for all steps before showing anything.
- Loading UI is wrong/weak: sidebar shows the empty-state text ("Chưa thấy phiên Claude nào có remote GitHub.") while loading; header only says "Đang xử lý…".

## Findings (scouted 2026-09-24, this machine)
- Store: 24 projects, 1210 files, 391 MB, 1 pack (compact). Local `~/.claude/projects`: 34 dirs, 1.5 GB, transcripts up to 174 MB. Base files exist for all 24.
- One git spawn ≈ 22 ms.
- First load = sequential chain, UI empty until the end:
  1. Startup maintenance holds `sync.lock`: `recover()` (rev-parse, set-url, weekly `fsck` + `gc --prune=now` on 391 MB — first ever run today 07:29), temp sweep over 1.5 GB tree, backup prune. First load's refresh waits up to 30 s, then UI retries every 3 s.
  2. `acquire_within` polls every 500 ms → up to +0.5 s per wait.
  3. Network: `ls-remote` then `fetch` = 2 round trips, each spawning the credential helper (our exe; debug build).
  4. `overview::list`: `list_dirs` + per project `ls-tree` + `cat-file` (≈ 50 spawns ≈ 1.1 s) + decrypt.
  5. Local evaluation: metadata fast path when base exists; full read + normalize + chunk + hash only without base or when both sides changed (new machine, after "clear local data").
- Debug build: own crate unoptimized → normalize/chunk loops slower than release. Measure in release.

## Decisions (user)
- Slow on every open → fix startup lock contention + per-load costs, not only weekly gc.
- Loading UI: skeleton + staged progress (no on-disk view cache).
- Add **2-phase load** (no cache file): `list_projects(false)` first against the last snapshot already on disk (≈1 s, no network), then fetch + refresh.
- Backend: measurement + quick wins + parallel project evaluation.

## Approaches considered
| Option | Verdict |
|---|---|
| On-disk dashboard cache (stale-while-revalidate) | Rejected by user (skeleton preferred); 2-phase load gives most of the gain with no new file |
| Skeleton + staged progress only | Accepted, combined with 2-phase load |
| Only fix empty-state text | Too little: perceived wait unchanged |
| Quick wins only | Accepted as base |
| + parallel evaluation | Accepted; honest note: gains mainly when hashing (no base) |

## Final design
**0. Measure**: `log::info!` durations per phase in `dashboard::load` (lock wait, refresh/network, manifests, evaluation, total) and in startup maintenance. Compare before/after on a **release** build.

**1. Backend**
- Startup does only cheap recovery (clear stale git locks, rev-parse check). Weekly `gc` moves to the watcher when idle (e.g. ≥10 min uptime, lock free); `fsck` only on the git-error rebuild path.
- `SyncLock::acquire_within` poll 500 ms → 50 ms.
- `StoreRepo::fetch` without the preceding `ls-remote`: empty remote detected from fetch's "couldn't find remote ref main" → `Ok(None)` (keeps `refs/session-relay/last` semantics). Watcher keeps `ls-remote`.
- Batch manifest reads: one `ls-tree` for `p/` + one `git cat-file --batch` process (exact byte-length parsing of binary blobs).
- In-memory manifest cache keyed by snapshot sha (in `DashboardCache`): focus reloads skip remote reads.
- Parallel `summarize` per project: `std::thread::scope`, `available_parallelism().min(4)` workers, results in stable order; link classification stays sequential before.
- New progress steps: `Waiting` (lock), `Checking` (network), `Evaluate { current, total }`; existing `Download { percent }`.

**2. UI**
- Sidebar/detail skeletons while `data === null` (monochrome Chalk bars, `animate-pulse`, `motion-reduce:animate-none`); empty-state text only after data loaded.
- Header stage text + bar (indeterminate when no percent): "Đang chờ tác vụ khác…", "Đang kiểm tra GitHub…", "Đang tải kho… N%", "Đang tính trạng thái X/Y dự án".
- 2-phase load in `use-dashboard`: `listProjects(false)` → show → `listProjects(true)` with "Đang kiểm tra GitHub…"; busy-retry kept.

## Risks
- `cat-file --batch` parsing (sizes, missing objects) → unit/integration test vs per-file `show`.
- Parallel threads may still spawn git for rare chunk reads (content decisions); cap threads.
- Stale first phase: cloud-newer projects may show "synced" for a few seconds → header says it is still checking.
- Moving gc: store growth between runs is bounded (weekly, idle); keep `recover()` rebuild on real git errors.

## Success metrics (release build, 24 projects)
- First real list visible ≤ ~1 s after window shows (phase 1); fresh statuses ≤ ~2.5 s on a warm network with no new snapshot.
- Focus reload ≤ 300 ms.
- No empty-state text while loading; every loading second has a stage label.
- Existing tests green + new tests (batch manifests, empty-remote fetch, stable parallel order).

## Next steps
- `/ck:plan` → sub-phase of Phase 4 ("4b: loading speed + skeleton"), then implement, test, review, user check.
