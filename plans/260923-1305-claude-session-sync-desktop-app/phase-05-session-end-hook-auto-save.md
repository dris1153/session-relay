---
phase: 5
title: "Auto-Save Hooks (Stop + SessionEnd)"
status: completed
priority: P1
effort: "2d"
dependencies: [2, 4]
---

# Phase 5: Auto-Save Hooks (Stop + SessionEnd)

## Context Links
- plan.md §IPC Contract (exe subcommands); Red Team #5, #8(scope), #15
- Claude Code hooks: https://code.claude.com/docs/en/hooks (common input: session_id, transcript_path, cwd, hook_event_name)
- `reports/spike-results.md` Spike B (VS Code firing, shell quoting, worker survival)

## Implementation Notes (2026-09-24)
- `engine/claude_hook_config.rs`: status/install/uninstall on `<claude_home>/settings.json` (exec form, `timeout: 10`, backup `settings.json.bak-<ts>` before every write, malformed shape never written).
- `hook.rs`: `hook-save` (≤64 KB stdin, BOM tolerated, transcript canonicalized and required inside `<claude_home>/projects/`, marker, detached worker; always exit 0, no stdout) and `hook-worker` (sleep, newest-marker check, push-only save). `pending.rs`: one file per turn (`pending/<enc>/<stamp>`), so clearing covered markers never deletes a newer one.
- `save_pending` classifies the dir under the lock (learns new links), pushes with `SyncMode::PushOnly`, keeps the marker only for failures that may pass (busy, offline, signed out, I/O).
- `auto_save.rs` (GUI): switch via `set_auto_save`, silent repair of a stale exe path at startup, watcher finishes markers older than 3 min, tray "Quit" finishes all markers (≤30 s) first. Tray has an "Auto-save" check item.
- Onboarding's last step offers auto-save (default on) and now also registers autostart (release builds).
- Failures: the dashboard lists projects whose latest activity is a failed auto-save (`auto_save_failures`); the tray shows attention and the detail pane a note. No toast (no notification plugin).

## Overview
User works mostly in the VS Code extension where `SessionEnd` is rare. `Stop` (end of every Claude turn) + `SessionEnd` both run `session-relay.exe hook-save`, which writes a durable pending marker and spawns a debounced detached worker (push-only). GUI retries stale markers. One toggle installs/uninstalls both hooks.

## Key Insights
- Hook path never starts Tauri: `main.rs` dispatches subcommands before Builder.
- Debounce: `Stop` → worker sleeps 120 s, exits if a newer marker for the same dir exists; `SessionEnd` → delay 0.
- Worker mode = PushOnly: never pulls (files may be open in Claude), never forces, skips Diverged.
- Hook entry uses exec form `{"type":"command","command":"C:/…/session-relay.exe","args":["hook-save"]}` (no shell; shell forms leaked handles and hung Claude — Spike B).
- Before spawning the worker, `hook-save` clears inheritance on its std handles (`SetHandleInformation(…, HANDLE_FLAG_INHERIT, 0)`); worker stdio = null; flags `CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB` (fallback without breakaway).
- Source of truth for "installed?" = `<claude_home>/settings.json` itself (no duplicate flag).

## Requirements
- `hook-save`: read stdin ≤ 64 KB, validate `transcript_path` component-wise inside `<claude_home>/projects/` (claude_home from app settings, not env), map parent dir → link (else run `links::discover` for that dir; Secondary/unknown → record skip), write `pending/<enc>.json {at, event}` atomically, spawn worker, exit 0 in < 300 ms, never print to stdout.
- `hook-worker --dir <enc> --delay <s>`: sleep; newer marker → exit; `try_lock` wait ≤ 60 s → busy: keep marker, exit; `sync_project(PushOnly)` → success: delete marker if `at` unchanged; append activity `{source:"hook"}`.
- GUI: on start, every 60 s, and before Thoát → markers older than 3 min → run PushOnly.
- Hook status: Installed | NotInstalled | StalePath (auto-fixed silently on start) | Malformed (settings.json invalid → never write, show message).

## Architecture
```
main.rs                     "hook-save" | "hook-worker" | "git-credential" | GUI
hook.rs                     entry + worker (spawn flags from Spike B)
pending.rs                  marker read/write/sweep
engine/claude_hook_config.rs  install/uninstall/status on <claude_home>/settings.json:
                            hooks.Stop[] and hooks.SessionEnd[] entry {"hooks":[{"type":"command","command":"C:/…/session-relay.exe","args":["hook-save"],"timeout":10}]}
                            ours = command ends with "session-relay.exe" and args == ["hook-save"]; idempotent; preserve other keys/hooks
                            (serde_json preserve_order); backup settings.json.bak-<ts> before write; refuse on parse error
```

