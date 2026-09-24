# Development Roadmap

Target of the first release (v0.1.0): Windows, one GitHub account, save and restore Claude Code sessions per GitHub project through an encrypted private repository, with auto-save. Detailed phase files: `plans/260923-1305-claude-session-sync-desktop-app/`.

## Phases

| Phase | Scope | Status |
|---|---|---|
| 1 | Feasibility spikes (isolated git, age + keyring, device flow without secret, Claude hook payloads, path encoding) and scaffold | Complete |
| 2 | Rust sync core: manifests, chunking, encryption, three-way state, orphan snapshots with lease push, backups, locks | Complete |
| 3 | GitHub App sign-in, storage check, key creation/unlock, onboarding | Complete |
| 4 | Dashboard, linking, tray, autostart, vi/en | Complete |
| 4b | Dashboard loading speed (two-phase load, batch manifest reads, parallel evaluation) and skeleton UI | Complete |
| 5 | Auto-save through Claude Code `Stop` / `SessionEnd` hooks | Complete; manual checks move to the Phase 7 E2E |
| 6 | Workspace scan, clone, conflict dialog | Complete; success criteria checked in the Phase 7 E2E |
| 7 | Docs, Claude Code version notice, installer + checksums, two-machine E2E | In progress |

## Phase 7 remaining

- [x] Claude Code version notice (tested 2.1.278–2.1.280; a newer minor gets a non-blocking notice in Settings)
- [x] Installer (`pnpm tauri build`, NSIS with the pre-uninstall hook) and `SHA256SUMS.txt`
- [ ] Install / uninstall check (hooks removed, app data kept)
- [ ] Two-machine E2E checklist, including the Phase 5 manual checks (`claude -p`, VS Code, worker surviving the terminal)

## v0.2.0 (released 2026-09-24)

- Session transcript viewer: click a session to read the whole conversation (messages, tool calls with results and diffs, nested subagents, metadata), local or cloud copy, with search, outline, Markdown export and a diverged-copy compare. Plan: `plans/260924-1148-session-transcript-viewer/`.

## v0.2.1 (released 2026-09-24)

- The app is named "Session Relay" and installs into its own folder; the installer moves 0.1–0.2 installs, their auto-save hooks and the start-with-Windows entry.

## Deferred

- Tray attention for projects that are not linked yet but have a checkout in the workspace folders (needs a scan per refresh).
- A notification when an auto-save fails while the window is hidden (the tray dot covers it for now).
- Sharing manifests between the dashboard cache and evaluation without copying (`Arc`).
- A job object so git children (store sync and project clone) die with the app on Quit.
- Code signing of the installer.
- Keep auto-save hooks and start-with-Windows when a later installer uninstalls the previous version first (the default choice on its reinstall page runs a full uninstall).

## Out of scope for v0.1.0

macOS / Linux · several GitHub accounts · Claude's `file-history` · automatic restore (pull) · rolling back the storage history · automatic retention in the storage repository (deleting sessions is manual) · key rotation.
