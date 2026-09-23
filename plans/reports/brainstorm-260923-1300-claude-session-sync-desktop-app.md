# Brainstorm: Claude Code Session Sync Desktop App

Date: 2026-09-23 · Status: approved · Next: /ck:plan

> **Superseded in part** by plan `260923-1305-claude-session-sync-desktop-app/plan.md` (Red Team Review + Validation Log): app name `session-relay`; GitHub App instead of OAuth App; `Stop`+`SessionEnd` debounced hooks with pending markers (no single-instance forwarding); local links map for project identity; append vs whole file classes; no global index; data in `%LOCALAPPDATA%`; vi+en UI; autostart.

## Problem
User works on same GitHub projects across multiple Windows machines. Claude Code session data lives locally (`~/.claude/projects/<encoded-cwd>/`) → must manually copy per project when switching machines. Want desktop app: "Lưu" (save) on machine A, "Sao chép" (restore) on machine B, project identified by GitHub remote URL, GitHub as storage.

## Facts discovered (scout + spikes)
- Layout per project dir: `*.jsonl` (transcripts), `<sessionId>/subagents/*.jsonl|*.meta.json`, `<sessionId>/tool-results/*`, `memory/*.md`.
- Dir name = absolute cwd with non-alphanumerics → `-` (`d:\Workspace\X` → `d--Workspace-X`). Different clone path on machine B → different dir name.
- `cwd` field inside jsonl recorded in mixed forms (`D:\…`, `d:\…`, `/d/…`).
- Local volume: 1.4 GB, 881 files; **5 jsonl > 100 MB** (max 182 MB) → exceeds GitHub hard file limit.
- Transcripts contain secrets (.env contents, tokens, command output).
- Issue anthropics/claude-code#18645 reported copied sessions not recognized in 2.1.9+.
- **Spike on Claude Code 2.1.261**: copied session into new encoded dir → `claude -p -c` loaded it correctly **both with and without** rewriting `cwd`. → Store raw bytes, no rewrite needed (keeps chunks byte-stable across machines). Interactive `--resume` picker still to verify.

## Decisions
| Topic | Decision | Rationale |
|---|---|---|
| Storage | Dedicated private repo `<user>/claude-sessions` | Project repos (incl. client orgs) would leak transcripts/secrets to collaborators; no push rights needed on client repos |
| Audience | Open source, single user | Device flow (no client secret), no code signing/updater |
| Encryption | age, passphrase-protected X25519 identity | Confidentiality of client transcripts |
| Trigger | Manual buttons + Claude Code `SessionEnd` hook auto-save | Manual-only = forgetting to save = original pain |
| Layout | 2-column dashboard + tray | User choice |
| Action model | Single status-driven button (Lưu / Sao chép / Xử lý…) + ⋯ force menu | Prevents wrong-direction overwrite |
| Linking on new machine | Scan workspace roots by remote URL + "Clone về" + manual pick | Fast new-machine setup |

## Evaluated alternatives (rejected)
- Orphan branch / commit in project repo: secret leak, client-repo write access, history noise.
- Syncthing/OneDrive of `~/.claude/projects`: breaks when clone paths differ; concurrent-write conflicts.
- GitHub Releases as blob store: no atomic CAS, custom GC; git + force-with-lease simpler.
- Electron: ~150 MB, heavy RAM for tray utility. Wails: smaller ecosystem.
- Web OAuth + PKCE: GitHub still requires client secret → unusable in open source client.
- Per-chunk passphrase encryption: scrypt ~1 s/file → too slow.
- Verifying project access via GitHub API: org third-party OAuth restrictions → false 404s. Identity from local `git remote` suffices.

## Final solution

### Stack
- Tauri v2 (core 2.11.x), Rust core; React + TS + Vite + Tailwind v4 (tokens from DESIGN.md).
- Rust crates: `age`, `zstd`, `keyring`, `reqwest`, `sha2`, `serde_json`. Git via system `git` (`std::process::Command`).
- Plugins: `single-instance`, `dialog`, `notification`, `opener`; built-in `tray-icon` feature.
- Fonts: Source Serif 4 + Inter bundled locally (Anthropic fonts proprietary). No Claude logo; README states "unofficial".

### Architecture
```
Claude Code ──SessionEnd hook──► app.exe hook-save (stdin JSON: cwd, session_id)
                                    │ GUI running → forward via single-instance
                                    ▼
~/.claude/projects/<enc>/ ◄──► Sync engine (Rust) ◄──► local clone ◄──git──► <user>/claude-sessions (private)
                                    ▲
                          React UI (dashboard + tray)
```

### Project identity
- key = normalized origin (`github.com/owner/repo`, lowercase, no `.git`) + subpath from `git rev-parse --show-toplevel`.
- Local discovery: scan `~/.claude/projects/*/` → read `cwd` from jsonl → `git -C <cwd> remote get-url origin`.
- Storage folder = `hash16(key)` to hide client project names; real key in encrypted index.

