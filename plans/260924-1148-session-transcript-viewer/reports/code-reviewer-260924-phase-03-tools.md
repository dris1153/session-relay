# Code Review: Phase 3 (search, outline, export, compare)

Date: 2026-09-24 · Scope: uncommitted diff on top of `e4e9bcc`: engine/transcript/{search,compare,export,preview,mod}.rs, commands/{session_tools,session_view}.rs, lib.rs, capabilities, tests/transcript_tools.rs, session-viewer UI (viewer-header, search-bar, outline-menu, copy-button, message-item, turn-section, conversation, session-viewer), scroll-to-item.ts, use-stick-to-bottom.ts, dashboard (page, header, conflict-dialog), locales, design-tokens.css.
Checks: `cargo clippy --all-targets -- -D warnings` clean · `cargo test` all green (transcript_tools 3/3) · `pnpm exec tsc --noEmit -p .` clean · `pnpm build` OK (session-viewer chunk 176 KB / 52.5 KB gzip) · en/vi parity: 252/252 keys, placeholders match · all touched files < 200 lines (dashboard-page.tsx is the largest at 191).
Method:
- Code reading.
- A release scratch crate (path dependency) ran `search`, `first_difference` and `write_markdown` on 3 real sessions (173/63/11 MB), read-only, printing counts only.
- Prefix cuts of the real files, to simulate a cloud copy that is behind the local one.
- A copy of `check_export_path`'s logic run against a fake protected dir in the scratchpad.

**Score: 6.5/10.** The engine side is solid and fast. Search takes 20 ms and export takes 11 ms on 173 MB. Fences, depth bounds and allocation are handled well. Real data breaks two headline features, though. A search for "error" returns hidden hook-noise events 95% of the time. The divergence marker lands on a hidden item in 9–13 of 15 simulated cases. One React key change also regresses the "toggle keeps blocks open" behavior. The export path guard can be bypassed; that is a defense-in-depth gap, since the strict CSP makes a webview compromise unlikely.

