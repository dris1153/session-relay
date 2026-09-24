# Changelog

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Nothing has been released yet.

## Unreleased (v0.1.0)

### Added
- Sync core: per-project manifests, line-aligned chunking of transcripts (≈4 MiB), zstd + age encryption, keyed names, one orphan snapshot commit pushed with `--force-with-lease`, three-way state per file, encrypted backups of anything overwritten, rollback detection, path rewriting between machines.
- GitHub App device-flow sign-in with expiring, rotating tokens in Windows Credential Manager (Local persistence); storage check over the REST API; passphrase-wrapped age identity.
- Onboarding, dashboard (projects by owner, sessions, activity), settings, tray (Open · Save all · Auto-save · Quit), autostart, Vietnamese and English.
- Two-phase dashboard load (statuses from the last snapshot on disk first), progress steps, skeletons; batch manifest reads and parallel evaluation.
- Auto-save through Claude Code `Stop` / `SessionEnd` hooks: pending markers, detached push-only worker, retries from the app, flush on Quit, hooks removed on uninstall.
- New-machine flow: find existing checkouts in the workspace folders or clone the repository, then restore; per-file conflict dialog for diverged sessions.
- App icon: two ink arcs handing over a Clay dot (source `src-tauri/app-icon.png`; regenerate with `npx tauri icon src-tauri/app-icon.png`).
