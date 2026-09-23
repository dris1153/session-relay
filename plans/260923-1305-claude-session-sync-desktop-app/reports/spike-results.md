# Phase 1 Spike Results

Date: 2026-09-23 · Rust 1.98.1 · Claude Code 2.1.280 (registry) / transcripts up to 2.1.278 · Git 2.54.0.windows.1 · Node 22.22.3

| Spike | Status | Decision |
|---|---|---|
| A(a) CLI `--resume` picker lists copied session | **PASS** (manual) | No cwd rewrite needed for listing |
| A(b) VS Code past conversations lists copied session | **PASS** (manual) | — |
| A(c) tool-results absolute path from other profile | **FAIL → fixed by rewrite** | Implement `normalize.rs` (mandatory) |
| B hooks — CLI (Stop/SessionEnd, stdin, survival) | **PASS with fixes** | Exec form (`command`+`args`), clear std-handle inheritance before spawn |
| B hooks — VS Code extension | **PASS (Stop) / SessionEnd NOT fired on window close** | Debounced Stop is the primary trigger; pending markers + GUI retry required |
| C keyring Local persistence | **PASS** | `keyring-core` + `windows-native-keyring-store`, modifier `persistence=Local` |
| C GitHub App device flow / refresh / scope | **PASS** | Refresh works without secret; rotate refresh token; retry transient network/HTML errors |
| C `github.com/new` prefill URL | PENDING (ask user) | — |
| D isolated git (local bare repo) | **PASS** | Env + lease design confirmed |
| D real push to github.com (schannel, credential helper) | **PASS** | 1 changed chunk → ~98 KiB pushed |
| E age+zstd throughput | **PASS** | 4 MiB chunks; expect ~3.3x compression (not 7x) |
| F encode rule | **PASS** | Exact rule below |
| F memory location | **RESOLVED (docs)** | Memory keyed by git repo root, not cwd |

## A(c) — tool-results path
- Copied 110-line prefix of a real session into a new encoded dir; rewrote `C:\\Users\\Dris\\.claude\\projects\\<encA>` → `C:\\Users\\PC\\…` to simulate machine A.
- `claude -p -c` resumed fine (conversation loads), but Read of persisted output path → **READ_FAILED** (path of other profile).
- Rewriting prefix to local `<claude_home>\\projects\\<encB>` → Read **OK**.
- Decision: on save replace JSON-escaped `<claude_home>\\projects\\<enc>` (case-insensitive drive/user) with `{{SR_PROJECT_DIR}}`; on restore expand to local. Hash/chunk over normalized stream. Only that prefix is rewritten (other `~/.claude` paths, e.g. skills, left as is).
- Also confirmed again: resuming a transcript whose `cwd` belongs to another machine works (`-c`).
- A(a) manual: interactive `claude --resume` in the new dir lists "Free public APIs project brainstorm" (title from `ai-title`), 846.7 KB, with raw `cwd` of the original machine.

## D — isolated git (local)
- Env: `GIT_CONFIG_NOSYSTEM=1`, `GIT_CONFIG_GLOBAL=NUL`, `GIT_TERMINAL_PROMPT=0`, `GCM_INTERACTIVE=never`, config via `GIT_CONFIG_COUNT` → `git config --show-origin` shows only command-line values (global/system ignored). `GIT_CONFIG_GLOBAL=NUL` works on Git for Windows.
- Orphan snapshot: `add -A` → `write-tree` → `commit-tree` (no parent) OK.
- `--force-with-lease=refs/heads/main:` (empty) → succeeds when ref absent; **rejected (stale info)** when ref exists.
- Stale lease (old sha) → rejected; correct lease → forced update OK.
- Changing 1 of 100 × 200 KB chunks: remote objects stay ≈ 20 MB (no re-upload of unchanged blobs).
- Must use fetch-based flow (`init` + `remote add` + `ls-remote` + `fetch --depth 1 origin main` + `reset --hard` + `clean -ffdx`), not `clone` (clone of repo whose HEAD ≠ main yields empty checkout).
- Local file remotes need `protocol.file.allow=always` (tests only) and `file:///` URL for `--depth`.

## F — Claude path encoding (exact)
```
enc = for each UTF-16 code unit of cwd: [A-Za-z0-9] ? unit : '-'
if enc.len() > 200: enc = enc[..200] + "-" + base36(abs(java_hash_code(cwd_raw)))
java_hash_code: h = 31*h + unit (i32 wrapping, over UTF-16 units of the raw path)
```
- `…\spikeF-Dự án.v2` → `…-spikeF-D---n-v2` (NFC input, one '-' per non-ASCII unit).
- 242-char path → 200 chars + `-9tdwhq` (matched `Math.abs(javaHash(raw)).toString(36)`).
- cwd > 260 chars: process cannot even start there (Windows MAX_PATH) → not a concern.

