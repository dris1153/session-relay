# Code Review: Phase 2 (diffs, saved outputs, subagents, images)

Date: 2026-09-24 · Scope: uncommitted diff on top of `954ea7b` (engine/transcript/{content,tool_result,usage,mod,items,records,source}.rs, transfer.rs, transcript_cache.rs, commands/session_view.rs, tests, session-viewer UI, tauri-commands.ts, locales)
Checks: `cargo clippy --all-targets -- -D warnings` clean · `cargo test` all green (transcript_details 3/3, `parse_real` ignored) · `pnpm exec tsc --noEmit -p .` clean · `pnpm build` OK (session-viewer chunk 166 KB / 49 KB gzip) · en/vi parity: 0 missing keys, placeholders match · all touched files < 200 lines (mod.rs 199).
Method: code reading. Aggregate shape scan of the 36 real sessions under `~/.claude/projects` (1.6 GB, read-only, counts only). A scratch crate (path dependency, own target dir) with a counting allocator measured the parse heap. The cache file was `include!`d into a scratch bin to reproduce eviction.

**Score: 7/10.** The trust boundary is tight: I found no way to read outside `<project>/<sid>/`, and images are safe to render. Correctness holds on real shapes. One cache bug breaks the viewer in sessions with many agents, and memory grew about 6× in the worst case.

## Security verdict
- **Path traversal: none found.** Refs are a strict grammar (`in:`/`out:`/`img:`/`agent:`, `/`-joined). `img:<n>` goes through `Vec::get`. `agent:<id>` resolves only if that id was recorded by an Agent/Task result in the *current* transcript, and ids are alnum ≤ 64 (`valid_agent_id`). `out:` yields only `tool-results/<name>`, where `<name>` is the last component of the recorded path, validated at parse time (`tool_result.rs:87-94`). `read_rel` validates again (`source.rs:61-65`), `session_id` must be 36 hex/dash characters, and `refuse_links` walks `<sid>`, `tool-results`/`subagents` and the file (`source.rs:74`). A recorded path that points at another project folder (2 real cases) still reads only this session's folder. That is the right choice.
- **Data URIs:** built in Rust from an allow-list (png/jpeg/gif/webp, no SVG), rendered as `<img src>` through React attribute escaping. CSP `img-src 'self' data:` is unchanged and sufficient. The base64 payload is not validated, but it cannot escape an `img` attribute.
- **Minor hardening gaps:** L1 (Windows device names) and L6 (ref depth), below.

## Critical
None.

## High

**H1. Opening the 12th subagent evicts the session itself: every later detail fails with "This session is no longer here".** `transcript_cache.rs:7,26-38`, `commands/session_view.rs:56`
- The cache is FIFO over all scopes (`insert(0)` + `truncate(12)`), and `find` does not refresh an entry. Each opened agent pushes the session's `""` entry one slot back. `session_detail` needs that entry for every ref (Show all, images, nested agents), and it returns `SessionNotFound` once the entry is gone. Reproduced with a copy of the cache:
  ```
  after opening agent #11: session entry cached = true
  after opening agent #12: session entry cached = false
  ```
  The two limits compound. If the viewed session was already the oldest entry (a cache hit is not moved to the front), the first agent opened evicts it. Real data: 22 of 36 sessions have subagents, with up to 170 per session. The problem passes CI because no test goes through the cache or `session_detail`.
- Fix: budget sessions and agents separately and make the cache LRU:
  ```rust
  const SESSIONS: usize = 2; // both sides of a diverged session
  const AGENTS: usize = 10;
  // find(): on hit, move the entry to index 0.
  // insert(): after insert(0, ..), keep the first SESSIONS entries with scope "" and the
  // first AGENTS with scope != "", and drop agent entries whose session entry is gone.
  ```
  Add a unit test in `transcript_cache.rs`: session + 12 agents, then the session is still found.

**H2. Worst-case resident memory rose from ~2 to ~12 full session parses, and each parse now carries all images.** `transcript_cache.rs:7`, `engine/transcript/mod.rs:92`, `items.rs:114`
- `ENTRIES` went from 2 to 12, and `open_session` inserts never evict other sessions, so browsing 12 sessions keeps 12 parses. Measured (release, counting allocator):

  | file | parse | retained heap | of which images |
  |---|---|---|---|
  | 173 MB | 390 ms | 79 MB | 53 MB (289 stored, 245 referenced) |
  | 140 MB | 324 ms | 66 MB | 39 MB |
  | 105 MB | 381 ms | 41 MB | 14 MB (110 stored, **21 referenced**) |

  That is up to ~1 GB held by a tray app until the engine is dropped. `store_images` runs before the meta/notification branch (`items.rs:114-120`), so images in `isMeta` records are stored but no item references them. In real data those are skill loads ("Base directory for this skill: …", 317 of 393 image-bearing user records).
