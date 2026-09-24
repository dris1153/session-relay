# Session Relay

[![Latest release](https://img.shields.io/github/v/release/dris1153/session-relay)](https://github.com/dris1153/session-relay/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/dris1153/session-relay/total)](https://github.com/dris1153/session-relay/releases)
[![License: MIT](https://img.shields.io/github/license/dris1153/session-relay)](LICENSE)
[![Platform: Windows 10 | 11](https://img.shields.io/badge/platform-Windows%2010%20%7C%2011-0078D6)](#requirements)
[![Built with Tauri 2](https://img.shields.io/badge/built%20with-Tauri%202-24C8DB?logo=tauri&logoColor=white)](https://tauri.app)
[![Rust 1.89+](https://img.shields.io/badge/Rust-1.89%2B-000000?logo=rust)](https://www.rust-lang.org)

A Windows desktop app that syncs your Claude Code sessions between machines, per GitHub project, through an encrypted private GitHub repository you own.

> **Unofficial.** Session Relay is not affiliated with or endorsed by Anthropic. It reads and writes Claude Code's local files (`~/.claude/projects/…`, and `~/.claude/settings.json` when auto-save is on), whose format is undocumented and may change.

## What it does

Claude Code keeps each project's sessions in `~/.claude/projects/<encoded-cwd>/`: transcripts, subagent logs, tool results and the project memory. Session Relay:

- groups those folders by **GitHub project** (the `origin` remote plus the folder Claude ran in), so the same project lines up on every machine even when it is checked out at a different path;
- **Save** pushes this machine's sessions, **Restore** brings the other machine's sessions here; each project shows one button for what it needs;
- rewrites the absolute paths Claude stores inside sessions, so restored sessions open from the other machine's checkout;
- **auto-saves** after Claude replies (Claude Code `Stop` / `SessionEnd` hooks), push-only;
- on a new machine, finds existing checkouts in your workspace folders or clones the repo, then restores its sessions;
- lets you pick a side per file when both machines changed the same session.

## Requirements

- Windows 10 or 11
- [Git for Windows](https://git-scm.com/download/win) 2.35 or newer
- A GitHub account and **your own GitHub App** (device flow, no secret) — see [docs/setup-github-app.md](docs/setup-github-app.md)
- To build: Node 20+ with pnpm, Rust 1.89+ (with the MSVC toolchain), WebView2 (preinstalled on Windows 11)

## Build

```bash
cp .env.example .env        # fill in SR_GITHUB_CLIENT_ID and SR_GITHUB_APP_SLUG
pnpm install
pnpm tauri dev              # development
pnpm tauri build            # NSIS installer in src-tauri/target/release/bundle/nsis/
```

Both `.env` values are public identifiers of your GitHub App; forks register their own app. The installer is not code-signed, so SmartScreen will warn. It installs per user into `%LOCALAPPDATA%\session-relay`.

## First run

1. **Git check** — the app needs Git 2.35+.
2. **Sign in to GitHub** with the code shown (device flow).
3. **Storage** — create an empty private repository (default name `claude-sessions`) and install your GitHub App on that repository only. The app links to both pages.
4. **Passphrase** — the first machine creates one (it must be strong); other machines unlock with the same passphrase. **Losing it means losing the data**; nobody can recover it.
5. **This machine** — machine name, Claude data folder, workspace folders, start with Windows, auto-save.

## Everyday use

The window lists your projects by owner. Each status has one primary button:

| Status | Meaning | Button |
|---|---|---|
| Not saved yet | this machine has new work | Save |
| The cloud is newer | another machine saved | Restore |
| Both sides have changes | different files changed on each side | Sync |
| The two machines diverged | both changed the same session | Resolve… (pick a side per file) |
| Not on this machine yet | the repo has no checkout here | Link… (or use the link panel: link a found checkout, or clone) |
| In sync | nothing to do | — |

The `⋯` menu has "overwrite the cloud with this machine", "overwrite this machine with the cloud" (both ask first), open folder, and copy a `claude --resume` command. Whatever gets overwritten is first backed up, encrypted, on this machine. Single sessions can be restored or deleted from the cloud in the session list.

Closing the window keeps the app in the tray (Open · Save all · Auto-save · Quit). The tray icon gets a dot when the cloud has something newer, a project diverged, or an auto-save failed. Quit first finishes pending auto-saves (at most 30 s).

### Auto-save

When enabled (onboarding, Settings or the tray), the app adds two hooks to `~/.claude/settings.json` and keeps a backup of the previous file (`settings.json.bak-*`, last 5). After each Claude reply, the hook records a marker and returns immediately; a background process saves the project about 2 minutes after the last reply (at least every 10 minutes during long sessions, 5 s after a CLI session ends). Auto-save only pushes: it never pulls and never overwrites a diverged session. If a background save dies, the app finishes it later.

Claude reads hooks when a session starts: open a new session (or reload the VS Code window) after turning auto-save on.

## Security model

- Everything leaving your machine is compressed (zstd) and then encrypted with [age](https://age-encryption.org) (x25519). File and project names in the repository are keyed hashes, so the repository shows only sizes and timing.
- The age identity is stored in the repository wrapped by your passphrase (scrypt), as `keys/identity.age`. A strength check (zxcvbn score ≥ 3) applies when creating it.
- The GitHub App token only reaches the repositories the app is installed on (install it on the storage repository only). It expires after 8 hours and is refreshed automatically; the refresh token rotates.
- Tokens and the unlocked identity are kept in Windows Credential Manager with local (non-roaming) persistence. **Any program running as your Windows user can read them, including commands Claude runs for you.** Signing out removes them; revoke the app at <https://github.com/settings/apps/authorizations>.
- The app refuses public repositories and repositories with foreign content. It re-checks the repository daily and at start; if it became public, syncing stops, auto-save included.
- There is no key rotation: to change the passphrase or identity, create a new repository and set up again.

## Where data lives

- `%LOCALAPPDATA%\dev.sessionrelay.desktop\` — the local clone of the storage repository (`store`), sync state (`base`), encrypted backups of overwritten files (`backups`), auto-save markers (`pending`), `links.json`, `settings.json`, `activity.jsonl`, logs.
- **Settings → Clear local data** removes the clone, sync state and backups (the next check downloads the storage again). Uninstalling removes the auto-save hooks from Claude's settings but keeps this folder and the Credential Manager entries: sign out first to remove the tokens and the unlocked key.

## Limitations

- Windows only. One GitHub account.
- Syncs sessions, subagent logs, tool results and project memory; not Claude's `file-history`.
- Claude deletes local sessions older than `cleanupPeriodDays` (30 days by default). Saved copies stay in the cloud and show as "only left in the cloud"; restore them per session.
- In VS Code, `SessionEnd` does not fire when the window closes; auto-save relies on `Stop` plus the pending markers.
- Tested with Claude Code 2.1.278–2.1.280.

## Documentation

- [docs/setup-github-app.md](docs/setup-github-app.md) — registering the GitHub App
- [docs/system-architecture.md](docs/system-architecture.md) — how it works
- [docs/code-standards.md](docs/code-standards.md) — conventions and checks
- [docs/development-roadmap.md](docs/development-roadmap.md) — phases and deferred work
- [docs/project-changelog.md](docs/project-changelog.md) — changes

## License

[MIT](LICENSE) © 2026 dris1153
