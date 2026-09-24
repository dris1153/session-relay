---
phase: 4
title: "Dashboard, Linking, Tray & i18n"
status: completed
priority: P1
effort: "4d"
dependencies: [3]
---

# Phase 4: Dashboard, Linking, Tray & i18n

## Context Links
- plan.md §IPC Contract (status enum, commands, events); Red Team #1, #3, #9, #14
- DESIGN.md — tokens, Filled Dark Button, Status Badge, Editorial Section Header
- [Tauri research §2 tray, §3 single-instance](./research/researcher-01-tauri-v2-windows-report.md)

## Implementation Notes (2026-09-23)
- Rust: `dashboard.rs` (view cache + `load`/`publish`), `watcher.rs`, `tray.rs`, `autostart.rs`, `commands/dashboard.rs`; engine: `progress.rs` (throttled sink), stall-based git transfers (`run_watched`, 120 s without output), `last_snapshot()` from `refs/session-relay/last` (set by every fetch and push, so statuses stay right offline and right after a save).
- Startup unlocks from the cached identity with no network; storage is checked behind the dashboard. Maintenance waits for the lock and publishes when done.
- `failure()` classifies git errors: credentials/403/not found → `auth_rejected`, network → `network`; only real git failures rebuild the clone. `LC_ALL=C` keeps messages parseable.
- Frontend: `features/dashboard/*`, `features/settings/settings-page.tsx`, `lib/{use-dashboard,status-copy,sessions,format}.ts`, `components/{confirm-dialog,folder-fields}.tsx`. Sessions are grouped from file rows in the UI.
- Deferred: error toasts while the window is hidden (needs the notification plugin; Phase 5 hooks are the main source of background errors); killing an in-flight git on Quit (job object, with Phase 5 process handling); manifest cache per snapshot for large project counts; per-row "checking…" state.

## Overview
2-column dashboard: sidebar project list + detail pane with one primary action per status, sessions list, activity. Minimal linking (folder picker + origin validation) so the main A→B flow works without phase 6. Tray with attention badge. Close → hide to tray; second launch focuses window. UI in Vietnamese + English (locale files). Autostart with Windows (default on, starts hidden in tray).

## Key Insights
- All buttons call `sync_project(mode=auto)`; only the label differs by status. Force actions in ⋯ with confirm.
- Remote change detection: 60 s timer runs `git ls-remote` (no lock, no worktree); only when head changed → fetch under `try_lock` (busy → keep cached status).
- Monochrome glyphs, Clay only for attention dot.
- Vietnamese copy in this plan = values of `vi` locale; code uses English keys only (user rule: string literals in English).
<!-- Updated: Validation Session 1 - name session-relay, vi+en i18n, autostart default on -->

## Requirements
- Sidebar: group by owner, glyph + name, filter "Cần chú ý" / "Tất cả", footer Settings + avatar; keyboard ↑/↓/Enter.
- Status → label/button:
  | status | glyph | sentence | button |
  |---|---|---|---|
  | local_ahead | ● | Chưa lưu | Lưu |
  | remote_ahead | ○ | Cloud mới hơn · lưu từ {machine} lúc {time} | Sao chép |
  | both | ◑ | Cả hai phía có thay đổi | Đồng bộ |
  | diverged | ◐ | Lệch 2 máy | Xử lý… (phase 6; until then ⋯ force only) |
  | not_linked | ◌ | Chưa có trên máy này | Liên kết… |
  | synced | ✓ | Đã khớp | — |
  | no_remote | – | Không có remote GitHub | — (greyed) |
- Detail: name (serif 30 px), remote (Ashen 14 px), sentence, primary button, ⋯ (Lưu đè cloud / Sao chép đè local / Mở thư mục / Copy `claude --resume`), warnings (Secondary dir, session_open skips, rollback_detected), sessions list (title, relative time `Intl.RelativeTimeFormat('vi')`, MB, per-file state; LocalDeleted rows labelled "Chỉ còn trên cloud" with row action "Sao chép phiên này" and "Xoá khỏi cloud"), activity (last 20).
- Linking: "Liên kết…" → folder dialog → `link_project` → origin mismatch → warning + explicit "Vẫn liên kết" → then sync.
- Header "Lưu tất cả" (push-only over local_ahead/both projects).
- Progress bar from `sync-progress`; SyncReport skipped reasons shown inline (`session_open` → "Đóng phiên Claude đang mở rồi thử lại").
- Tray: Mở · Lưu tất cả · Tự lưu (checkable, phase 5) · Thoát; left click shows window; icon attention variant when any project remote_ahead/both/diverged or last hook run failed.
- Settings page: machine name, claude_home, workspace roots, hooks toggle (phase 5), repo link, logout (+ revoke link), "Xoá dữ liệu cục bộ" (clone/backups/base; confirm), note "Claude tự xoá phiên sau cleanupPeriodDays ngày".
- From the Phase 3 review:
  - Offline start (M9): install the engine from the cached identity + `settings.repo`, run `check_storage` in the background, block only on a definitive non-ready state.
  - First full fetch of a large store: progress + a stall-based timeout instead of the fixed 180 s (onboarding no longer fetches).
  - `store_reset`, `wrong_identity`, `auth_rejected` from a sync route back to onboarding; 24 h private re-check in the watcher.
- i18n (helper + files created in phase 3): language setting (default from system locale: vi-* → vi else en); Rust errors `{code, params}` mapped via `errors.<code>` keys; tray menu labels via `tray_labels(lang)` in Rust, rebuilt on language change.
- Autostart: `tauri-plugin-autostart` with arg `--minimized` (start hidden in tray); enabled at end of onboarding, toggle "Khởi động cùng Windows" in Settings.
- Non-functional: 50 projects render < 100 ms; focus rings; cursor rule from tokens.

