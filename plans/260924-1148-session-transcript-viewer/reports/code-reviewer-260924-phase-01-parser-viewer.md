# Code Review: Phase 1 (transcript parser, IPC, basic viewer)

Date: 2026-09-24 · Scope: uncommitted diff + untracked files (engine/transcript, transcript_cache, commands/session_view, session-viewer UI, dashboard wiring, locales, deps)
Checks: `cargo clippy --all-targets -- -D warnings` clean · `cargo test` all green (3 new transcript tests pass, `parse_real` ignored) · `npx tsc --noEmit -p .` clean · `npm run build` OK (viewer chunk 162 KB / 48 KB gzip, lazy) · en/vi parity: 0 missing keys, every `viewer.*` key used in code exists · all touched files < 200 lines (dashboard-page.tsx 197).
Method: code reading plus a throwaway scratch crate (path dependency on `src-tauri`, since deleted) that fed synthetic JSONL shapes to `transcript::parse`. Probe output is quoted below.

**Score: 7/10.** The trust boundary is tight, the markdown is safe, and the cloud read is careful. Two things block the success criteria: (a) the branch walk trusts `parentUuid` so strictly that one plausible real-world shape (parallel tool results), a torn line or a parentless trailing record silently hides tool results or whole histories, and (b) the markdown links do not work outside github.com.

