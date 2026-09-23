---
phase: 6
title: "Workspace Scan, Clone & Conflicts"
status: pending
priority: P2
effort: "1.5d"
dependencies: [4, 5]
---

# Phase 6: Workspace Scan, Clone & Conflicts

## Context Links
- plan.md §IPC Contract (`scan_workspaces`, `clone_and_link`, `sync_project(files, force_*)`); Red Team #3, #14
- Phase 2 `project_identity::origin_key_for`, `links`

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
- [ ] workspace scan + test
- [ ] link panel
- [ ] clone with cancel
- [ ] conflict dialog + per-file force
- [ ] tray rule for actionable not_linked

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