## F — memory location
Docs (code.claude.com/docs/en/memory): "`<project>` path is derived from the git repository, so all worktrees and subdirectories within the same repo share one auto memory directory. Outside a git repo, the project root is used." `autoMemoryDirectory` setting can relocate it.
- Decision: memory files belong to key `(remote, subpath="")` and live in `projects/encode(repo_root)/memory/`; session files belong to `(remote, subpath)` in `projects/encode(cwd)/`. If `autoMemoryDirectory` is set → skip memory sync with warning. (Phase 2 update.)

## B — hooks (CLI, Claude Code 2.1.280)
- Probe = GUI-subsystem Rust exe. Piped stdin readable (hook JSON ~850–1000 bytes, has `hook_event_name`).
- Stop fires in `claude -p`; SessionEnd fires on exit. Both workers (spawned `CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB`, breakaway allowed) survived 150 s after Claude exited.
- **Hang found:** without fixes Claude `-p` hung until killed (120 s): spawned worker inherited pipe handles → Claude waited for EOF. Shell forms (Git Bash / PowerShell string commands) leak extra inheritable handles from the shell.
- **Fix (verified, Claude exits in 6–8 s):** (1) register hooks in exec form `{"type":"command","command":"C:/…/session-relay.exe","args":["hook-save"]}` (no shell, no quoting); (2) in `hook-save`, `SetHandleInformation(GetStdHandle(STD_*), HANDLE_FLAG_INHERIT, 0)` for stdin/stdout/stderr before spawning; worker stdio = null.
- Docs: SessionEnd hooks share a 1.5 s budget (raised by per-hook timeout up to 60 s); default hook timeout 600 s; `shell` field ignored when `args` set.

## B — hooks (VS Code extension, manual)
- One chat turn in VS Code → `Stop` fired (exec form), stdin OK, worker spawned with breakaway.
- Window closed → **no `SessionEnd` event**; the Stop worker kept running and logged DONE 150 s later (survived window close).
- Conclusion: in VS Code (main surface of the user) `SessionEnd` cannot be relied on; debounced `Stop` + durable pending markers are required.

## C — keyring
- `keyring_core::set_default_store(windows_native_keyring_store::Store::new()?)`; `Entry::new_with_modifiers(service, user, &HashMap::from([("persistence","Local")]))`.
- 74-char identity + 40-char token roundtrip OK; `CredEnumerateW` shows target `LegacyGeneric:target=<user>.<service>`, Persist = 2 (LOCAL_MACHINE). Default (no modifier) = Enterprise (roams) → always pass modifier.
- Note: `CredEnumerateW` filter supports only prefix + `*`.

## C — GitHub App (real, app `session-relay-dev-dris1153`)
- Device flow OK (`/login/device/code` + poll). User token `ghu_…` expires_in 28800 s (8 h); refresh token lifetime 15 638 400 s (~181 days).
- **Refresh with `grant_type=refresh_token` WITHOUT client_secret → OK** (new 8 h token + new refresh token → must store rotated refresh token each time).
- `GET /user/installations` → `app_slug`, `repository_selection=selected`, installation repos = only `session-relay-spike` (private). App slug can be discovered at runtime from installations.
- Scope check: `/user/repos` listed 55 repos, but all except the storage repo are **public** (unauthenticated API 200); no other private repo of the user is visible → private access limited to the installed repo, as designed.
- Network robustness: polling hit `ENOTFOUND` and HTML (non-JSON) responses transiently → app must retry on network errors and non-JSON bodies during polling and API calls.
- `github.com/new?name=…&visibility=private` prefill: PENDING (ask user).

## D — real push to github.com
- Isolated env + `http.sslBackend=schannel` + `credential.helper=` reset then `!node <helper>` (helper prints `username=x-access-token` / `password=<token>`): `ls-remote` on private repo OK.
- First push with empty lease → `[new branch]`; re-push of same commit → "Everything up-to-date" (no-op, fine).
- Fetch depth 1 → reset → change 1 of 40 × 100 KB files → push transferred **5 objects, 97.98 KiB** (dedupe works over HTTPS).
- Stale lease → rejected. Token temp file deleted after run.
- User `url.insteadOf` not tested by mutating global config (not touched on purpose); covered by `GIT_CONFIG_GLOBAL=NUL` isolation proven locally.

## E — throughput (182 MB real transcript)
- 42 chunks × 4 MiB (line boundaries), zstd 3 + age x25519: **0.32 s (568 MB/s)**, output 54.4 MB (**3.3x**), decrypt+decompress roundtrip OK.
- Revised estimate: 1.4 GB local data → ~420 MB in repo (not ~200 MB). Per save of an active session ≈ one ≤ 4 MiB chunk ≈ ~1.3 MB upload.

## Cleanup
Scratch session dirs for spikes A/B/F removed from `~/.claude/projects/`. Remote test repo `dris1153/session-relay-spike` holds random test blobs only (keep as dev storage or delete).
