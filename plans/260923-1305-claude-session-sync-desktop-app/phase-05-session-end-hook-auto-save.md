---
phase: 5
title: "Auto-Save Hooks (Stop + SessionEnd)"
status: pending
priority: P1
effort: "2d"
dependencies: [2, 4]
---

# Phase 5: Auto-Save Hooks (Stop + SessionEnd)

## Context Links
- plan.md §IPC Contract (exe subcommands); Red Team #5, #8(scope), #15
- Claude Code hooks: https://code.claude.com/docs/en/hooks (common input: session_id, transcript_path, cwd, hook_event_name)
- `reports/spike-results.md` Spike B (VS Code firing, shell quoting, worker survival)

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
- [ ] claude_hook_config + tests
- [ ] main.rs dispatch + hook entry/worker
- [ ] pending markers + GUI retry + pre-quit flush
- [ ] toggles (settings, tray, onboarding)
- [ ] failure surfacing via activity

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
