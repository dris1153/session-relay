# Brainstorm: session transcript viewer

## Problem
Clicking a session in the dashboard should open the full conversation, "as detailed as possible". Target: v0.2.0, after the v0.1.0 E2E.

## Decisions (user)
- Content: user + Claude messages, tool calls with results, nested subagents, per-turn metadata.
- Cloud-only sessions: view straight from the store (decrypt in memory), nothing written to `~/.claude`.
- Placement: replaces the detail pane, back button; project sidebar stays.
- Markdown: rendered safely (library, no raw HTML).
- Tools: search, prompt outline, copy / export Markdown, compare both copies of a diverged session.
- Branches: current branch only. System noise: hidden by default, toggle to show.

## Facts from a real transcript (24 MB, 8 902 lines, Claude Code 2.1.280)
- Record types: attachment 4 175, assistant 1 711, user 979, plus atis-latch, last-prompt, ai-title, mode, queue-operation, file-history-*, system. ~70 % of records are noise.
- Blocks: assistant tool_use 919 / text 141 / thinking 651 (only 66 have text; the rest store a signature only); user tool_result 918 / text 34 / image 3.
- `toolUseResult` is structured (dict in 886 of 919): Edit/Write carry `structuredPatch`, Bash stdout/stderr.
- System subtypes: stop_hook_summary, api_error, compact_boundary (parentUuid null, `logicalParentUuid` links across).
- Walking from the last user/assistant record back via `parentUuid` (fallback `logicalParentUuid`) keeps 2 662 of 2 705 messages; the rest are abandoned rewind branches.
- Subagents: `<sid>/subagents/agent-*.jsonl` + `.meta.json` {agentType, description, toolUseId} → link from the Task tool_use.
- Large outputs: `<sid>/tool-results/*.txt`.
- Assistant usage: input, output, cache_creation, cache_read tokens; model per message.

## Approaches
| Option | Verdict |
|---|---|
| A. Rust parses into a view model; big bodies fetched on expand; parsed session cached in memory | **Chosen** |
| B. Ship raw JSONL to the webview, parse in JS | Rejected: 24 MB over IPC, UI stalls, search duplicates Rust work |
| C. Rust pages items by range | Only if A measures too slow: search/outline/jump get complex |

## Design
### Engine (`engine/transcript/`)
- `parse(bytes) -> Transcript`: tolerant line parser (unknown record → raw event, never an error), branch walk, tool_use ↔ tool_result pairing, noise flag per event.
- Item kinds: `User {text, images, at}`, `Assistant {blocks: Text | Thinking{empty} | Tool{id, name, input_preview, output_preview, is_error, detail?, diff?, subagent?}, model, usage, at}`, `Event {kind, noisy, text}`.
- Bodies over ~8 KB, `tool-results` files, subagent transcripts and images become `detail` refs, fetched with `session_detail`.
- Sources: local file, or cloud copy decrypted into memory (split a `read_remote` out of `transfer::pull`: fetch chunks at the snapshot, decrypt, verify hash, expand the path placeholder to this machine's dir). Read under the sync lock or tolerate a concurrent clone rebuild.
- Cache: one parsed session keyed by (key_hash, session id, side, file signature).
- Search and Markdown export run in Rust over full content.

### IPC (draft)
- `open_session(key_hash, session_id, side: local|cloud) -> SessionView {meta, items, outline}`
- `session_detail(ref) -> Detail` (full output, subagent transcript, image data URI)
- `search_session(query) -> item indices`
- `export_session(path)`

### UI
- Viewer in the detail pane: header (title, model, version, span, token totals, search, noise toggle, export, local/cloud switch when diverged), outline of prompts on the left, message column.
- Long lists: CSS `content-visibility: auto` first; batch rendering only if measured slow.
- Markdown: `react-markdown` + `remark-gfm`; no raw HTML; links open in the external browser (never navigate the webview); remote images shown as links (CSP blocks them anyway).
- Diffs from `structuredPatch`, monochrome per DESIGN.md.
- Diverged compare: common prefix by uuid, marker "the copies differ from here"; "View" link in the Resolve dialog.

## Risks
- Undocumented, version-dependent format → tolerant parser, fixtures from real files with content scrubbed, unknown records visible behind the noise toggle.
- Thinking mostly empty → show "thinking (not stored)".
- Payload size: estimate 2–4 MB view model for 24 MB; targets: open < 1 s local, < 2 s cloud; measure on release build.
- Untrusted content (web fetch output) → never render HTML; external links via opener only.
- New dependency: react-markdown + remark-gfm (~40 KB gzip).
- Effort ≈ 3–4 days.

## Phases
1. Parser + IPC + basic viewer: messages, markdown, tool blocks, metadata, noise filter, current branch, local + cloud.
2. Depth: Edit/Write diffs, tool-results files, nested subagents, images.
3. Tools: search, outline, copy / export, diverged compare.

## Success criteria
- The 24 MB session opens under 1 s (local) / 2 s (cloud) on a release build, scrolls smoothly.
- Every user prompt and Claude text of the current branch is shown; tool calls pair with their results; subagents open inline.
- No file under `~/.claude` is written by viewing.
