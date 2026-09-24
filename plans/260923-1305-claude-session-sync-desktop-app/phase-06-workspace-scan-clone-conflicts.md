---
phase: 6
title: "Workspace Scan, Clone & Conflicts"
status: completed
priority: P2
effort: "1.5d"
dependencies: [4, 5]
---

# Phase 6: Workspace Scan, Clone & Conflicts

## Context Links
- plan.md §IPC Contract (`scan_workspaces`, `clone_and_link`, `sync_project(files, force_*)`); Red Team #3, #14
- Phase 2 `project_identity::origin_key_for`, `links`

## Implementation Notes (2026-09-24)
- `engine/workspace_scan.rs`: walks each workspace root (depth ≤ 4; skips hidden, `node_modules`, `target`, `dist`, `build`; never descends into a checkout; links/junctions not followed) and reports `github.com/owner/repo` + path via `project_identity::locate`. Real run: 50 checkouts under `D:\Workspace` in 0.25 s.
- `engine/git_clone.rs`: `git clone --progress https://<remote>.git <root>\<repo>` with the user's own git setup (GCM), no terminal prompts, percent to the header, cancel = kill process tree, a failed or cancelled clone removes its folder. `root` must be a configured workspace root and the project must be `not_linked`.
- UI: `link-project-panel.tsx` in the detail pane of a `not_linked` project (found checkouts → "Liên kết & Sao chép"; none → "Clone về <root>\<repo>" with root picker; manual folder; rescan). `conflict-dialog.tsx` from the new primary action "Xử lý…" on `diverged`: per file, this machine (size, modified) vs cloud (machine, size, saved) → `sync_project(force_local|force_remote, [rel])`; the discarded side is backed up by the engine.
- `FileRow` gained `local_size`, `local_modified`.
- Deferred: tray attention for not_linked projects with a scan match (would scan on every load).

## Overview
Speed up linking on new machines (scan workspace roots, one-click clone) and resolve Diverged sessions via a dialog. Manual folder linking already exists (phase 4).

## Key Insights
- Scan reuses `origin_key_for` (same parser as discovery) → worktrees/`.git` files consistent.
- Clone uses the machine's own git credentials (GCM), not the app token (token only covers storage repo).
- Conflict dialog uses manifest/base metadata only (size, time, machine) — no full decrypt.

## Requirements
- Scan: walk each root depth ≤ 4, skip `node_modules`, `.git`, `target`, `dist`, hidden dirs; results in memory (rescan when panel opens or roots change); multiple matches per key → list, user picks.
- not_linked detail: match found → "Tìm thấy tại …" + [Liên kết & Sao chép]; none → [Clone về <root>\<repo>] (root selector if > 1) + existing [Liên kết…].
- Clone: `git clone https://github.com/<owner>/<repo>.git <dest>` with user's normal git env (not isolated), spinner, cancel = kill process tree; then link → sync.
- Conflict dialog ("Xử lý…"): per diverged file: "Máy này · {size} · sửa {time}" vs "{machine} · {size} · lưu {time}"; Giữ bản này / Giữ bản cloud → `sync_project(force_local|force_remote, files=[rel])`; discarded side backed up encrypted (phase 2 backup rules).

## Architecture
```
engine/workspace_scan.rs   scan(roots) → Vec<(KeyHash, remote, PathBuf)> via origin_key_for
engine/git_clone.rs        clone(url, dest) with cancel handle
UI features/dashboard/{link-project-panel,conflict-dialog}.tsx
```

## Related Code Files
- Create: `src-tauri/src/engine/{workspace_scan,git_clone}.rs`, `src/features/dashboard/{link-project-panel,conflict-dialog}.tsx`, `src-tauri/tests/workspace_scan.rs`
- Modify: `src-tauri/src/commands.rs`, `src/features/dashboard/project-detail-pane.tsx`

## Implementation Steps
1. `workspace_scan.rs` + test on temp tree (real `.git/config`, worktree `.git` file, nested repos, skipped dirs).
2. Link panel (match / clone / manual) + commands.
3. `git_clone.rs` with cancel; post-clone link + sync.
4. Conflict dialog + per-file force; refresh after apply.
5. Tray attention also for not_linked projects that have a scan match (actionable).

## Todo List
- [x] workspace scan + test
- [x] link panel
- [x] clone with cancel
- [x] conflict dialog + per-file force
- [ ] tray rule for actionable not_linked (deferred)

## Success Criteria
- [ ] New machine, repo not cloned: Clone về → sessions restored → visible in VS Code past conversations, ≤ 3 clicks after onboarding
- [ ] Diverged session resolved either way; discarded version exists as encrypted backup
- [ ] Scan of 2 roots × ~200 repos < 3 s

## Risk Assessment
- Same remote cloned twice → explicit choice; other dir stays Secondary (phase 2 rule).
- Private client repo clone needs user's credentials → failure message suggests signing in with git/GCM.

## Security Considerations
- Clone URL built only from normalized key; dest path must be inside a configured workspace root.
- Linking a folder whose origin mismatches the key requires explicit confirmation (phase 4 flow).

## Review Log (2026-09-24)
Report: [code-reviewer-260924-phase-06-scan-clone.md](./reports/code-reviewer-260924-phase-06-scan-clone.md) · 7.5/10, 0 critical, 1 high, 4 medium, 12 low.

Fixed:
- H1/M1: one clone at a time (`busy`), the clone claims its folder with `create_dir` (only a folder it created is deleted), a cancel before git starts is honoured, partial folders are removed with retries and, if one survives, lose `.git/config` so the scan never offers them. Integration test covers failure, cancel and the busy guard.
- M2: `destination_exists` error with guidance (en/vi).
- M3: the scan re-runs every time the link panel opens (incl. after Settings).
- M4: the conflict dialog shows why a file was skipped (e.g. its Claude session is open).
- L1 `--` before url/dest, the project name must be one path component and the remote must re-normalize to itself; L2 `cancel_clone` off the main thread; L3/L10 clone and resolve state keyed by project, Close has initial focus; L4 cancel is silent; L5 own `clone` progress step; L6 `LC_ALL=C`, error detail keeps `error:` lines; L8 scan skips `AppData` and `$*`, walks a root that is itself a checkout, stops after 20 000 folders; L9 stale clone root; L11 a link learned during the clone is not overwritten.

Not changed: L7 git/GCM left running when quitting mid-clone (job object, deferred with the store's git); L12 folder name is the lowercased repo name (the key is lowercased; the original case would need the GitHub API).
