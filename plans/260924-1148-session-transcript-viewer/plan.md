---
title: "Session transcript viewer (v0.2.0)"
description: "Click a session to read the whole conversation: messages, tool calls with results, diffs, nested subagents, metadata; local or cloud copy; search, outline, export, diverged compare."
status: in-progress
priority: P2
effort: 3.5d
branch: main
tags: [feature, frontend, rust, transcript]
blockedBy: [260923-1305-claude-session-sync-desktop-app]
blocks: []
created: 2026-09-24
---

# Session transcript viewer

## Overview
Clicking a session row opens a viewer in place of the project detail pane. Rust parses the Claude JSONL (local file, or the cloud copy decrypted in memory) into a compact view model; the webview only renders it. Starts after the v0.1.0 E2E passes (blockedBy).

Design: [brainstorm report](../reports/brainstorm-260924-1148-session-transcript-viewer.md) (facts measured on a real 24 MB transcript, decisions, rejected options).

## Phases

| Phase | Name | Effort | Status |
|-------|------|--------|--------|
| 1 | [Parser, IPC and basic viewer](./phase-01-parser-ipc-and-basic-viewer.md) | 1.5d | Complete |
| 2 | [Diffs, persisted outputs, subagents, images](./phase-02-diffs-outputs-subagents-images.md) | 1d | Complete |
| 3 | [Search, outline, export, diverged compare](./phase-03-search-outline-export-compare.md) | 1d | Pending |

Sequential 1→2→3.

## IPC contract (additions)
| Command | Args → Return |
|---|---|
| `open_session(key_hash, session_id, side: local\|cloud)` | → `SessionView{key_hash, session_id, side, meta: SessionMeta, items: Item[]}` (`other_side` comes with phase 3) |
| `session_detail(key_hash, session_id, side, ref)` | → `Detail` (phase 1: full tool input/output; phase 2: persisted output, subagent `SessionView`, image data URI) |
| `search_session(key_hash, session_id, side, query)` | → `Hit[]{item, block?}` (phase 3) |
| `export_session(key_hash, session_id, side, path)` | → () Markdown file at a path chosen in a save dialog (phase 3) |

`SessionMeta{title?, models[], version?, git_branch?, cwd?, started_at?, ended_at?, usage{input, output, cache_read, cache_creation}, prompts, tool_calls, off_branch}`.
`Item = user{uuid, at, text, images} | assistant{uuid, at, model?, usage?, blocks[]} | event{uuid?, at?, event, noisy, text}`; `Block = text{text} | thinking{text?} | tool{id, name, summary, input, input_truncated, output?, output_truncated, is_error}` (phase 2 adds `diff`, `subagent`, image refs).
New error code: `session_not_found`.

## Key dependencies
- Engine groundwork from plan 260923 (manifests, `remote::file_content`, `PathRewrite`, links, `SyncLock`).
- New npm deps: `react-markdown`, `remark-gfm` (no `rehype-raw`, ever).
- Transcript format is undocumented (Claude Code 2.1.278–2.1.280 observed); parser must tolerate unknown shapes.

## Out of scope
Editing or truncating transcripts · showing abandoned rewind branches · syntax highlighting · separate viewer windows · viewing sessions of projects with no cloud copy and no local dir.

## Conventions
Files < 200 lines; Rust snake_case, TS kebab-case; en/vi locale parity; fixtures hand-written from real structure with synthetic content; tests never read or write `~/.claude` (the manual `parse_real` timing test reads a path given by env only). Update `docs/system-architecture.md` (IPC), `docs/project-changelog.md`, `docs/development-roadmap.md` (v0.2.0) when phases complete.
