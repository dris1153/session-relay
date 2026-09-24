---
title: "Session Relay — Claude Code Session Sync (Tauri v2, Windows)"
description: "Tray + dashboard app that saves/restores Claude Code session data per GitHub project via an encrypted private GitHub repo."
status: in-progress
priority: P1
effort: 17d
branch: main
tags: [feature, desktop, tauri, rust, frontend, auth, crypto]
blockedBy: []
blocks: []
created: 2026-09-23
---

# Session Relay — Claude Code Session Sync

## Overview
Save (Lưu) Claude Code sessions of a GitHub project on machine A, restore (Sao chép) on B. Project key = normalized git remote + subpath. Storage = user's private repo `claude-sessions`, chunked + zstd + age-encrypted, one orphan snapshot commit pushed with `--force-with-lease`. Auth = GitHub App (device flow, repo-scoped, expiring tokens). Auto-save via Claude Code `Stop` (debounced) + `SessionEnd` hooks. UI: 2-column dashboard + tray, DESIGN.md style.

Decisions: [Brainstorm report](../reports/brainstorm-260923-1300-claude-session-sync-desktop-app.md) (superseded where Red Team Review below says so)
Research: [Tauri v2 Windows](./research/researcher-01-tauri-v2-windows-report.md) · [Rust sync core](./research/researcher-02-rust-sync-core-report.md) (versions/API corrected below)

## Phases

| Phase | Name | Effort | Status |
|-------|------|--------|--------|
| 1 | [Feasibility Spikes & Scaffold](./phase-01-feasibility-spikes-and-scaffold.md) | 1.5d | Complete |
| 2 | [Rust Sync Core](./phase-02-rust-sync-core.md) | 4.5d | Complete |
| 3 | [GitHub App Auth, Key Setup & Onboarding](./phase-03-github-auth-key-setup-onboarding.md) | 2.5d | Complete |
| 4 | [Dashboard, Linking, Tray & i18n](./phase-04-dashboard-ui-and-tray.md) | 4d | Complete |
| 4b | [Dashboard Loading Speed & Skeleton UI](./phase-04b-dashboard-loading-speed-and-skeleton.md) | 1d | Complete |
| 5 | [Auto-Save Hooks (Stop + SessionEnd)](./phase-05-session-end-hook-auto-save.md) | 2d | Complete (manual checks in Phase 7 E2E) |
| 6 | [Workspace Scan, Clone & Conflicts](./phase-06-workspace-scan-clone-conflicts.md) | 1.5d | Pending |
| 7 | [Docs & Release](./phase-07-docs-and-release.md) | 1d | Pending |

Sequential 1→7. Phase 5 core parts may start after 2.

## Verified Versions (crates.io / docs.rs, 2026-09-23)
tauri 2.11.6 · single-instance 2.4.5 · dialog 2.7.3 · notification 2.4.0 · opener 2.5.5 · age 0.12.1 (`age::encrypt/decrypt`, `x25519::Identity::generate()`, `scrypt::Recipient/Identity`) · keyring-core 1 + windows-native-keyring-store 1.1 (modifier `persistence=Local`, verified) · zstd 0.14.0 · blake3 1.8.7 · reqwest 0.13.5 · zxcvbn 3.1.1 · tauri-plugin-autostart 2.5.1. Stay on Tauri 2.x.

## Key Dependencies
- Git for Windows ≥ 2.35 (`GIT_CONFIG_GLOBAL`, `sparse-checkout set --no-cone`), Rust ≥ 1.89 (`File::try_lock`), Node 20+, WebView2
- GitHub App registered by user (device flow on, Contents RW + Metadata R, expiring tokens); client_id/slug via build env
- Claude Code layout: `<claude_home>/projects/<encoded-cwd>/`, registry `<claude_home>/sessions/<pid>.json` (undocumented; guarded)

## IPC Contract (single source for phases 3–6)
| Command | Args → Return |
|---|---|
| `get_app_state` | → `AppState{git_version?, git_ok, has_client_id, signed_in, identity_unlocked, repo?, machine_name, claude_home, workspace_roots, language?, autostart, hooks: installed\|not_installed\|stale_path\|malformed}` |
| `start_login` / `logout` | → `LoginCode{user_code, verification_uri, expires_in}` / () ; event `auth-changed{signed_in, reason?}` |
| `check_storage` | → `StorageCheck{state: no_installation\|repo_missing\|repo_public\|repo_foreign\|needs_new_key\|needs_unlock\|ready, user{login, avatar_url}, repo?, install_url, create_repo_url}`; REST only, no clone |
| `passphrase_strength(passphrase)` | → 0–4 (create needs 3+) |
| `create_key(passphrase)` / `unlock_key(passphrase)` | → () ; re-run the storage check first; errors `weak_passphrase`, `key_exists`, `decrypt_failed`, `wrong_identity` |
| `save_settings(patch)` | → `AppState`; validates language (`vi`/`en`) and absolute paths |
| `list_projects(fetch)` | → `Dashboard{projects: ProjectView[]{key_hash, remote, owner, name, subpath, status, local_root?, files: FileRow[]{rel, state, conflict, title?, size?, saved_by?, saved_at?}, unreadable[]}, unmanaged, secondary, errors[], offline, fetched_at?, auto_save_failures[[key_hash, code]]}`; sessions are grouped from `files` in the UI |
| `project_activity(key_hash)` | → `Activity[]` (last 20) |
| `sync_project(key_hash, mode, files?)` | mode `auto\|force_local\|force_remote` → `SyncReport{pushed[], pulled[], skipped[[rel, reason]]}` |
| `save_all` | → `SaveAllItem[]{key_hash, report?, error?}` (push-only over local_ahead/both) |
| `link_project(key_hash, local_root, force)` | → `{origin_matches, found_remote?}`; links only when the origin matches or `force` |
| `delete_remote_session(key_hash, session_id)` · `open_project_folder(key_hash)` · `clear_local_data` | → () |
| `set_auto_save(enabled)` | → `AppState` (installs/removes the hooks in `<claude_home>/settings.json`) |
| `local_projects` | → `Dashboard?` (no network; None before the first download) |
| events | `projects-changed(Dashboard)` · `sync-progress{step: download\|upload (percent) \| restore\|save (current, total)}` · `storage-changed` · `auth-changed` |
| `scan_workspaces` · `clone_and_link(key_hash, root)` (phase 6) | → `ScanMatch[]` / () |

