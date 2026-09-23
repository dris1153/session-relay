# Session Relay Brainstorm & Plan Complete

**Date**: 2026-09-23 14:00
**Severity**: N/A (Planning phase)
**Component**: session-relay (greenfield desktop app)
**Status**: Plan approved, ready for Phase 1 spikes

## What Happened

Completed brainstorm + red team review + plan finalization for `session-relay`: a Tauri v2 desktop app that syncs Claude Code session data between Windows machines via a private GitHub repo. Problem: session dirs (~/.claude/projects/<encoded-cwd>/) must be manually copied per project when switching machines.

## The Decision Stack

**App**: Tauri v2 + React/TS + Rust. **Storage**: user's private `claude-sessions` repo (not project repos — avoids leaking transcripts to collaborators). **Auth**: GitHub App + device flow (repo-scoped, expiring 8h tokens; avoids `repo` scope overpermission). **Chunking**: line-boundary ~4 MiB targets to solve 5 jsonl files exceeding GitHub's 100 MB hard limit. **Encryption**: age X25519 + zstd (~1.4 GB → ~200 MB). **Sync**: orphan snapshot commit + force-with-lease CAS (bounded remote, atomic updates). **Auto-save**: Stop hook (debounced 120s) + SessionEnd hook with pending markers (avoids blocking Claude exit). **Project ID**: normalized remote URL (lowercase `github.com/owner/repo`) + subpath, stored as hash16 to hide client names. **UI**: 2-column dashboard + tray, Vietnamese + English, monochrome status glyphs.

## What Went Wrong (First Draft)

Plan initially assumed CLI-only usage + append-only storage. Red team + spike findings forced rewrites: (1) OAuth `repo` scope too broad + unrevocable → switched to GitHub App; (2) 32/33 local sessions run in the VS Code extension where SessionEnd rarely fires → added debounced Stop hook; (3) .meta.json files have no trailing newline → naive line truncation stores 0 bytes → must handle edge case; (4) Absolute tool-results paths differ per user profile → Spike A decides placeholder normalization.

## Spike Result (Critical)

Copied session to new encoded dir on same machine → `claude -p -c` loaded it **both with and without** cwd rewrite. Contradicts issue #18645 (reported unsupported in 2.1.9+, test ran on 2.1.261). **Implication**: store raw bytes, no cwd rewrite needed, keeps chunks byte-stable across machines. Interactive picker + VS Code list integration still unverified — P0 spike required.

## Red Team Findings (39 → 15 deduped)

4 Critical: (1) git status refresh resets shared clone → need locking; (2) non-jsonl truncation → 0 bytes on .meta.json; (3) identity from first cwd breaks on restore → duplicate dirs per project key; (4) OAuth scope → GitHub App. 8 High: mtime guard, rare SessionEnd, 30-day cleanup risk, absolute paths, global git config leak, manifest traversal, broad permissions. Accepted all; user decisions applied (GitHub App, Stop+SessionEnd debounced).

## Lessons

- Greenfield plans miss operational reality: test first (spike before commit). SessionEnd alone inadequate — humans copy projects outside hooks.
- File format fragility: no assumptions about newlines or absolute paths. Edge case = crash.
- Auth model compounds: `repo` scope is permanent, GitHub Apps expiring tokens allow graceful revocation. Cost: user setup complexity.
- Git + credentials = footgun: token never in .git/config, argv or env; isolated git config + own exe as credential helper.

## Next Steps

Phase 1 spikes (no core code yet): (A) interactive `--resume` picker on new machine; (B) git + age + zstd roundtrip with real project data; (C) hook quoting/shell escaping; (D) SessionEnd integration; (E) long-path handling; (F) credential manager persistence. All must resolve before Phase 2 Rust core. Phase 1 effort 1.5d; total 17d to ship.

---

**Status**: DONE
**Summary**: Brainstorm + red team validated Tauri + GitHub App + debounced hooks approach; spike proved sessions copy & resume correctly; plan blocks on Phase 1 spike validation before implementation.