- Fix: H1's `SESSIONS = 2` restores the old bound. Call `store_images` only in the `Item::User` branch. Optionally keep images as decoded bytes (−25%) and base64-encode them in `session_detail`.

## Medium

**M1. A deleted saved output shows "This session is no longer here. Refresh the list and try again."** `source.rs:76`, `session_view.rs:77-79`
- About 30% of real `<persisted-output>` results (21 of 71: 6 main, 15 in agent files) point at `tool-results` files Claude Code has already deleted. Every one of them offers "Show all" (`mod.rs:157`, `output_truncated || persisted`) and then fails with a misleading message. The spec (Risk Assessment) asks for "the 2 KB preview with a note". A cloud copy without the file fails the same way.
- Fix: in `session_detail`, map `SessionNotFound` from `Detail::File` to `DetailView::Text { text: <stored preview> }` plus a flag, or add an error code `output_gone` with its own locale string. The preview is already in `Block::Tool.output`, so return it from `Transcript::detail` as a fallback (`Detail::File { rel, fallback }`).

**M2. A subagent's parse is cached with an empty signature and never revalidated.** `session_view.rs:68`
- 373 of 420 real agents are async (`status: async_launched`), so their file keeps growing after the parent's result is written. If the parent session is idle while a background agent runs (the parent file is unchanged, so `open_session` reuses the cached session and does not drop its agents), reopening the viewer shows the partial conversation with no hint. The cloud side has the same problem: a sync can change `agent-<id>.jsonl` without changing `<sid>.jsonl`.
- Fix: generalize `source::load` to `load_rel(.., rel, known)` (local `len:mtime`, cloud manifest hash; the code exists already). Store that signature in the agent entry and compare on `find`, exactly like the session path does.

**M3. "Show all" on a saved output ships the whole file through IPC into a `<pre>`.** `session_view.rs:77-79`, `tool-block.tsx:54`
- The largest real `tool-results` file is 10.6 MB. JSON-escaping it, then wrapping it in `whitespace-pre-wrap break-words` inside a `max-h-96` box, stalls WebView2 for seconds and costs ~100 MB+ of DOM text. Fix: cap the served text (for example 1 MB, cut on a char boundary with `cut()`), return `truncated: true`, and show a note.

**M4. Spec items missing or unverified** (phase-02 Requirements / Implementation Steps)
- (a) "Bash shows stdout/stderr separately": not implemented; `toolUseResult.stdout/stderr` are ignored.
- (b) "diff capped at 400 lines per block with 'show all'": there is no `diff:` detail ref. Instead the note says "The full change is in the input", which is false for shell edits (the input is a command) and for Write updates (the input is the new content, not the change).
- (c) "images capped at 10 MB each": no cap. The real maximum is 0.68 MB base64, so the risk is low; a one-line `data.len() <= 10 << 20` check in `content::image` covers it.
- (d) Tests: no depth-2 nested agent test (Implementation Step 4), and no test of `session_detail` ref walking or cache scoping. A cache test would have caught H1.

## Low

**L1. `read_rel`'s own validator differs from `file_set::classify`.** `tool_result.rs:92-94`, `source.rs:61-65`. It accepts Windows device names (`CON`, `NUL`, `COM1`, `aux.txt`) and trailing dots, which `is_unsafe_component` rejects, and it caps names at 128 characters where the sync allows 160. It is reachable only through transcript text: any tool output that starts with `<persisted-output>` and contains `saved to: …\COM1` (for example, a prompt-injected `echo` in Bash). On Windows, `std::fs::read(…\tool-results\COM1)` can open the device and block a worker thread. Fix (also DRY): `file_set::classify(&format!("{session_id}/{rel}")).is_some()` plus the two prefix checks, and `symlink_metadata(&path)?.is_file()` before reading.

**L2. The cloud `read_rel` expands path placeholders in Whole-class files.** `source.rs:104`. `tool-results/*` files are Whole: `snapshot::take` never normalizes them and `transfer::write_verified` never expands them, but `cloud()` always calls `expand`. The viewer can then differ from a restored file (only if the content contains the placeholder bytes). Fix: expand only when `entry.class == FileClass::Append`.