### Storage format
```
claude-sessions/
  keys/identity.age          # age X25519 identity, scrypt-passphrase encrypted
  index.age                  # key → folder, last machine, timestamps
  p/<hash16>/
    manifest.age             # per file: rel path, size, chunk list (plaintext hash → chunk file)
    c/<keyed-hash>.zst.age   # chunks
```
- Chunking: jsonl split on line boundaries, ~4 MB raw target. Append-only → sealed chunks never change → only tail chunk re-uploaded. Solves 100 MB limit. Other files = 1 chunk each.
- zstd then age (≈1.4 GB → ~200 MB est.).
- Each save = orphan commit of current tree + `git push --force-with-lease=<fetched sha>` (CAS). Rejected → fetch, re-merge, retry. Remote holds 1 snapshot → bounded size, fast clone. Tradeoff: no remote rollback; local backups cover it.
- Token passed per command via `-c http.extraHeader`, never written to `.git/config`.

### Sync states (per file, aggregated per project)
| State | Condition | Primary action |
|---|---|---|
| ✓ Khớp | identical | ⋯ only |
| ● Chưa lưu | remote is prefix of local | Lưu |
| ○ Cloud mới hơn | local is prefix of remote | Sao chép |
| ◐ Lệch 2 máy | both appended differently | Xử lý… |
| ◌ Chưa có ở máy này | remote only | Clone về / Chọn thư mục |

Safety: read up to last `\n` (Claude may be writing); restore via temp file + rename; never delete local files; overwritten versions backed up to `%APPDATA%/<app>/backups/`; abort restore if target mtime changes mid-op.

Synced: `*.jsonl`, `<sessionId>/**`, `memory/**`. Not synced v1: `~/.claude/file-history` (rewind checkpoints), global `history.jsonl`, settings, credentials.

### Auth & secrets
- GitHub OAuth App + Device Flow, scope `repo` (private repo). Token in Windows Credential Manager (`keyring`). Upgrade path: GitHub App scoped to storage repo only.
- First machine generates age identity, encrypts with passphrase, pushes `keys/identity.age`. New machine: passphrase once → identity cached in Credential Manager → headless hook works without prompt.
- Passphrase loss = total data loss → explicit warning at setup.

### Hook
- "Bật tự lưu" merges `SessionEnd` entry into `~/.claude/settings.json` (backup first, preserve other hooks).
- CLI mode: forward to running GUI, else spawn detached worker and exit immediately. Saves only that project. Toast only on failure. App re-validates hook path on start.

### UX
1. Onboarding (serif headings): GitHub device code (large) + "Mở GitHub" → storage repo (auto-create private `claude-sessions` or pick) → passphrase create/unlock → workspace roots + auto-save toggle.
2. Sidebar: projects grouped by owner, status glyph, "Cần chú ý" filter; bottom: settings, avatar.
3. Main pane: name (serif 30px), remote, status sentence ("Cloud mới hơn · lưu từ PC-HOME lúc 09:12"), dark primary button, ⋯ (Lưu đè / Sao chép đè / Mở thư mục / Copy `claude --resume`), sessions list (first prompt as title, date, size, machine), activity log.
4. Conflict dialog: per session side by side (message count, time, machine) → keep this / keep cloud; other auto-backed-up.
5. Tray: Clay dot when attention needed; menu Mở · Lưu tất cả · Tự lưu Bật/Tắt · Thoát; close window → hide to tray.
6. Monochrome status glyphs (●○◐✓◌), no green/red, per design system.

## Risks
1. Claude Code storage/validation changes (issue #18645). Mitigation: P0 verify interactive picker; cwd-rewrite fallback; supported-version check.
2. Restoring while Claude has project open → mtime guard + UI hint.
3. First upload ~200 MB → progress UI.
4. `repo` scope broad; passphrase loss unrecoverable.
5. Windows long paths (encoded dir names) → enable long-path awareness.

## Phases
- P0 Spike: interactive `--resume` picker; git + age + zstd roundtrip.
- P1 Rust core: discovery, identity, chunk/manifest, crypto, git transport, state compare.
- P2 Auth + onboarding.
- P3 Dashboard + tray.
- P4 Hook CLI + single-instance.
- P5 Scan/clone, conflict dialog, polish.

Out of scope v1: macOS/Linux, file-history, auto-pull, multi-account, GitHub App, remote history rollback.

## Success criteria
- Switch machine ≤ 3 clicks, < 30 s for typical project (tail-chunk update).
- Restored sessions appear in `claude --resume` on machine B.
- No pushed file > 100 MB; remote size ≈ compressed current data.
- Hook save never blocks Claude exit.

## Open questions
- App name (avoid implying official Anthropic product).
- Chunk size tuning (4 MB default) after measuring real save frequency.