## Related Code Files
- Modify: `src-tauri/src/main.rs`, `src-tauri/src/commands.rs` (install_hooks, uninstall_hooks; HookStatus in AppState), `src-tauri/src/tray.rs` (Tự lưu toggle), `src/features/settings/settings-page.tsx`
- Create: `src-tauri/src/{hook,pending}.rs`, `src-tauri/src/engine/claude_hook_config.rs`, `src-tauri/tests/claude_hook_config.rs`
- Modify: `src/features/onboarding/onboarding-flow.tsx` (final step offers "Bật tự lưu" now that it exists)

## Implementation Steps
1. `claude_hook_config.rs` + tests: empty file, existing Stop/SessionEnd hooks from other tools, reinstall idempotent, uninstall restores others exactly, malformed JSON refused, stale path detected/fixed.
2. `main.rs` dispatch; `hook.rs` entry (validation, marker, spawn) + worker (debounce, lock wait, PushOnly).
3. `pending.rs` + GUI retry loop (share watcher.rs timer) + pre-quit flush (max 30 s, then quit anyway with marker kept).
4. Toggles: settings page + tray checkbox + onboarding final step → install/uninstall; status read from file each time.
5. Failures → activity entries with `source:"hook"`; GUI shows attention badge + toast for new failures since last view.

## Todo List
- [x] claude_hook_config + tests
- [x] main.rs dispatch + hook entry/worker
- [x] pending markers + GUI retry + pre-quit flush
- [x] toggles (settings, tray, onboarding)
- [x] failure surfacing via activity (dashboard note + tray attention; toast deferred)

## Success Criteria
- [ ] VS Code session: ~2 min after last Claude reply, cloud has the turn (verify from machine B); Claude UI never waits on hook
- [ ] Killing worker mid-save (or shutdown) leaves marker → GUI completes save on next start
- [ ] Diverged project never auto-overwritten; uninstall leaves other hooks byte-identical (diff) and a backup file exists

## Risk Assessment
- Worker killed on VS Code close → marker + GUI retry covers.
- Hook schema change in Claude Code → status becomes Malformed/NotInstalled, surfaced in settings.
- Many turns → at most one active save per dir (debounce + lock); cheap when unchanged (no commit).

## Security Considerations
- `transcript_path` validated component-wise after canonicalize; worker re-validates dir name against links.
- No stdout output; no content/token in logs; settings.json backup local only.

## Review Log (2026-09-24)
Code review 7/10 (`reports/code-reviewer-260924-phase-05-hooks.md`). Fixed:
- H1: a `storage-blocked` file (set when the storage check is not Ready, cleared when Ready) stops hook workers too; they run in their own processes and never saw the GUI dropping its engine. Markers stay until storage is usable.
- H2: hook stdin cap 16 MB (the Stop payload carries the whole last reply) and the rest is drained, so Claude never sees a broken pipe.
- H3: `hook-uninstall` subcommand run by the NSIS pre-uninstall hook (`src-tauri/windows/installer-hooks.nsh`, skipped on updates); a `claude_home` change moves the hooks to the new Claude folder.
- M1: stale-path repair only in release builds and only when the registered exe no longer exists.
- M2: failures only for projects on screen; the watcher refreshes locally when the activity file changes (worker outcomes).
- M3: Quit is single-shot and exits after at most 30 s whatever the saves do.
- M4: a symlinked `settings.json` is written through; backups capped at 5 with millisecond names.
- M5: orphan threshold 15 min (longer than a live worker), exponential backoff (1 min → 30 min) for failed GUI retries.
- M6: toggling auto-save no longer resets unsaved settings; stale path shows as on in the tray too; the page picks up tray changes on focus.
- Lows: pending folders are kept (no race with the next marker), worker spawned with `CreateProcessW` inheriting no handle at all and cwd = app data dir, `hook-save` catches panics and refuses UNC transcript paths before probing, `install` is a no-op when already installed, future-dated markers count as stale, `SessionEnd` waits 5 s, a save is forced when turns kept coming for 10 min, BOM accepted in Claude's settings, disabled checkbox label shows not-allowed, onboarding shows the autostart choice.
- Tests: `tests/claude_hook_config.rs` (+BOM, no-op reinstall, backup cap), `tests/hook_save.rs`, unit tests for path validation and a 400 KB payload.
- Pending manual checks (user): VS Code round trip, kill a worker mid-save → GUI finishes it, uninstall leaves other hooks intact. NSIS hook compiles only at bundling (Phase 7).