Status enum: `synced | local_ahead | remote_ahead | both | diverged | not_linked | no_remote`. Errors `{code, detail}` — UI maps codes via locale files (`vi`, `en`); Rust never returns UI text (tray labels are read from the same locale files). Exe subcommands: `hook-save`, `hook-worker --dir <enc> --stamp <n> --delay <s>`, `git-credential get`. Hooks registered in exec form (`command` + `args`).

## Out of Scope (v1)
macOS/Linux · `file-history` · auto-pull · multi-account · remote history rollback · auto remote retention (manual delete only)

## Conventions
Rust files snake_case, TS kebab-case, files < 200 lines. Tests: real git (local bare repo), real crypto, temp dirs, no mocks. All app data in `app_local_data_dir()` = `%LOCALAPPDATA%\dev.sessionrelay.desktop\` (Tauri identifier; shared with WebView2 data), never the install dir `%LOCALAPPDATA%\session-relay\` (holds the exe). "Xoá dữ liệu cục bộ" deletes only our subfolders (store, base, backups, pending, logs, settings). UI phases verify CSP with `npm run tauri build -- --debug` (dev server bypasses CSP). Release profile keeps unwinding (no `panic = "abort"`) so the panic hook can log.

## Red Team Review

### Session — 2026-09-23
**Findings:** 39 raw → 15 deduped (15 accepted, 0 rejected; minor sub-suggestions dropped: build provenance attestation, time-based identity lock)
**Severity breakdown:** 4 Critical, 8 High, 3 Medium · User decisions: GitHub App auth; Stop+SessionEnd debounced auto-save

| # | Finding | Severity | Disposition | Applied To |
|---|---------|----------|-------------|------------|
| 1 | Status refresh resets shared clone outside lock; crash leftovers | Critical | Accept | Phase 2, 4 |
| 2 | Last-`\n` truncation on non-jsonl (meta.json → 0 B); double read; no-op saves commit | Critical | Accept | Phase 2 |
| 3 | Identity from first cwd breaks on restored files; duplicate dirs per key | Critical | Accept | Phase 2, 4, 5 |
| 4 | OAuth `repo` scope too broad/unrevocable → GitHub App | Critical | Accept (user) | Phase 1, 3 |
| 5 | SessionEnd rare in VS Code; hook quoting; no retry | High | Accept (user) | Phase 1, 5 |
| 6 | Restore over live idle session silently branches | High | Accept | Phase 2 |
| 7 | BaseState per-file, mtime, no-decrypt status, single sync fn | High | Accept | Phase 2 |
| 8 | Mutable files (memory/*.md) stuck Diverged | High | Accept | Phase 2 |
| 9 | Claude 30-day cleanup → resurrection, unbounded remote | High | Accept | Phase 2, 4 |
| 10 | Git inherits global config; token in env; no timeouts | High | Accept | Phase 1, 2 |
| 11 | Manifest path traversal; no integrity/anti-rollback | High | Accept | Phase 2 |
| 12 | Absolute tool-results paths differ per machine/profile | High | Accept | Phase 1, 2 |
| 13 | Roaming credentials/data, plaintext unbounded backups, weak passphrase | Medium | Accept | Phase 2, 3 |
| 14 | IPC contract drift; phase 5/6 bloat; custom manifest; CSP null | Medium | Accept | plan.md, 1, 4–7 |
| 15 | Titles (`ai-title`), git preflight, key race, claude_home, version check | Medium | Accept | Phase 2, 3, 5 |

## Validation Log

### Session 1 — 2026-09-23
**Verification:** skipped claim sampling (Red Team Review already verified with file:line + local data evidence); crate versions re-checked on crates.io.
**Questions asked:** 3 (+ 2 decisions during red team)

| Decision | Answer | Propagated To |
|---|---|---|
| Auth model | GitHub App (device flow, repo-scoped, 8 h tokens + refresh) | Phase 1, 3 |
| Auto-save trigger | `Stop` (debounced 120 s) + `SessionEnd`, pending markers, GUI retry | Phase 1, 5 |
| App name | `session-relay` (binary, `%LOCALAPPDATA%\dev.sessionrelay.desktop`, keyring service, repo marker, `SR_` build env) | all phases |
| UI language | Vietnamese + English from v1 (locale files, English keys in code) | Phase 3, 4 |
| Autostart | On by default, starts hidden in tray, toggle in Settings | Phase 4 |

Recommendation: proceed to implementation starting with Phase 1 spikes; do not start Phase 2 until spike-results.md has no unresolved FAIL.
