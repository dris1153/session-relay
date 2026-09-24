---
phase: 2
title: "Diffs, persisted outputs, subagents, images"
status: completed
priority: P2
effort: "1d"
dependencies: [1]
---

# Phase 2: Diffs, persisted outputs, subagents, images

## Context Links
- [plan.md](./plan.md) · [phase 1](./phase-01-parser-ipc-and-basic-viewer.md)

## Overview
Deepen tool blocks: Edit/Write diffs, full outputs Claude saved to `tool-results/`, nested subagent conversations, and images pasted into prompts.

## Key Insights
- `toolUseResult` is structured for most tools (886 of 919 records are objects): Bash → `stdout`/`stderr`; Edit/Write → `filePath`, `structuredPatch` (hunks), Write also `content`/`type`.
- Large outputs: the `tool_result` text starts with `<persisted-output>` and contains `Full output saved to: <abs path>\tool-results\<name>.txt`, then a 2 KB preview. The file lives at `<sid>/tool-results/<name>` (local) or the same rel in the manifest (cloud).
- Subagents: `<sid>/subagents/agent-<id>.jsonl` + `agent-<id>.meta.json` `{agentType, description, toolUseId}`; the parent `Agent` tool_use id equals `toolUseId`. Agents can spawn agents (`spawnDepth`).
- User `image` blocks carry base64 `source.data` + `media_type`; CSP allows `data:` images.

## Requirements
- Functional: Edit/Write show a unified diff (monochrome: added lines on Soft Stone with `+`, removed lines Ashen with `−`, context Graphite); Bash shows stdout/stderr separately; persisted outputs load in full on "show all"; an `Agent` block shows agent type + description and expands into the agent's own conversation (same renderer, indented), nested agents expand on demand; prompt images render as thumbnails that open larger in the viewer.
- Non-functional: diff capped at 400 lines per block with "show all"; images fetched lazily, capped at 10 MB each; subagent parse cached with the parent.

## Architecture
- `engine/transcript/tool_result.rs` — maps `toolUseResult` + tool name to `Block.tool.{output, diff, stderr}`; `diff = [{old_start, new_start, lines: ["+…", "-…", " …"]}]`.
- Persisted output: detect the `<persisted-output>` prefix, extract the file name after the last `tool-results` separator (validate `[A-Za-z0-9_.-]{1,128}`), detail ref `file:<name>`.
- Subagents: source lists `<sid>/subagents/*.meta.json` (local dir or manifest keys), maps `toolUseId → agent id`; `Block.tool.subagent = {agent_type, description, ref: "agent:<id>"}`; `session_detail("agent:<id>")` returns a nested `SessionView` from the same parser.
- Images: `Item.user.images = [ref "img:<uuid>:<n>"]`; detail returns a `data:` URI built in Rust (media type allow-list: png, jpeg, gif, webp).
- Source abstraction from phase 1 gains `read_rel(rel)` (local file under the project dir, refusing links and `..`, or cloud manifest entry) so all of the above work for both sides.

## Related Code Files
- Create: `src-tauri/src/engine/transcript/tool_result.rs`, `src/features/session-viewer/{diff-view,subagent-block,image-thumbs}.tsx`, fixtures for Edit/Write/Bash results, persisted output, a subagent pair, an image block
- Modify: `engine/transcript/{mod,items}.rs`, `commands/session_view.rs`, `src/features/session-viewer/tool-block.tsx`, `src/lib/tauri-commands.ts`, locales

## Implementation Steps
1. Fixtures: Edit with `structuredPatch`, Write (create), Bash with stderr, persisted-output result + its `tool-results` file, subagent jsonl + meta, user image (tiny PNG).
2. `tool_result.rs` + tests.
3. `read_rel` for both sides with path validation; persisted output detail.
4. Subagent mapping + nested view + tests (depth 2).
5. Image detail with media-type allow-list.
6. UI: diff view, stdout/stderr, subagent block (lazy), image thumbnails + enlarged view.
7. Locales; clippy, tests, tsc, build; code review.

## Todo List
- [x] fixtures
- [x] tool_result mapping
- [x] load_rel (local + cloud)
- [x] persisted outputs
- [x] subagents nested
- [x] images
- [x] UI pieces
- [x] review

## Success Criteria
- [ ] Every Edit/Write in a real session shows a readable diff
- [ ] "Show all" on a persisted output shows the full file, for local and cloud copies
- [ ] Agent blocks open the right subagent conversation (matched by `toolUseId`)
- [ ] Prompt images display; nothing outside the session folder can be read through a crafted ref

## Risk Assessment
- `toolUseResult` shapes vary per tool and version → unknown shapes fall back to the plain `tool_result` text.
- Persisted file missing (cleanup, not synced) → show the 2 KB preview with a note.

## Security Considerations
- Detail refs are parsed strictly; `read_rel` only serves `<sid>/tool-results/<name>` and `<sid>/subagents/agent-<id>.{jsonl,meta.json}` under the session folder; links/junctions refused like `transfer::refuse_links`.
- Image data URIs only for allow-listed media types.

## Implementation Notes (2026-09-24)
- Engine: `transcript/content.rs` (user content, images with a type allow-list and a 10 MB cap), `tool_result.rs` (diffs, agent ref, saved output), `resolve.rs` (ref paths such as `agent:<a>/agent:<b>/out:<tool>`, depth ≤ 8), `usage.rs`; `source::load_rel` shares the loader with `load` and accepts only paths `file_set::classify` accepts under `tool-results/` and `subagents/`.
- Agent ↔ file: `toolUseResult.agentId` (present on all 420 real Agent results) instead of scanning `*.meta.json`. Nested agents are assumed to live in the same `subagents/` folder (no real sample yet).
- `session_detail` returns `text {text, truncated}` (capped at 2 MB) | `session` | `image` | `gone` (saved output removed by Claude: the preview stays with a note).
- Cache: at most 2 sessions and 10 subagents, most recently used first; a new parse of a session drops its subagents; subagent files are reread when their signature changes (async agents keep writing).
- Not done: Bash stdout/stderr split (the result text Claude saw already holds both; the error flag marks failures); "show all" for diffs over 400 lines (the note says only part is shown).
- Real data (this session, 28 MB): 501 diffs, 21 agents (all 21 files found), 4 saved outputs.

## Review Log (2026-09-24)
Report: [code-reviewer-260924-phase-02-details.md](./reports/code-reviewer-260924-phase-02-details.md) · 7/10, 0 critical, 2 high, 4 medium, 8 low. Security: no issue found.

Fixed: H1 a FIFO cache shared by sessions and agents evicted the session after 12 agents (now separate budgets, LRU, unit tests); H2 memory (2 sessions again; images of skill loads and notices are no longer stored); M1 removed saved outputs show a note instead of an error; M2 subagents reread on change; M3 detail text capped at 2 MB; M4 image cap, nested-agent and resolver tests, diff note wording; L device names (`CON`, `NUL`) rejected through `file_set::classify` and non-files refused; Whole files from the cloud are not path-expanded; `persistedOutputPath` preferred; `bashEditDiff.moreFiles` marks the diff as partial; empty image wrapper removed; images named for screen readers; the "Show system events" switch applies inside subagents; ref depth capped.
Not changed: the import cycle message-item → tool-block → subagent-block → message-item (render-time only, safe in ESM).
