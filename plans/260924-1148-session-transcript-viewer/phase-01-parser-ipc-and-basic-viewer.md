---
phase: 1
title: "Parser, IPC and basic viewer"
status: pending
priority: P2
effort: "1.5d"
dependencies: []
---

# Phase 1: Parser, IPC and basic viewer

## Context Links
- [plan.md](./plan.md) · [brainstorm report](../reports/brainstorm-260924-1148-session-transcript-viewer.md)
- Code: `src-tauri/src/engine/remote.rs` (`manifest`, `file_content`), `engine/normalize.rs` (`PathRewrite`), `engine/links.rs` (`dir_for`), `engine/title.rs`, `src/lib/sessions.ts`, `src/features/dashboard/{session-list,project-detail-pane,dashboard-page}.tsx`

## Overview
Rust parser for the current branch of a session, `open_session` / `session_detail`, and a viewer showing user and Claude messages (markdown), tool calls paired with their results, per-turn metadata and a noise toggle. Works for the local file and for the cloud copy.

## Key Insights (measured on a 24 MB, 8 902-line transcript)
- One API message is split over several `assistant` records (1 735 records, 786 `message.id`s, up to 7 per id); `usage` is identical across records of an id → group by `message.id`, count usage once.
- Tool results arrive as `user` records with `tool_result` blocks (918) → attach to the `tool_use` with the same id, never shown as user messages.
- 31 `user` records have `isMeta: true` (skill text, command output) → noisy events.
- Current branch: from the last non-sidechain user/assistant/system record, follow `parentUuid`, else `logicalParentUuid` (compact boundary has `parentUuid: null`). Keeps 2 662 of 2 705 messages.
- ~70 % of records are noise: attachments (`hook_non_blocking_error`, `total_tokens_reminder`, `environment`, …), `atis-latch`, `last-prompt`, `mode`, `queue-operation`, `file-history-*`.
- Thinking: 66 of 651 blocks have text; the rest store a signature only.
- Cloud content is normalized: `remote::file_content` returns it with the project-dir placeholder; `PathRewrite::new(dir).expand` restores paths.

## Requirements
- Functional: open any session row that has a local file or a cloud copy; show prompts, Claude text (markdown), thinking (or "not stored"), tool blocks (name, input preview, output preview, error state, "show all"), per-message time · model · tokens, compact and API-error markers; noise hidden by default with a toggle; back button returns to the project pane.
- Non-functional: 24 MB session opens < 1 s local, < 2 s cloud (release build); viewing never writes under `~/.claude`; unknown or malformed lines never fail the parse.

## Architecture
```
session row click ─▶ DashboardPage.viewing = {key_hash, session_id, side}
SessionViewer ─▶ open_session ─▶ commands/session_view.rs
   ├─ source: local  = projects_dir/<dir_for(key)>/<sid>.jsonl (read whole file)
   │          cloud  = SyncLock(5 s) → last_snapshot → remote::manifest → files["<sid>.jsonl"]
   │                   → remote::file_content → PathRewrite(local dir | "<project>").expand
   ├─ engine::transcript::parse(bytes) → Transcript (full bodies kept in Rust)
   ├─ ViewCache in AppState: Mutex<Vec<(CacheKey, Arc<Transcript>)>>, max 2 entries (both sides for compare)
   │   CacheKey = (key_hash, sid, side, len + mtime | manifest entry hash)
   └─ Transcript::view() → SessionView (previews: tool input ≤ 2 KB, output ≤ 8 KB)
"show all" ─▶ session_detail(ref "in:<tool_id>" | "out:<tool_id>") from the cached Transcript
```
Engine module `engine/transcript/`:
- `mod.rs` — public types (`Transcript`, `Item`, `Block`, `SessionMeta`, `Usage`), `parse`, `view`.
- `records.rs` — tolerant extraction from `serde_json::Value` (no strict structs: fields missing → defaults).
- `branch.rs` — current-branch walk; attachments kept only when their parent is on the branch; `off_branch` count.
- `items.rs` — grouping by `message.id`, tool_use ↔ tool_result pairing, `isMeta`, event kinds + noise rule.
- `noise.rs` (if items.rs nears 200 lines) — kept visible: `compact_boundary`, `api_error`, attachment `file` (@-mention), `queued_command`; everything else `noisy: true`; unknown record types → `event{kind: "other", noisy: true, text: type name}`.

