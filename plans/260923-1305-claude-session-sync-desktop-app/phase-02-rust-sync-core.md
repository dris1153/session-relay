---
phase: 2
title: "Rust Sync Core"
status: completed
priority: P1
effort: "4.5d"
dependencies: [1]
---

# Phase 2: Rust Sync Core

## Context Links
- plan.md §IPC Contract, §Red Team Review (#1–3, 6–13, 15)
- `reports/spike-results.md` (chunk size, keyring variant, encode rule, path normalization yes/no, git env)

## Implementation Notes (2026-09-23)
- Module dir is `src-tauri/src/engine/` (not `core/`: that name shadows Rust's `core` crate in macros).
- Extra modules vs plan: `context` (Engine), `remote` (reads via `git show`), `evaluate`, `transfer`, `overview` (refresh/list/delete), `chunker`, `sync_policy`, `git_process`, `fs_util`, `title`, `path_decode`, `maintenance`, `logging`. `recovery.rs` became `store_repo::recover` + `maintenance::on_startup`.
- Discovery fallback found on real data: dirs with only `memory/` left (transcripts cleaned up) have no `cwd` → decode the encoded dir name by walking the filesystem; accept only an unambiguous match (`path_decode.rs`). Real data: 23 repos found, 9 correctly without remote, 40 ms.
- `RemoteDeleted` keeps its base entry (dropping it would make the file `LocalOnly` and re-upload it); base entries with neither local nor cloud file are pruned.
- Links persisted in `links.json` (`repos`: remote → checkout root, `dirs`: encoded dir → key).

## Overview
Pure-Rust module `src-tauri/src/engine/` (no Tauri dependency): project mapping, file snapshotting, chunk/encrypt/manifest, isolated git transport, status, one `sync_project` op, backups, crash recovery. Integration-tested against a local bare repo with two simulated machines.

## Key Insights
- Identity of a local session dir comes from a **local map** (links), never re-derived from content of restored files (their `cwd` belongs to another machine).
- Two file classes: **append** (`*.jsonl`) snapshot up to last `\n`; **whole** (everything else) read entirely. Only append class can be Diverged; whole class uses 3-way + last-writer-wins.
- Chunk boundaries deterministic → status via keyed hashes of local ranges vs manifest chunk names; decrypt at most 1 chunk.
- Every git command on the clone runs under one app lock; status reads remote manifests via `git show <sha>:<path>`, never touching the worktree.
- Nothing written when nothing changed → no commit, no push.

## Architecture
```
core/
  paths.rs            app_dir (%LOCALAPPDATA%\dev.sessionrelay.desktop), claude_home (from settings),
                      encode_dir(path): UTF-16 units, [A-Za-z0-9] kept else '-', long-path rule per Spike F
  links.rs            Links { key_hash16 → {remote, subpath, local_root} } in settings; encoded dir for a link;
                      discover(): for each projects/<enc>: distinct cwd values (last first) that exist locally,
                      are not UNC/\\?\ and encode_dir(cwd)==enc (case-insens.) → origin_key_for(cwd) → add link if key free;
                      key already linked to other root → dir marked Secondary (warning, never synced)
                      memory/** belongs to key (remote, subpath="") in encode_dir(repo_root)/memory (Claude keys auto memory
                      by git repo, shared by subdirs/worktrees — Spike F); `autoMemoryDirectory` set → skip memory + warn
  project_identity.rs origin_key_for(path): toplevel + origin (parse .git/config; `.git` file/worktree → `git` fallback),
                      normalize → github.com/owner/repo lowercase; subpath rel to toplevel with '/'
  file_set.rs         allowlist: ^[0-9a-f-]{36}\.jsonl$ · ^[0-9a-f-]{36}/subagents/agent-[A-Za-z0-9]{1,64}\.(jsonl|meta\.json)$ ·
                      ^[0-9a-f-]{36}/tool-results/[A-Za-z0-9._-]{1,160}$ · ^memory/[A-Za-z0-9._-]{1,128}\.md$ ;
                      reject reserved names/trailing dot; skip `*.sr-tmp`; class = append if .jsonl else whole
  snapshot.rs         one pass per file: open, fix S (append: last '\n'; whole: len), stream [0..S) → keyed full hash +
                      chunks (append: seal at line end ≥ 4 MiB, hard cut 32 MiB; whole: 32 MiB cuts); re-stat whole files,
                      changed during read → retry once else skip; assert Σlen == S
  normalize.rs        (only if Spike A(c) failed) replace JSON-escaped `<claude_home>\projects\<enc>` prefix (all case forms)
                      with `{{SR_PROJECT_DIR}}` on save, expand on restore; hashing/chunking over normalized stream
  crypto.rs           age x25519 identity; zstd(3)→age chunk encrypt/decrypt; scrypt wrap of identity (logN ≥ 18,
                      max_work_factor on decrypt); keys via blake3::derive_key(ctx, identity_secret): names / mac / backup
  manifest.rs         ProjectManifest{v, remote, subpath, generation, files{rel → FileEntry{class, size, hash, chunks[{name,len}],
                      title?, saved_by, saved_at}}, mac}; mac = keyed blake3 over canonical JSON without mac;
                      read verifies mac + remote/subpath == expected key
  store_repo.rs       run_git: isolated env (Spike D), credential helper = own exe, CREATE_NO_WINDOW, timeout
                      (120 s; push 600 s) → kill tree (taskkill /T /F); ops: ensure_clone, remote_head (ls-remote),
                      fetch(depth 1), show(sha, path), prepare_worktree(sha) (reset --hard + clean -ffdx),
                      snapshot_commit() (add -A, write-tree, commit-tree), verify_tree(chunk names), push_lease(sha, expected)
  recovery.rs         under lock at startup: delete stale .git/*.lock, clean -ffdx; fsck --connectivity-only fails → re-clone;
                      sweep `*.sr-tmp` in claude_home/projects; prune backups (>14 d or >2 GB)
  live_sessions.rs    read claude_home/sessions/*.json → {sessionId} where pid alive AND process start == procStart;
                      parse failure → empty set + warning (fallback: mtime guard only)
  state.rs            per-file FileState + aggregate status (rules below)
  sync.rs             sync_project(key, mode: Auto|PushOnly|ForceLocal|ForceRemote, files: Option<Vec<rel>>, progress)
  base.rs             BaseState per key: {generation_seen, files{rel → {size, mtime, hash}}}; atomic write (temp+rename)
  backup.rs           encrypted backups (%LOCALAPPDATA%\…\backups\<ts>\<key>\<rel>.age) only when overwritten content is
                      not a prefix of the new content
  activity.rs         append-only activity.jsonl {ts, key, source: gui|hook, action, result, message}
  lock.rs             File::try_lock on app_dir\sync.lock; holder info {pid, op, started_at} written inside
  error.rs            thiserror enum → {code, params} (no UI text)
  logging.rs          file logger in app_local_data_dir\logs (rotating; never content/tokens) + panic hook for GUI, hook-save, hook-worker
```

### Storage layout (repo)
```
session-relay.json     plaintext marker {app:"session-relay", v:1}
.gitattributes               "* -text -diff"
keys/identity.age            identity secret, age-scrypt(passphrase)
p/<key_hash16>/manifest.age  ProjectManifest   (key_hash16 = keyed blake3(names_key, remote+"\n"+subpath)[..16])
p/<key_hash16>/c/<name32>.zst.age
```
No global index: project list = decrypt all `p/*/manifest.age` (small). Recipient public key never stored anywhere.

### State rules (L local snapshot, B base, R manifest entry)
- Quick equality: L==B if size+mtime match B (skip hashing); R==B if R.hash==B.hash.
- L==R (hash) → InSync.
- L only: LocalOnly (push). R only, no B: RemoteOnly (pull). R only, B present, R==B: **LocalDeleted** (Claude cleanup; not attention, no auto-pull). L present, B present, R absent, L==B: **RemoteDeleted** (drop base, keep file, no push).
- L==B, R≠B → RemoteAhead. L≠B, R==B → LocalAhead.
- Both changed — append: LocalAhead if every R chunk name == keyed hash of L range; RemoteAhead if L's sealed chunks equal R's first chunks and L's tail is prefix of the next R chunk (decrypt that 1 chunk); else Diverged. Whole: newer of L.mtime vs R.saved_at wins (LocalAhead/RemoteAhead), loser backed up.
- Manifest generation < base.generation_seen → error `rollback_detected` (no action).
- Aggregate: diverged > both (local-ahead + remote-ahead files) > local_ahead > remote_ahead > synced. LocalDeleted/RemoteDeleted never raise attention.

### sync_project algorithm (all under lock)
1. remote_head → fetch if changed → prepare_worktree(sha) → load manifest (verify mac, key, generation).
2. Snapshot local files (allowlist) → states.
3. Pull set (Auto/ForceRemote): RemoteAhead/RemoteOnly (+Diverged if ForceRemote, + `files` filter). Skip files whose session is live (`live_sessions`) → reason `session_open`. For each: decrypt → `<target>.sr-tmp` → verify hash → re-stat target unchanged since step 2 else skip `being_written` → backup if needed → rename → set mtime = saved_at → base[rel] = R.
4. Push set (Auto/PushOnly/ForceLocal): LocalAhead/LocalOnly (+Diverged if ForceLocal). Encrypt missing chunks via temp+rename, entries from the same snapshot pass; title = `ai-title.aiTitle` → `last-prompt.lastPrompt` → first user prompt (IDE tags stripped), ≤ 80 chars.
5. If push set empty → done (no manifest write). Else generation+1, write manifest.age, delete unreferenced chunks of this project, snapshot_commit, verify_tree, push_lease(expected = step-1 sha or empty). LeaseRejected → back to 1 (max 3). Success → base[rel] = pushed entry for pushed files only; generation_seen = new.
6. Files skipped/aborted keep old base. Append activity; emit progress.
Target dir = claude_home/projects/encode_dir(link.local_root + subpath); not linked → error `not_linked`.

## Related Code Files
- Create: `src-tauri/src/core/*.rs` (modules above), `src-tauri/tests/{sync_roundtrip,state_rules,crash_recovery}.rs`
- Modify: `src-tauri/Cargo.toml` (age, zstd, blake3, serde, serde_json[preserve_order], regex, thiserror, walkdir, chrono, windows-sys, tempfile[dev])

## Implementation Steps
1. paths + links + project_identity + file_set with unit tests (Vietnamese path, UNC reject, worktree `.git` file, duplicate key → Secondary).
2. snapshot + crypto + manifest (+ normalize if Spike A(c) failed) with tests: file without trailing newline, empty file, binary pdf, append stability (sealed chunks unchanged after append), mac tamper, remote/subpath mismatch.
3. store_repo + recovery + lock with tests on local bare repo (lease empty/stale, stale index.lock, partial chunk file cleaned, timeout kill).
4. live_sessions + state + base + backup with table-driven tests for every rule.
5. sync.rs + activity.
6. Integration: A save → B link+restore (different root, different claude_home) → B append + save → A RemoteAhead → A restore → bytes equal; concurrent A/B save → one LeaseRejected then succeeds; status refresh concurrent with save cannot corrupt (lock); unchanged save = no commit; LocalDeleted not pulled; restore skipped for live session; aborted restore then save does not push that file; rollback detected.

## Todo List
- [x] paths/links/identity/file_set + tests
- [x] snapshot/crypto/manifest (+normalize) + tests
- [x] store_repo/recovery/lock + tests
- [x] live_sessions/state/base/backup + tests
- [x] sync.rs + activity
- [x] integration suite green (`cargo test`)

## Success Criteria
- [x] `cargo test` green incl. all integration scenarios above (41 tests; lease-retry loop itself not exercised — no deterministic injection point)
- [ ] 180 MB session: save after 1 MB append uploads ≤ 1 chunk + manifest (verified at 12 MiB: ≤ 2 new chunk files); status of unchanged 1.4 GB tree < 1 s — **measure in Phase 4 on real data**
- [x] No chunk > 50 MB (32 MiB hard cap, tested with a 33 MiB line); `.meta.json` roundtrips byte-exact
- [ ] Token never in argv/env/`.git/config` — **verify in Phase 3** (credential helper does not exist yet)

## Review Log
- 2026-09-23 code review 6/10 → fixed: C1 whole-file last-writer-wins lost edits without backup (now base-aware, edit-time `modified_at`, loser always backed up, hook skips two-sided edits); H1 transcript pulled without prefix proof; H2 git could walk into a parent repo (GIT_DIR/GIT_WORK_TREE/ceiling); H3 read failure looked like an empty cloud (fail-closed `show`, vanished manifest = rollback); M1–M9, L1–L6, L8, L10–L14. Open: M10 (backups buffer whole file), L7 (long-path hash case), L9 (pushed files read twice).
- Re-verification (same reviewer, own repro crate): all C/H/M1 repros fixed and fail safe; sparse checkout keeps other projects in every tested path; score 8/10. Follow-ups fixed: N1 corrupt clone rebuilt on the next git failure (`Engine::with_store`, sync retry); N2 per-file pull/push errors skip only that file; N3 ForceLocal republishes cloud-deleted sessions only when explicit or the manifest vanished; pre-push guard `others_unchanged` (other projects byte-identical) removes reliance on per-version sparse behavior; git ≥ 2.35. Open Lows: N4 no in-app reset for corrupt links/base (Phase 4 "Xoá dữ liệu cục bộ"), N6 two git processes per chunk read, N7 lease-retry loop not exercised end to end.
- Tester found (and I verified) the marker gap: a machine with a different identity could push a parallel encrypted set → store marker `key_check` → `WrongIdentity`.

## Risk Assessment
- Undocumented `sessions/*.json` changes → live guard degrades to mtime guard + warning (not silent).
- Claude rewrites a jsonl → local-only change still pushes (R==B rule); both-changed rewrite → Diverged (safe).
- Spike F shows memory lives under git-root dir for subfolder cwd → memory files attributed to the root key (adjust links).

## Security Considerations
- Path allowlist on both save and restore; restore also refuses reparse points in any parent (`symlink_metadata`) and checks component-wise `starts_with` after canonicalize.
- Manifest mac + key binding + generation stop swap/replay; identity/keys only in memory + Credential Manager.
- Backups encrypted, bounded; logs contain no content/token.