## Security verdict
- Search and compare take no paths. The query is a plain substring (no regex). Refs and subagent ids still pass through `classify` (`agent-[A-Za-z0-9]{1,64}`) and `refuse_links`. No new read surface.
- Export takes the path from the webview, so the command cannot know that the dialog produced it. The `canonicalize(parent)` + `starts_with` guard correctly rejects `..`, case changes, trailing dots, `\\?\` and junctioned parents (verified). It does not stop the cases in M1.
- Export content is the transcript only: header facts (models, version, branch, cwd, times, token counts), prompts, replies, thinking, tool input/output (output capped at 50 KB), diffs and subagents. No image data, no machine name, no account. Noisy events are skipped.

## Critical
None.

## High

**H1. Search matches hidden noise by its event *type name*, and a hit in hidden noise force-enables "Show system events".** `engine/transcript/search.rs:28`, `session-viewer.tsx:45-49`
- `needle.in_text(event)` matches the raw kind (`hook_non_blocking_error`, `stop_hook_summary`, `meta`, …). Probe on the real 173 MB session:
  - "error" gives 1000 hits (the cap). 955 are hidden items, and 950 of those match only through the name `hook_non_blocking_error` (12,877 such events in this session). Real error hits past the cap are dropped.
  - "hook" gives 960/1000 name-only hits. "meta" 190/369. "summary" 263/499.
  - Even text matches land in hidden items: "không" 63, "system" 153, "ide" 180.
- The first Enter jumps to a hidden event. `goTo` then sets `showSystem(true)`, which renders about 15k extra `<li>` (77–86% of items are hidden noise) and leaves the switch on. Through H3, this also collapses every block the user had opened.
- Fix:
  1. Drop the `event` match entirely.
  2. Search hidden items only when they are shown. Pass the switch state: `search(query, system: bool)`, and skip `Event { noisy: true }` (and signature-only assistant items) when it is false. If the product wants hidden hits, keep them out of the count and do not flip the switch on a hit. The "hook failed" assertion in `tests/transcript_tools.rs:21` would then change to test the `system` flag.

**H2. The divergence marker usually sits on a hidden item, so it never renders.** `turn-section.tsx:44,46`, `viewer-header.tsx:47-51`, `engine/transcript/compare.rs:5-8`
- `first_difference` returns the first differing item. `TurnSection` draws the marker only if that exact index is among the shown `entries`. I parsed prefixes of real files (a cloud copy that is behind) at 15 line cuts each. The first differing item was hidden in 9/15, 13/15 and 12/15 cuts. That is expected: the Stop/auto-save hook writes `stop_hook_summary` and hook-error events right after each save. The success criterion "the marker sits at the first differing message on both sides" fails in most real cases.
- "Go to where they differ" is still shown. Clicking it flips system events on. When a copy is ahead only by hook noise, it lands on noise.
- Fix (UI): `const markAt = divergeAt === null ? null : entries.find((e) => e.index >= divergeAt)?.index ?? null;`. Pass `markAt` to `Conversation` and to the button. Hide the button when `markAt` is null, because nothing visible differs.

## Medium

**M1. The export path guard can be bypassed, and the command trusts a path from the webview.** `commands/session_tools.rs:48-73`
- All three cases below were verified against a fake protected dir in the scratchpad:
  - UNC loopback `\\localhost\C$\…\prot\x.md`: `canonicalize` returns `\\?\UNC\localhost\C$\…`, which does not `starts_with` `\\?\C:\…`. The guard returned OK and the file was **written into the protected dir**.
  - Hardlink: `out\link.md` is a hardlink to `prot\CLAUDE.md`. The parent is not protected, so the guard passes. `File::create` truncated and **overwrote the protected file**. A file symlink behaves the same way, because `File::create` follows it.
  - ADS: `…\prot:evil.md` has parent `…` and extension `md`. The guard passes and a stream is written onto the protected directory. This one is harmless, but it shows the guard is lexical.
- Other `\\server\share\x.md` paths are accepted. Canonicalizing one makes an SMB connection, which can leak the NTLM hash.
- In the normal flow the path comes from the native dialog and the CSP is strict, so exploiting this needs a compromised webview. The fix is cheap, though, and the spec lists this guard as a security control.
- Fix:
  1. Show the dialog in Rust: `app.dialog().file().add_filter("Markdown", &["md"]).set_file_name(name).blocking_save_file()` inside `blocking`. Drop the `path` argument and `dialog:allow-save`.
  2. Keep the guard for UX. After canonicalizing, accept only `Prefix::VerbatimDisk`.
  3. Write to `<path>.sr-tmp` in the same dir, then `fs::rename`. The rename replaces the directory entry, so it does not write through a hardlink or symlink. It also avoids leaving a truncated half-file when a write fails midway. Today an overwrite that fails destroys the old file.
- Also add a unit test for the guard (tempdir: protected, `..`, uppercase, junction). Nothing tests it today.

**M2. Toggling system events remounts shown messages: opened tool blocks close, "Show all" text and opened subagents are lost.** `turn-section.tsx:46`
- `rest.map(e => [marker, <MessageItem key={index}/>])` returns nested arrays. React reconciles each inner array as a *keyless fragment matched by slot* (`updateSlot` → `updateFragment` with key null). When `showSystem` changes, `rest` shifts and each slot's inner `MessageItem` key changes, so the item remounts. Phase 1 relied on `key={index}` directly in the array ("toggling system events keeps opened blocks open", `session-viewer.tsx:27`). H1 triggers the toggle automatically.
- Fix: `rest.flatMap(({ item, index }) => [...(index === markAt ? [<DivergeMarker key={`d${index}`} />] : []), <MessageItem key={index} … />])`, or `<Fragment key={index}>`.

**M3. The `data-block` wrapper adds a blank 12 px gap to about half of Claude's messages.** `message-item.tsx:53-57`
- `BlockView` returns `null` for signature-only thinking, but the `<div data-block>` wrapper stays and takes a `gap-3` slot. Real data: 2138 of 3951, 1107 of 2117 and 162 of 462 shown assistant items have such a block.
- Fix: skip the wrapper for `block.type === "thinking" && !block.text`, or add `className="empty:hidden"`.

**M4. Windows paths shown in a tool's summary often cannot be found.** `engine/transcript/search.rs:65-67`
- `input` is stored as pretty JSON, so `D:\Workspace\x` is `D:\\Workspace\\x` in the text searched. `summary` is not searched. Real session: 39 of 200 tool calls with a backslash in their summary were not found by searching their own summary text.
- Fix: add `self.in_text(summary)` (cheap). A better option is to search the unescaped string values of the input.

**M5. `session_detail` still needs the root in cache, and compare now evicts more often.** `commands/session_view.rs:88`
- Scenario: the user opens a live, `local_ahead` session A, then opens B within about a second, while A's compare is still loading.
  - Compare re-parses A-local (the file changed) and then inserts A-cloud.
  - The cache holds `[A-cloud, A-local]`, so B is evicted.
  - Every later "Show all", image or subagent in B fails with `session_not_found`.
- Fix: use `target.open(&state)?` instead of `find(..).ok_or(SessionNotFound)`. It is a stat when cached and heals after eviction. Search and export already work this way.

## Low
- **L1. The highlight can throw.** `scroll-to-item.ts:30-35`: offsets come from `toLocaleLowerCase()` text, but `İ` (U+0130) lowercases to 2 code units. A later match then goes to `setEnd` past the node length, which throws `IndexSizeError`. On the pending-jump path this happens inside `useEffect`. There is no error boundary, so the whole app unmounts. Fix: skip nodes where `lower.length !== text.length`, wrap in try/catch, and use `toLowerCase()` (locale-free, same as Rust).
- **L2. The nested `data-block` selector can pick a block inside an opened subagent.** `scroll-to-item.ts:9`: `li.querySelector('[data-block="N"]')` finds a nested subagent message's wrapper first when an earlier block's subagent is open. That needs 2+ Agent calls in one item (1–2 per real session). Fix: `:scope > div > [data-block="N"]`.
- **L3. The export recursion bound is off by one.** `export.rs:11,88`: `level` grows by 2 per nesting, so `level < 2 + AGENT_DEPTH` allows 2 levels, not the documented 3. Use `level < 2 + 2 * AGENT_DEPTH`, or pass a `depth` argument. No nested agents appear in real data.
- **L4. Cloud export can hang and silently drop subagents.** `session_tools.rs:55`: each cloud subagent load takes the SyncLock (5 s wait) and decrypts the manifest again. With 170 subagents and a save running, that is up to about 14 min, and failures become silent omissions (`.ok()`). Load the manifest once, or note skipped subagents in the file.
- **L5. Stale hit indexes on live sessions.** `session-viewer.tsx:45-49`: search, compare and export re-parse a changed file, but the UI keeps the older `view`. A hit at an index ≥ `view.items.length` sets `showSystem(true)` and leaves `pending` set forever. Guard with `item < view.items.length`, or refresh the view.
- **L6. Search requests can race.** `search-bar.tsx:21-24`: a slower earlier request can resolve last and jump to hits for the old query. Keep a request id and ignore stale responses.
- **L7. Highlights persist.** They survive clearing or editing the query and are cleared only by the next jump or on unmount. Clear on query change.
- **L8. Matches outside the preview give no visible cue.** They are beyond the 8 KB output, 2 KB input or 400 diff lines, and event text past 8 KB can never be expanded. The jump opens the block with no mark. The spec's fallback ("outline the matching item") is not implemented. A brief outline, or a "match is in the hidden part: Show all" note, would do.
- **L9. Design.** DESIGN.md:225 says Clay is "never used to … highlight links, or draw attention to data". The search highlight is a Clay background on data, and the color is hardcoded instead of using the token. Suggest a neutral mark, e.g. `background: color-mix(in srgb, var(--color-carbon-ink) 12%, transparent)` plus an underline. The Clay divergence line reads as an editorial mark and is fine.
- **L10. Accessibility.**
  - The side switch `role="group"` has no `aria-label`.
  - The `DivergeMarker` `<li role="separator">` inside an `<ol>` breaks list semantics (axe `list`).
  - The outline menu has no Escape handling, no focus move and no `aria-haspopup`.
  - The search counter "3 / 17" and the "Copied" state are not in an `aria-live` region.
  - Nested copy buttons all show when the outer message is hovered, because Tailwind `group-hover` matches any ancestor `.group`.
- **L11. Export UX.**
  - Refusing a protected folder shows only "Invalid data."; add `errors.export_path`.
  - A `save()` rejection is unhandled.
  - The button is not disabled while an export runs.
- **L12. Compare misses in-message differences.** `compare.rs`: an assistant item keeps its first record's uuid, so extra blocks of the same API message (cloud saved mid-message) compare as equal. The result can be `(None, None)` although the copies differ. This is acceptable; document it.
- **L13. The outline dropdown overflows narrow windows.** It is `w-[28rem]` anchored `left-0`. At the 880 px min width (sidebar 288 px) it runs past the window edge. Use `w-[min(28rem,calc(100vw-20rem))]` or anchor it right.

## Edge cases checked (OK)
- Hit indexes are full-list indexes, the same as `view()` (1:1 with `preview::item`) and `entry.index` / `data-index`. `block` equals the `data-block` index. The subagent `MessageItem`s have no `data-index`, so no collision.
- The jump opens the correct `<details>`: `ToolBlock`'s root is `<details>`, as are the thinking and long-event elements.
- Stick-to-bottom: a jump fires `scroll` (unpins) before the ResizeObserver callback in the same frame. A pending jump after the showSystem render lands correctly. The only nit: a target within 80 px of the end stays pinned and can be pushed up when its block expands.
- Compare when a copy is missing: `SessionNotFound` on either side gives `(None, None)`. Events without a uuid key on `(event, text)`. Real data has 0 uuid-less events and 0 empty user/assistant uuids.
- Unicode search: the ASCII fast path is correct (UTF-8 continuation bytes are never ASCII). The non-ASCII path uses `to_lowercase` on both sides. The Vietnamese query takes 17 ms on 173 MB. Full case folding (ß/SS, final sigma) and NFC/NFD are not handled. That is acceptable.
- Fences: the fence length is the longest backtick run + 1 (min 3), verified by the test. The 50 KB cut respects char boundaries. `time()` uses `get(..16)` and cannot panic.
- `Target::open` `expect`: `known` is `Some` only when `cached` is `Some`. Safe.
- Resolve dialog "View": only top-level `<36>.jsonl` rels exist (ALLOWED regex), so `!includes("/")` means transcript. The dialog only opens for the selected project, so the viewer renders.
- The save dialog on Windows sets a default extension (rfd `SetDefaultExtension`) and keeps the overwrite prompt.

## Plan TODO status
Search, export, commands, outline/search/copy and compare + Resolve "View" are implemented. **Docs (step 7) are not done**: `docs/system-architecture.md` (IPC for `search_session`/`compare_sides`/`export_session`), the changelog and the roadmap are untouched. The spec's success criterion "marker at first differing message" is not met in practice (H2).

## Recommended actions (in order)
1. H1: no event-name match; a `system` flag; no forced toggle.
2. H2 + M2: snap the marker to the next shown item, and use `flatMap` keys in `TurnSection`.
3. M1: Rust-side save dialog, a disk-prefix check, temp + rename, and a guard test.
4. M3, M4, M5: small one-line fixes.
5. L1 (crash guard), then the rest as polish. Docs.

## Unresolved questions
1. Does "shown noise included" in the spec mean hidden system events should be searchable at all? My reading is only the events shown by default (compact, api_error, mention, queued, interrupted).
2. Should compare still run for `local_ahead` sessions? It decrypts and parses the whole cloud copy on every open of a live session, and holds the SyncLock while it loads. The marker then only shows where unsaved content starts.
3. Should the export include saved `tool-results` files (currently only the record's preview text) and the uncapped tool inputs? The largest real input is 43 KB, so this is fine today.

**Status:** DONE_WITH_CONCERNS
**Summary:** Build, clippy, tests, tsc and i18n all pass, and the engine is fast. On real data, though, search is flooded by hidden hook-noise events and the divergence marker is mostly invisible (H1, H2). A key change also collapses opened blocks when system events are toggled (M2), and the export path guard can be bypassed through UNC loopback and hardlinks (M1).
