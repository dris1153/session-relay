---
phase: 1
title: "Feasibility Spikes & Scaffold"
status: completed
priority: P1
effort: "1.5d"
dependencies: []
---

# Phase 1: Feasibility Spikes & Scaffold

## Context Links
- [Brainstorm report](../reports/brainstorm-260923-1300-claude-session-sync-desktop-app.md) — done: `claude -p -c` resumes copied session with/without `cwd` rewrite (2.1.261)
- [Tauri research](./research/researcher-01-tauri-v2-windows-report.md)
- DESIGN.md (project root)
- Local facts (2026-09-23): 32/33 transcripts `entrypoint: claude-vscode`; `ai-title{aiTitle}`, `last-prompt{lastPrompt}` lines exist, no `summary`; registry `<claude_home>/sessions/<pid>.json` keys `pid, sessionId, cwd, procStart, status, entrypoint, version`; 14/33 transcripts embed absolute `…\projects\<enc>\<sid>\tool-results\…` paths; user profiles differ across machines (`PC`, `Dris`); `.meta.json` files have no trailing `\n`

## Overview
Kill unknowns that would force redesign, then scaffold Tauri v2 + React + TS + Vite + Tailwind v4 with design tokens and explicit CSP.

## Requirements
- Each spike → PASS/FAIL + chosen approach in `plans/260923-1305-claude-session-sync-desktop-app/reports/spike-results.md`.
- Scaffold runs (`npm run tauri dev`), tokens usable as Tailwind utilities, default Tauri Windows manifest kept (no custom manifest in v1).

## Implementation Steps
1. **Spike A — resume across machines** (scratch copies only):
   - Copy a session that has tool-results into a new encoded dir under a *different* scratch root; verify (a) CLI `claude --resume` picker lists/opens it, (b) VS Code extension "past conversations" lists/opens it, (c) ask model to Read one referenced tool-result → record whether absolute path of the other machine breaks it.
   - If (c) breaks → adopt placeholder normalization (phase 2 §Path normalization). If (a)/(b) fail → retry with `cwd` rewrite; if still fail → STOP, escalate.
2. **Spike B — hooks on Windows**:
   - Register temporary `Stop` + `SessionEnd` hooks in a scratch project's `.claude/settings.local.json` with command `C:/…/hook-probe.exe hook-save` (forward slashes, no spaces, no quotes). Verify it parses under Claude's hook shell (Git Bash and PowerShell), fires in **VS Code extension** and CLI, GUI-subsystem exe reads piped stdin.
   - Probe spawns detached child (`CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB`, fallback without breakaway) that sleeps 150 s then writes a file; verify survival after (i) turn end, (ii) CLI exit, (iii) closing VS Code window.
   - Remove temporary hooks afterwards.
3. **Spike C — secrets & GitHub App**:
   - keyring 4.2 roundtrip; set **Local** persistence via windows-native-keyring-store modifiers; confirm with `CredEnumerate` Persist=LOCAL_MACHINE(2). Fallback: keyring 3.x `windows-native` + direct `windows-sys` CredWrite.
   - Register a test GitHub App (device flow on, Contents RW, Metadata R, expiring tokens). Verify: device flow login; `GET /user/installations` + `/user/installations/{id}/repositories`; token refresh with `grant_type=refresh_token` **without client_secret**; `https://github.com/new?name=claude-sessions&visibility=private` pre-fills form.