**L3. The persisted-output location is parsed from the text before `toolUseResult.persistedOutputPath`.** `tool_result.rs:39-42`. Any tool output that starts with the tag is treated as saved, and its real inline text becomes unreachable. Prefer the structured field when it is present; all 19 real cases agree with the text.

**L4. Shell-edit diffs are silently partial.** `tool_result.rs:57-59`. `bashEditDiff.moreFiles > 0` in 14% of real shell edits (237 of 1 696). `changedFiles` lists them but they are not shown. `created`/`deleted` flags are not marked, and `unavailable: true` (11 cases) shows nothing. Show "and N more files" from `moreFiles`, and mark created/deleted in the file header.

**L5. UI details**
- `message-item.tsx:38`: the `<div className="mt-2">` wrapper renders even when there are no images, adding 8 px to every prompt bubble. Render it only when `item.images.length > 0`.
- `image-thumbs.tsx:32-33`: the thumbnail buttons have no accessible name (`alt=""` inside `<button>`) and no `aria-expanded`. Use `alt={t("viewer.image_n", { n: i + 1 })}` or an `aria-label`.
- `subagent-block.tsx:36`: always hides noisy items and ignores "Show system events".
- `image-thumbs.tsx:14`: `Promise.all` means one failed image hides all of them.

**L6. Nested ref depth is unbounded.** `session_view.rs:58-75`. If a crafted transcript records an Agent result that names its own id, `agent:a/agent:a/…` re-reads and re-parses the file for every segment and churns the cache. Cap the number of segments (for example 6).

**L7. Circular import** `message-item → tool-block → subagent-block → message-item`. It is safe today because `MessageItem`/`isShown` are hoisted function declarations. Converting either to `const`/`memo()` would hit the TDZ, and Fast Refresh falls back to full reloads on cycles. Move `LoadDetail` and `isShown` to `session-viewer/item-visibility.ts`, or pass `renderItem` into `SubagentBlock`.

**L8. Wording nits.** "first 400 changed lines" also counts context lines. Removed lines show ASCII `-` while the hunk header uses `−`. Clay for error text follows the phase-1 pattern, but DESIGN.md reserves clay for decorative marks. Keep it consistent or revisit both phases together.

## Correctness on real shapes (verified, no finding)
- Parallel results: no real user record holds more than one `tool_result`, so `toolUseResult` given to the first result (`content.rs:42`) is always the right one.
- Write: 3 516 `type: create` (empty `structuredPatch`, handled as whole-content additions) and 654 `update` (patch). Edit: 5 074, all with a patch. MultiEdit: none in the data; the shape is compatible (`filePath` + `structuredPatch`).
- Agent: all 420 results (47 sync, 373 async) carry `agentId`, all ids are valid, and all 420 files exist at `<sid>/subagents/agent-<id>.jsonl`. Keying on `agentId` rather than `meta.json` is sound. No nested agent calls appear in the data, so the same-folder assumption is untested (see questions).
- Saved outputs in agent files also live in the parent's `<sid>/tool-results/` (27 found), so `agent:<id>/out:<tool>` resolves correctly.
- Parse cost: 173 MB in 390 ms (phase 1: 339 ms). View JSON 15 MB (was 13).

## Positive observations
- Refs are resolved against what the transcript recorded, never against webview paths. Validation runs twice (parse and `read_rel`). Only the file name of a recorded path is used.
- Images load lazily per click. The data URI is assembled in Rust with an allow-list. `persisted` is `#[serde(skip)]`.
- Lenient parsing is kept: unknown shapes fall back to the plain result text.
- `refuse_links` is reused instead of duplicated. The tests cover bad rels for both sides.

## Recommended actions
1. H1 + H2: split the cache budgets (2 sessions + ~10 agents, LRU) and add a cache unit test.
2. H2: store images only for `Item::User`.
3. M1: fall back to the preview plus a note for missing saved outputs.
4. M2: give agent entries a signature (`load_rel`).
5. M3: cap served saved-output text.
6. M4: decide on stderr and diff "show all" (implement, or amend the spec), and add the image cap and a depth-2 test.
7. L1: switch `read_rel` to `file_set::classify` and add an `is_file` check.

## Unresolved questions
1. Where does Claude Code write the file of an agent started *by a subagent*: the same `<sid>/subagents/` folder (assumed here) or nested? No real sample exists.
2. Should an async agent's final `<task-notification>` be linked to its Agent block? Today the block's output is only "Async agent launched…".
3. Was `ENTRIES = 12` meant to cover sessions too, or only agents? H1/H2 assume the latter.
