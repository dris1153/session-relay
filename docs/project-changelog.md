# Changelog

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Changed
- The app is named "Session Relay" (window, shortcuts, Apps list) and installs into `%LOCALAPPDATA%\Session Relay`. Installing over 0.1–0.2 removes the old `session-relay` install first and keeps app data, sign-in and auto-save hooks; the installer points the hooks and the start-with-Windows entry at the new exe.
- Start with Windows: the entry quotes the exe path (the folder now has a space), and the switch shows what Windows will do, including a change made in Task Manager.

## [0.2.0] — 2026-09-24

### Added
- Session viewer: click a session to read its conversation (prompts, Claude's replies as Markdown, tool calls with their results, per-turn model and tokens, compactions and API errors), from this machine's file or straight from the cloud copy without restoring it. System events are hidden behind a switch; rewound prompts are left out.
- Session viewer tools: search over the whole session, an outline of prompts, copy per message, Markdown export (with subagents), and a switch between both copies of a diverged session with a marker where they part; the Resolve dialog links to it.
- Session viewer reading aids: the prompt of the turn being read stays at the top, a session opens at its newest message.
- Session viewer details: diffs of file edits (Edit, Write, shell edits), full outputs Claude saved to `tool-results/`, subagent conversations opened inline (also nested), and pasted or returned images.

### Changed
- Links in Markdown open any http(s) or mailto address in the external browser (only GitHub and Git downloads before).
- The frontend is built with pnpm (`pnpm install`, `pnpm tauri build`).

## [0.1.0] — 2026-09-24

### Added
- Sync core: per-project manifests, line-aligned chunking of transcripts (≈4 MiB), zstd + age encryption, keyed names, one orphan snapshot commit pushed with `--force-with-lease`, three-way state per file, encrypted backups of anything overwritten, rollback detection, path rewriting between machines.
- GitHub App device-flow sign-in with expiring, rotating tokens in Windows Credential Manager (Local persistence); storage check over the REST API; passphrase-wrapped age identity.
- Onboarding, dashboard (projects by owner, sessions, activity), settings, tray (Open · Save all · Auto-save · Quit), autostart, Vietnamese and English.
- Two-phase dashboard load (statuses from the last snapshot on disk first), progress steps, skeletons; batch manifest reads and parallel evaluation.
- Auto-save through Claude Code `Stop` / `SessionEnd` hooks: pending markers, detached push-only worker, retries from the app, flush on Quit, hooks removed on uninstall.
- New-machine flow: find existing checkouts in the workspace folders or clone the repository, then restore; per-file conflict dialog for diverged sessions.
- App icon: two ink arcs handing over a Clay dot (source `src-tauri/app-icon.png`; regenerate with `npx tauri icon src-tauri/app-icon.png`).