## Security verdict (question 1)
- **No file outside the project dir or the store is reachable.** `key_hash` resolves only through the dashboard cache (`dashboard::project_key`). `session_id` is whitelisted to 36 hex/dash characters (`source.rs:33`), so `..`, `/`, `\` and `:` cannot pass (the test covers it). The path is `projects_dir/<dir_for(key)>/<sid>.jsonl`, and `encode_dir` yields a single segment. `side` is a serde enum. `reference` is only a `HashMap` lookup. Cloud paths come from the MAC'd manifest, and the chunks are verified by name and length.
- **Markdown:** `skipHtml`, no `rehype-raw`, no `dangerouslySetInnerHTML` anywhere in `src/`. `a` becomes a `<button>`, so the webview never navigates. react-markdown's `defaultUrlTransform` already blanks `javascript:`, and the `WEB` regex gates `openUrl`. `img` becomes text. The CSP (`default-src 'self'`, `img-src 'self' data: avatars`) is unchanged and adequate. Nothing decrypted is written to disk. The only write is the lock's `holder.json` in the app dir, never under `~/.claude`.

## Critical
None.

## High

**H1. Parallel tool calls: a result parented to its own tool_use record falls off the branch, so the tool shows "No result recorded" (or the whole tool call disappears).** `engine/transcript/branch.rs:18,26`
- Failure: newer Claude Code writes `sourceToolAssistantUUID` on tool_result records, and third-party format write-ups say the result's `parentUuid` points at the assistant record that issued the call. With parallel calls, the file then holds sibling branches: `a1(t1) ← a2(t2)`, `r1.parent=a1`, `r2.parent=a2`, `a3.parent=r2`. The walk from `a3` never visits `r1`. `r1` is a `User` record, so the attach rule (`is_conversation → only if uuid is None`) drops it. Probe:
  ```
  == parallel results parented to their tool_use: off_branch=1 tool_calls=2
     asst tool t1 output=None          <- result exists in the file
     asst tool t2 output=Some("result t2")
  == parallel, next turn off r1: off_branch=2 tool_calls=1
     asst tool t1 output=Some("result t1")   <- tool_use t2 vanished entirely
  ```
  The real 24 MB file lost 43 of 2 705 messages as "off branch". Some of those may be exactly this, and they are then counted as "messages on rewound branches". This violates the success criterion "each tool call shows its result". It passes CI because both fixtures are linear chains.
- Verify first: extend `parse_real` to print the number of on-path `Block::Tool { output: None }` (it should be ≤ 1, the last call) and a breakdown of off-branch records by kind (tool-result-only user vs prompt vs assistant).
- Fix (branch.rs, after the walk, still O(n)): (1) keep an off-path non-sidechain assistant record whose `message.id` equals the id of an on-path assistant record (parallel siblings of one API message); (2) keep an off-path user record that holds only `tool_result` blocks when any of its `tool_use_id`s belongs to a kept tool_use (pair by id, not by parent, so case 1b also works). Rewinds are unaffected, because a rewind always restarts at a typed prompt. Add a `parallel.jsonl` fixture with both shapes.

**H2. Markdown links outside github.com / git-scm.com never open, and nothing tells the user.** `src-tauri/capabilities/default.json:11-18`, `markdown-text.tsx:11`
- Failure: `opener:allow-open-url` is scoped to `https://github.com/*` and `https://git-scm.com/*` only. Clicking `[docs](https://docs.rs/…)` or `mailto:` makes the plugin reject the URL, and `.catch(() => {})` swallows the rejection. Most links Claude writes point at docs sites, so the "links open in the browser" requirement fails for them.
- Fix: either add `{ "url": "http://*" }, { "url": "https://*" }, { "url": "mailto:*" }` to the existing allow list (the same set as the plugin's `allow-default-urls` minus `tel:`), or keep the capability narrow and add a Rust command `open_link(url)` that checks the scheme and calls `app.opener().open_url(..)`, which the JS scope does not restrict. Either way, log or report the rejection instead of an empty catch.

## Medium

**M1. A torn line mid-file hides the whole earlier history as "rewound".** `branch.rs:18`
- Failure: `cur = parent.or(logical_parent).and_then(index.get)`. When `parentUuid` names a record that failed to parse, the walk stops there. That record can be missing because Claude was killed mid-append and `--resume` appended onto the partial line. Everything before it becomes `off_branch`. Probe: `== torn line mid-file: off_branch=2` shows only "second prompt/second answer", and the header says "2 messages on rewound branches are not shown". The requirement "malformed lines never fail the parse" holds only in letter.
- Fix: when `parent` is `Some(id)` but not in `index`, try `logical_parent`, then fall back to the nearest preceding non-sidechain user/assistant record in file order (`recs[..i].iter().rposition(..)`). Count these as `broken_links` rather than `off_branch`, so the UI never calls them rewinds. Add a fixture with a mid-file broken line.

**M2. The leaf may be a parentless `system` record, and then the whole conversation is hidden.** `branch.rs:10,33-35`
- Failure: `is_message` includes `System`. If the newest record is a system record with `parentUuid: null` and no `logicalParentUuid` (any subtype other than `compact_boundary` written that way, now or in a future version), the path is that one record. Probe: `== trailing parentless system record: off_branch=2 prompts=0` shows one noisy event, so the viewer reports "Nothing to show in this session."
- Fix: pick the leaf among user/assistant only (as the brainstorm describes). System and attachment records attach through the parent rule. Make that rule accept `parent.or(logical_parent)`, so a trailing `compact_boundary` still shows.

**M3. A prompt that merely quotes `<command-name>…</command-name>` is replaced by the command.** `items.rs:97,135-140`
- Failure: `command_prompt` searches for the tags anywhere in the text. Probe: the prompt "Why does the parser show `<command-name>/cook</command-name>` as the prompt? Please explain in detail." is displayed as `/cook`. The user's real text is lost, which is plausible in this very project (debugging transcripts with Claude).
- Fix: prettify only when `text.trim_start()` starts with `<command-message>` or `<command-name>` (the shape Claude Code writes).

**M4. Decrypted cloud transcripts stay in memory and remain servable after sign-out or a storage block.** `transcript_cache.rs` (never cleared), `commands/session_view.rs:40`, `app_state.rs:93`
- Failure: `logout`, `check_storage` (public/re-keyed store) and `watcher::recheck_storage` call `drop_engine()`, but `AppState.transcripts` keeps up to two full transcripts (cloud plaintext included). `session_detail` does not check the engine, so it still returns full tool input/output after sign-out. The dashboard cache and `ManifestCache` are reset with the engine, and this cache should follow the same rule.
- Fix: add `TranscriptCache::clear()`, call it from `drop_engine` and `install_engine`, and start `session_detail` with `state.engine().ok_or(Error::NotLoggedIn)?`.

**M5. The item key is the position in the filtered list, so toggling "Show system events" remounts every item.** `session-viewer.tsx:39`
- Failure: `key={`${i}:${item.uuid}`}` uses `i` from `view.items.filter(..)`. Showing or hiding noise shifts almost every index. React unmounts and remounts about 2–3k items, re-parses all markdown, collapses every opened tool `<details>` and drops text already loaded with "Show all". Switching between two sessions can also reuse per-item state when index and uuid collide (items with a missing uuid use `""`).
- Fix: key by the index in `view.items` (`view.items.map((item, index) => ({ item, index })).filter(..)`), or have Rust emit a stable `index`. Also render `<SessionViewer key={`${keyHash}:${sessionId}:${side}`}>`.

## Low
- **L1. The side comes from the row's most urgent file state, not from the transcript's own state.** `sessions.ts:43`, `dashboard-page.tsx:93`: if `<sid>.jsonl` is `local_deleted` or `remote_only` but a `tool-results`/`subagents` file of the same session is `local_only`, the row reads `local_only`, so `local` is chosen and the viewer returns `session_not_found`, although a cloud copy exists. Keep the transcript file's own state in `SessionRow` (for example `transcriptState`), or fall back to cloud on `session_not_found`.
- **L2. The cache does not avoid I/O.** `source.rs:41-57`, `transcript_cache.rs:22-24`: the signature is computed only after the whole file is read. For cloud, that is after every chunk has been fetched and decrypted (about 15 git spawns for 24 MB) while holding `sync.lock`. A cache hit saves only the 70 ms parse. Compute the signature first (local: `metadata` len+mtime; cloud: `entry.hash`, known before `file_content`) and load only on a miss. The cloud signature also ignores the expansion dir: linking the project after a first view reuses a parse with `<project>\` paths. Append `local.display()` to the signature.
- **L3. Event text has no preview limit.** `mod.rs:102`: `meta` (skill bodies, compact summaries), `notification`, `ide` and orphan `tool_result` events are shipped whole. `<local-command-stdout>` and orphan results can be large. Cap them at `OUTPUT_PREVIEW` with a detail ref, as tool blocks do.
- **L4. Format gaps.** `records.rs:48`: `api_error` text comes from `content` only (real records may carry `error`/`retryAttempt` instead, so the UI shows "API error: " with nothing after it). `records.rs:172`: a `queued_command` whose `prompt` is an array gives empty text. `items.rs:9`: `<bash-input>`/`<bash-stdout>`/`<bash-stderr>` (the CLI's `!` mode) and "[Request interrupted by user…]" count as prompts.
- **L5. Markdown details.** `markdown-text.tsx:11`: relative links (the VS Code extension writes `[file.ts](src/file.ts#L4)`) look clickable but do nothing. Render them as plain or code text. `markdown-text.tsx:24`: the inline-code style (`px-1 rounded bg`) also applies to code inside `pre` blocks. Style `pre > code` separately.
- **L6. Silent failures in the UI.** `tool-block.tsx:35`: "Show all" errors are swallowed and the button just re-enables. `session-viewer.tsx:34`: an open error (for example `busy` after the 5 s lock wait during a long sync) has no retry, and the user must go back and click the row again.
- **L7. Accessibility.** `tool-block.tsx:12`: `display:flex` on `<summary>` removes the disclosure triangle in WebView2, so the only expand cue is the cursor. Focus is lost when the row button unmounts on open, and again on Back. Move focus to the back button or heading on open, and back to the row on close. The "←" in the Back label is read aloud; wrap it in `aria-hidden`.
- **L8. `formatCount`/`formatClock` create an `Intl` formatter per call.** `format.ts:25-31`: about 1.6k calls per render of a long session, repeated on every toggle. Memoize per language.
- **L9. Usage is taken from the first record of a `message.id`.** `items.rs:56`: this is correct if the usage is identical across records, as measured. Take the last (or max `output_tokens`) record so that a streamed partial first record cannot undercount.
- **L10. Tests and docs gaps.** No fixture covers cycles, duplicate uuids, parallel siblings (H1), a mid-file broken line (M1) or a parentless trailing record (M2). The plan's IPC contract still lists `other_side`, `truncated` and `event{kind}`, while the code has `input_truncated`/`output_truncated`, `summary` and an `event` field. `docs/system-architecture.md` (IPC), `project-changelog.md` and `development-roadmap.md` are not updated yet.

## Edge cases checked and OK
- Cycles and self-parent: `keep.insert` guard stops the walk. Duplicate uuids: the last occurrence wins (only `off_branch` is inflated). Compaction: `logicalParentUuid` is followed, so pre-compact history stays. Rewind: the abandoned branch is excluded and the new branch keeps file order.
- A tool_result written before a later tool_use record of the same `message.id` pairs correctly (the global `tools` map, blocks appended to the first record's item). An orphan result becomes a noisy `tool_result` event.
- Sidechain records are skipped for both items and meta. A truncated last line is dropped. Garbage/`{}`/`[1,2]` gives no items and no panic. `cut` respects UTF-8 boundaries (test with `é`).
- Cloud read: read-only git (`rev-parse`, `ls-tree`, `cat-file`), so stale `*.lock` files do not matter (recurring class 1 does not apply). No path to `with_store` means a git error can never trigger a clone rebuild. A missing or never-fetched clone makes `last_snapshot()` return `None` → `session_not_found`. Lock contention gives `busy` after 5 s. Placeholder expansion targets `projects_dir/<enc>` (same as `sync.rs:104`), or `<project>` when not linked.
- The cache parses outside the mutex and handles poisoning. Concurrent opens only cost a duplicate parse. `session_detail` after the file changed serves the newest parse, and tool ids are stable across appends.
- Frontend: `useSessionView` uses a per-effect `live` flag with no `run()` guard, so the StrictMode trap from Phase 4b does not apply. The viewer is lazy-loaded. `import type { ViewedSession }` is erased, so there is no eager import. The session row is a real `<button>` next to Restore/Delete, not nested, so no `stopPropagation` is needed. The memory row is disabled.

## Positive observations
- A clean separation: the engine returns a compact view model, full bodies stay in Rust, and details are served by reference.
- The input whitelist sits at the right layer (engine `source::load`), not in the UI.
- The fixtures are realistic (IDE blocks, split assistant records, `isMeta`, `isCompactSummary`, a queued command, an unknown record type, a broken tail) and the tests are named after behaviour.
- Performance is measured and recorded: 72 ms parse, 3.3 MB view, 584 ms click-to-paint, 7 ms scroll step.

## Recommended actions (in order)
1. H1: measure with `parse_real`, then keep message-id siblings and tool-result records paired by id, and add a fixture.
2. H2: widen the opener scope (or add a Rust `open_link`), and stop swallowing the error.
3. M1 + M2: harden the walk (dangling parent fallback, leaf = user/assistant, attach via `parent.or(logical_parent)`), and add fixtures.
4. M3, M4, M5: one-line or small fixes.
5. L1–L3 before Phase 3 (compare and search reuse the same load/cache path). Other Low items as convenient.

## Unresolved questions
1. How do Claude Code 2.1.278–2.1.280 parent tool_result records for parallel calls (`parentUuid` = the previous record, or = `sourceToolAssistantUUID`)? This decides whether H1 is live today or latent. The `parse_real` counts above answer it without sharing any content.
2. Do real `system/api_error` records have `content`? If not, which field should the marker show?
3. Does any system subtype other than `compact_boundary` have `parentUuid: null` in real files (M2)?
4. On `session_not_found` for the local side, should the viewer fall back to the cloud copy automatically, or offer a button?

**Status:** DONE_WITH_CONCERNS
**Summary:** Review complete. Trust boundary, markdown safety and the cloud read are sound. Two High findings (tool results dropped on parallel-call sibling branches, pending a real-file check; markdown links blocked by the opener scope) and five Medium branch-walk, cache and key issues should be fixed before Phase 1 is marked done.
