---
phase: 3
title: "Search, outline, export, diverged compare"
status: completed
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
- [x] search engine + tests
- [x] export + golden test
- [x] commands
- [x] outline, search UI, copy
- [x] compare + Resolve "View"
- [x] docs

## Success Criteria
- [x] Search finds a word that only appears in a truncated tool output and jumps to it
- [x] Exported .md opens cleanly and contains every prompt and Claude reply of the branch
- [x] On a diverged session the marker sits at the first differing message on both sides
- [x] Search on the 24 MB session < 300 ms (release)

## Risk Assessment
- Highlighting inside markdown-rendered nodes is fiddly → highlight only in visible items, fall back to outlining the matching item.
- Export path pointing into `~/.claude` or the app data dir → rejected by the command.

## Security Considerations
- Export writes only to the dialog-chosen absolute path; never overwrites without the dialog's own confirmation.
- Search queries are plain substrings (no regex from the webview).

## Implementation Notes (2026-09-24)
- Engine: `transcript/search.rs` (case-insensitive, ASCII fast path, 1 000-hit cap; 20 ms on a 173 MB session), `compare.rs` (`first_difference` by record uuid), `export.rs` (Markdown; outputs capped at 50 KB with a fence longer than any backtick run; subagents three levels deep, at most 50 per export, missing ones noted), `preview.rs` (split from mod.rs).
- Commands: `commands/session_tools.rs` (`search_session`, `compare_sides`, `export_session`); `session_view.rs` gained a `Target` helper (`open`, `shown` = the parse the viewer shows, `open_agent`).
- Outline is a drop-down of prompts in the header, not a left column (the window already has the project list on the left).
- Export opens the native save dialog from Rust: the window never passes a path; the file is written beside and renamed over.
- Search highlights matches with the CSS Custom Highlight API (no DOM changes); a jump opens the collapsed tool block of the match.

## Review Log (2026-09-24)
Report: [code-reviewer-260924-phase-03-tools.md](./reports/code-reviewer-260924-phase-03-tools.md) · 6.5/10, 2 high, 5 medium, 13 low.

Fixed: H1 search matched event type names and thousands of hidden hook notices (now only event text, and system events only while shown; no automatic switch); H2 the diverge marker sat on hidden items (moved to the next shown item, button hidden when none); M1 export path from the webview could reach protected folders through UNC loopback, hard links or streams (native dialog in Rust, local-disk prefix only, write + rename); M2 a nested array broke item keys (flatMap with keys); M3 empty block wrappers added space; M4 tool summaries (Windows paths as typed) are searched; M5 details and search use the parse the viewer shows; L highlight guarded against length-changing lowercase, block lookup limited to the item's own blocks, export depth really 3, export subagent cap, stale search results dropped, highlight cleared, monochrome highlight and marker, outline width capped, compare only for diverged sessions (not for a session that is merely ahead: it would decrypt the cloud copy on every open).
Not changed: saved `tool-results` files are not part of the export; subagents and saved outputs are not searched.
