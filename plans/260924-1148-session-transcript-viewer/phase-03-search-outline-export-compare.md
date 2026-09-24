---
phase: 3
title: "Search, outline, export, diverged compare"
status: pending
priority: P2
effort: "1d"
dependencies: [2]
---

# Phase 3: Search, outline, export, diverged compare

## Context Links
- [plan.md](./plan.md) · [phase 1](./phase-01-parser-ipc-and-basic-viewer.md) · [phase 2](./phase-02-diffs-outputs-subagents-images.md)
- Code: `src/features/dashboard/conflict-dialog.tsx`, `@tauri-apps/plugin-dialog` (save dialog)

## Overview
Tools for long sessions: a prompt outline, full-text search, copy per message, Markdown export, and a local/cloud switch that marks where two diverged copies part.

## Requirements
- Functional:
  - Outline: every user prompt (first line, ≤ 80 chars, time) in a left column; click scrolls to it; collapses to a dropdown under ~900 px width.
  - Search: case-insensitive over prompts, Claude text, tool inputs and full outputs (not previews), shown noise included; Enter/Shift+Enter for next/previous; the matching tool block opens; count "3 / 17".
  - Copy: button per message (Markdown of that message).
  - Export: save dialog → Markdown file: header with meta, `## You` / `## Claude` per turn, tool calls as fenced blocks with outputs capped at 50 KB each (noted when cut), subagents as nested sections.
  - Compare: when the session has both a local and a cloud copy that differ, header switch "This machine / Cloud"; the first item whose uuid differs between the two gets a marker "The copies differ from here"; the Resolve dialog gets a "View" link per file that opens this viewer.
- Non-functional: search on the 24 MB session < 300 ms; export streams to disk (no full string in the webview).

## Architecture
- `engine/transcript/search.rs` — iterate items/blocks over full bodies, return `Hit{item, block?}` (cap 1 000 hits).
- `engine/transcript/export.rs` — writes Markdown with `std::io::Write` to the chosen path (path comes from the dialog; command validates it is absolute and not under `~/.claude`).
- Compare: `SessionView.other_side` (phase 1) tells the UI a second copy exists; the UI opens the other side (cache holds both) and computes the first differing uuid index itself.
- UI: `outline.tsx`, `search-bar.tsx` (highlight via `<mark>` in rendered text nodes of visible items only), copy button in `message-item.tsx`, export button in the header, side switch in the header; `conflict-dialog.tsx` "View" → `onView(rel)` → dashboard opens the viewer.

## Related Code Files
- Create: `src-tauri/src/engine/transcript/{search,export}.rs`, `src/features/session-viewer/{outline,search-bar}.tsx`
- Modify: `commands/session_view.rs`, `engine/transcript/mod.rs`, `src/features/session-viewer/{session-viewer,viewer-header,message-item}.tsx`, `src/features/dashboard/{conflict-dialog,dashboard-page}.tsx`, `src/lib/tauri-commands.ts`, locales

## Implementation Steps
1. `search.rs` + tests (hits in text, tool output beyond preview, noisy event).
2. `export.rs` + test (golden Markdown from a fixture).
3. Commands `search_session`, `export_session` (+ save dialog on the UI side).
4. Outline, search bar with navigation, copy buttons.
5. Side switch + divergence marker; "View" in the Resolve dialog.
6. Locales; measure search time on the real session; clippy, tests, tsc, build; code review.
7. Docs: architecture (IPC), changelog, roadmap v0.2.0.

## Todo List
- [ ] search engine + tests
- [ ] export + golden test
- [ ] commands
- [ ] outline, search UI, copy
- [ ] compare + Resolve "View"
- [ ] docs

## Success Criteria
- [ ] Search finds a word that only appears in a truncated tool output and jumps to it
- [ ] Exported .md opens cleanly and contains every prompt and Claude reply of the branch
- [ ] On a diverged session the marker sits at the first differing message on both sides
- [ ] Search on the 24 MB session < 300 ms (release)

## Risk Assessment
- Highlighting inside markdown-rendered nodes is fiddly → highlight only in visible items, fall back to outlining the matching item.
- Export path pointing into `~/.claude` or the app data dir → rejected by the command.

## Security Considerations
- Export writes only to the dialog-chosen absolute path; never overwrites without the dialog's own confirmation.
- Search queries are plain substrings (no regex from the webview).