## Architecture
```
Rust
  commands.rs   list_projects, get_project_detail, sync_project, save_all, link_project, delete_remote_session,
                open_project_folder (spawn_blocking; emit sync-progress / projects-changed)
  watcher.rs    60 s loop: ls-remote → changed? try_lock fetch → recompute → emit projects-changed + tray icon
  tray.rs       TrayIconBuilder, menu, set_icon(normal|attention), CloseRequested → prevent_close + hide
  lib.rs        plugin order: single-instance FIRST (focus main window), opener, dialog, notification
React
  features/dashboard/{dashboard-page,project-sidebar,project-detail-pane,primary-action-button,project-overflow-menu,
                      session-list,activity-log,link-folder-button}.tsx
  features/settings/settings-page.tsx
  components/{status-glyph,button,confirm-dialog}.tsx
  lib/{status-copy,use-projects,i18n}.ts   (status-copy maps status → locale keys)
  locales/{vi,en}.json
```

## Related Code Files
- Create: files above, `src-tauri/icons/{tray-normal,tray-attention}.png`, `src-tauri/src/tray_labels.rs`
- Modify: `src-tauri/src/lib.rs`, `src-tauri/src/commands.rs`, `src-tauri/capabilities/default.json`, `src/app.tsx`

## Implementation Steps
1. Commands per IPC contract; key_hash validated against known list (no raw paths from UI except dialog-picked link root).
2. watcher.rs + tray + hide-on-close + single-instance focus.
3. Sidebar, detail pane, primary button, overflow + confirm ("Ghi đè bản trên cloud bằng bản trên máy này? Bản cloud cũ được sao lưu mã hóa trên máy này.").
4. Link flow.
5. Sessions list incl. LocalDeleted actions; activity.
6. Progress + skipped reasons + error toasts (only when window hidden).
7. Settings page (+ language, autostart toggles).
7b. Extend vi/en locale files (dashboard, settings, error codes); language setting.
7c. Autostart plugin + `--minimized` handling in lib.rs.
7d. Set `document.documentElement.lang` from active language; check devtools console for CSP errors in `npm run tauri build -- --debug`.
8. Empty states ("Chưa thấy phiên Claude nào có remote GitHub", "Mọi dự án đã khớp"); visual pass vs DESIGN.md (canvas #f8f8f6, cards #fff r24, hairline #e7e6e1, serif only 24/30 px, sans ≤ 580).

## Todo List
- [x] Commands + validation (`commands/dashboard.rs`; key_hash resolved through the cached view)
- [x] watcher + tray + single-instance
- [x] Sidebar + detail + primary/overflow
- [x] Link flow
- [x] Sessions list + activity
- [x] Progress/skips (toasts while hidden: deferred, see below)
- [x] Settings page
- [x] i18n (vi, en) + tray labels
- [x] Autostart (release builds only; applied on an explicit choice, `--minimized`)
- [x] Empty states + visual pass

## Success Criteria
- [ ] Each status shows exactly one correct primary action; force actions need confirm
- [ ] Machine B with repo cloned but never opened in Claude: Liên kết → Sao chép → session visible in VS Code past conversations and `claude --resume`
- [ ] Remote save from another machine flips tray to attention within ~60 s; concurrent hook save never corrupts (lock test from phase 2 + manual)
- [ ] Close hides to tray; Thoát quits; second launch focuses window
- [ ] Switching language updates all screens + tray menu without restart; no hard-coded UI text in TSX/Rust (grep check)
- [ ] After reboot app is in tray (autostart) with no window shown

## Risk Assessment
- ls-remote every 60 s offline → silent backoff, status "Ngoại tuyến".
- Hashing on first run of a new machine (no base) is heavier → per-row "Đang kiểm tra…", computed off UI thread.

## Security Considerations
- UI never passes arbitrary filesystem paths except dialog-picked link root; opener limited to github.com + resolved project folder.

## Review Log (2026-09-23)
Code review 7/10 (`reports/code-reviewer-260923-phase-04-dashboard.md`); tester found no real bug (its offline case used a deleted file:// remote, which says nothing about https). Fixed:
- H1: startup maintenance waits for the lock and publishes afterwards; the first load retries while the store is busy.
- H2: every computed view updates the tray, not only background refreshes.
- M1: the view cache lock is never held across git; a generation counter drops results from a replaced engine.
- M2: progress from background refreshes is ignored unless an action runs; the sink throttles to 100 ms per step (first and last always pass); file steps report `current` of `total`; only transfer lines count ("Receiving/Unpacking/Writing objects").
- M3/L1: `refs/session-relay/last` replaces the cached sha and FETCH_HEAD, so a just-saved project never shows as deleted from the cloud.
- M4: 403/"Permission to" → auth; git spawn failures → `io` (no rebuild); `LC_ALL=C`.
- M5: the watcher marks offline on network failure and hands auth loss to onboarding once.
- M6: focus reloads skip while an action runs or within 5 s of the last load.
- M7: storage errors also show a message; one action at a time; the selected project is pinned.
- M8: autostart is applied only when the user sets it (onboarding end, Settings), never at startup.
- Lows: UNC paths refused before probing; cache reset inside the lock in "clear local data"; poison-tolerant cache lock; tolerant tray label parsing; PowerShell-safe resume command; "open folder" opens the Claude cwd; activity label for cloud deletes; dialog label, menu arrow keys, global focus ring.
- Tests: `tests/progress_and_offline.rs` (file progress; an unreachable https remote keeps the clone and the last snapshot). Tester's three files were replaced (duplicated unit tests, conditional assertions, unsynced test files).