UI `src/features/session-viewer/`:
- `session-viewer.tsx` — layout, back button, noise toggle, loading/error.
- `viewer-header.tsx` — title, models, version, branch, span, token totals.
- `message-item.tsx` — user / assistant / event rendering, metadata line.
- `tool-block.tsx` — native `<details>`; summary `Name · first line of input`; input + output `<pre>`; "show all" → `session_detail`.
- `markdown-text.tsx` — `react-markdown` + `remark-gfm`, `skipHtml`; `a` → button calling `openUrl` for http/https/mailto only; `img` → link text; code blocks mono on Soft Stone.
- `src/lib/use-session-view.ts` — load + detail fetch; `src/lib/tauri-commands.ts` types.
- Items get `content-visibility: auto; contain-intrinsic-size: auto 160px`.

## Related Code Files
- Create: `src-tauri/src/engine/transcript/{mod,records,branch,items}.rs`, `src-tauri/src/commands/session_view.rs`, `src-tauri/src/session_view_cache.rs`, `src-tauri/tests/transcript.rs`, `src-tauri/tests/fixtures/transcripts/*.jsonl`, `src/features/session-viewer/*.tsx`, `src/lib/use-session-view.ts`
- Modify: `engine/mod.rs`, `engine/error.rs` (codes), `commands/mod.rs`, `lib.rs` (register), `app_state.rs` (cache field), `src/lib/tauri-commands.ts`, `src/features/dashboard/{session-list,project-detail-pane,dashboard-page}.tsx`, `src/locales/{en,vi}.json`, `package.json`
- Delete: none

## Implementation Steps
1. Fixtures (synthetic text, real structure): basic turn; assistant split over 3 records with one `message.id`; tool_use/tool_result incl. `is_error`; rewind (abandoned branch); compact boundary with `logicalParentUuid`; `isMeta` user; noise attachments; unknown record type; malformed and truncated last line; sidechain records.
2. `records.rs` + `branch.rs` + `items.rs` + `parse`; tests on every fixture (branch membership, grouping, pairing, noise flags, usage counted once, no panic on garbage).
3. `view()` with preview limits and detail refs; `session_detail` lookups.
4. Sources: local path via `links.dir_for`; cloud via manifest + `file_content` under a 5 s `SyncLock` (Busy → `busy`), expand placeholder. `session_not_found` when neither exists.
5. Cache + commands + registration; ignored test `parse_real` timing a file from `SR_TRANSCRIPT` (read-only).
6. `npm i react-markdown remark-gfm`; viewer components; session row becomes a button (keeps Restore/Delete buttons working, `stopPropagation`); `DashboardPage` holds `viewing` (extract a small hook if the page nears 200 lines).
7. Locales (en/vi) for viewer copy and new error codes.
8. Measure on release with the real 24 MB session: parse ms, payload size, time to first paint, scroll smoothness. If > target, switch the item list to batch rendering (IntersectionObserver) before considering Rust paging.
9. clippy, cargo test, tsc, build; code review.

## Todo List
- [ ] fixtures
- [ ] parser (records, branch, items) + tests
- [ ] view model + details
- [ ] local and cloud sources
- [ ] cache + commands
- [ ] viewer UI + markdown + tool blocks
- [ ] locales
- [ ] measurements recorded here

## Success Criteria
- [ ] All fixture tests pass; malformed input never errors
- [ ] 24 MB session: < 1 s local, < 2 s cloud, smooth scroll (release)
- [ ] Every prompt and Claude text on the current branch appears; each tool call shows its result
- [ ] No file under `~/.claude` changes when viewing (compare mtimes before/after)

## Risk Assessment
- Format drift across Claude versions → tolerant `Value` extraction, unknown → visible behind the noise toggle; fixtures cover today's shapes.
- Payload too big for IPC/React → preview limits; measured in step 8 with a fallback.
- Cloud read races a clone rebuild → read under `SyncLock`, error mapped to `busy`.
- Claude appending while reading → last partial line dropped by the tolerant parser.

## Security Considerations
- Transcript text is untrusted (web fetch output, tool output): no raw HTML, no `dangerouslySetInnerHTML`; links never navigate the webview, only `openUrl` for http/https/mailto after a click.
- Remote images blocked by CSP already; markdown images rendered as links.
- Cloud chunks are verified (`remote::chunk` checks name/len); nothing decrypted is written to disk.