4. **Spike D — isolated git transport** (local bare repo + one real push to test repo):
   - Env: remove all `GIT_*`, `SSH_*`; set `GIT_CONFIG_NOSYSTEM=1`, `GIT_CONFIG_GLOBAL=NUL` (try `/dev/null` if NUL fails), `GIT_TERMINAL_PROMPT=0`, `GCM_INTERACTIVE=never`; config via `GIT_CONFIG_COUNT`: `http.sslBackend=schannel`, `core.hooksPath=<empty dir>`, `core.fsmonitor=false`, `core.autocrlf=false`, `protocol.allow=never`, `protocol.https.allow=always`, `protocol.file.allow=always` (tests only), `credential.helper=` then `credential.helper=!C:/…/probe.exe git-credential`.
   - Verify: push to github.com works with a user `url.insteadOf` set globally (ignored); orphan snapshot (`add -A`, `write-tree`, `commit-tree` no parent); first push `--force-with-lease=refs/heads/main:` succeeds only if ref absent; stale lease rejected; second snapshot with 1 of 100 files changed transfers only new objects.
5. **Spike E — throughput**: 180 MB jsonl → 4 MiB line chunks → zstd 3 → age x25519; record time/size; confirm chunk size.
6. **Spike F — Claude path rules**: create sessions in scratch dirs with Vietnamese chars (`D:\tmp\Dự án`) and > 200-char path; record exact encoded dir names (UTF-16 unit rule, truncation+hash). Start a session in a repo **subfolder**: record where `memory/` lands (cwd dir vs git-root dir).
7. Scaffold: `npm create tauri-app@latest` (React, TS, Vite), `productName`/binary `session-relay` (no spaces → hook path needs no quotes); add `@tailwindcss/vite`, `@fontsource-variable/inter`, `@fontsource-variable/source-serif-4`; `npm ci --ignore-scripts` policy with lockfile.
8. `src/styles/design-tokens.css`: `@import "tailwindcss"`; `@theme` from DESIGN.md (colors, radii 8/16/24, shadows, type scale); fonts Source Serif 4 / Inter; base layer cursor rule (pointer on button/[role=button]/select/checkbox labels, not-allowed when disabled).
9. `tauri.conf.json`: window 1040×680 (min 880×560); explicit CSP `default-src 'self'; connect-src ipc: http://ipc.localhost; img-src 'self' data: https://avatars.githubusercontent.com; font-src 'self'; style-src 'self' 'unsafe-inline'`.
10. `git init`, `.gitignore` (node_modules, dist, src-tauri/target, *.local).

## Related Code Files
- Create: `package.json`, `package-lock.json`, `vite.config.ts`, `tsconfig.json`, `index.html`, `.gitignore`, `src/main.tsx`, `src/app.tsx`, `src/styles/design-tokens.css`
- Create: `src-tauri/{Cargo.toml,build.rs,tauri.conf.json}`, `src-tauri/capabilities/default.json`, `src-tauri/src/{main.rs,lib.rs}`
- Create: `plans/260923-1305-claude-session-sync-desktop-app/reports/spike-results.md`

## Todo List
- [x] Spike A resume (CLI picker, VS Code list, tool-results path)
- [x] Spike B hooks (Stop/SessionEnd, VS Code, quoting, stdin, survival)
- [x] Spike C keyring Local persistence + GitHub App device flow/refresh (new-repo URL prefill: pending user answer)
- [x] Spike D isolated git + credential helper + lease + dedupe
- [x] Spike E throughput
- [x] Spike F encode rules + memory location
- [x] spike-results.md
- [x] Scaffold + tokens + CSP + git init (+ `.env.example`, build.rs env loader)

## Success Criteria
- [x] Every spike recorded; each FAIL has a chosen fallback before phase 2 (open: `github.com/new` prefill, cosmetic)
- [x] Debug build shows serif heading on parchment canvas (screenshot); `npm run tauri build` produces NSIS installer (1.8 MB)

## Risk Assessment
- Spike A (a)/(b) fail even with rewrite → project blocked → escalate.
- Breakaway denied and worker dies on VS Code close → rely on pending markers + GUI retry (phase 5), document.
- keyring cannot set Local persistence → direct CredWrite via `windows-sys` (~40 lines).

## Security Considerations
- Spike data = real transcripts → scratchpad only, delete after; test GitHub App/repo deleted after spikes or reused as dev app.
- Temporary hooks removed after Spike B.
