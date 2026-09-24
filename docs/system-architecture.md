# System Architecture

## Components

```
React UI (src/)                    Tauri app layer (src-tauri/src/)             Engine (src-tauri/src/engine/)
┌──────────────────────────┐  IPC  ┌───────────────────────────────┐        ┌──────────────────────────────┐
│ onboarding · dashboard · │ ────▶ │ commands/{setup,settings,     │ ─────▶ │ sync · overview · evaluate · │
│ settings                 │ ◀──── │  dashboard,workspace}         │        │ state · manifest · crypto ·  │
│ events: projects-changed,│       │ dashboard (view cache) ·      │        │ transfer · publish ·         │
│ sync-progress,           │       │ watcher · tray · autostart ·  │        │ store_repo · git_process ·   │
│ auth-changed,            │       │ auto_save                     │        │ auth · key_setup ·           │
│ storage-changed          │       └───────────────────────────────┘        │ storage_check · claude_hook_ │
└──────────────────────────┘                                                │ config · workspace_scan ·    │
                                   same exe, headless subcommands:          │ git_clone …                  │
                                   git-credential · hook-save ·             └──────────────────────────────┘
                                   hook-worker · hook-uninstall
```

`main.rs` dispatches the headless subcommands before Tauri starts, so the credential helper and the hook never open a window.

## Storage repository

A private GitHub repository owned by the user. Its `main` branch always holds **one orphan snapshot commit**; every save builds a new snapshot and pushes it with `--force-with-lease` (compare-and-swap). The local clone is only a cache.

```
session-relay.json          plaintext marker: app, version, key_check (MAC proving which identity owns the store)
.gitattributes              * -text -diff
keys/identity.age           age identity, passphrase-wrapped (scrypt)
p/<project hash>/           one folder per project; the name is a keyed blake3 hash of (remote, subpath)
  manifest.age              sealed manifest (zstd + age, MAC over the body)
  c/<chunk hash>.zst.age    content chunks (zstd, then age); names are keyed hashes of the plaintext
```

Manifest (per project): `{ v, remote, subpath, generation, files: { rel → { class, size, hash, chunks[{name,len}], title?, saved_by, saved_at, modified_at? } } }`. `generation` grows on every push; a machine that has seen a higher generation refuses an older snapshot (`rollback_detected`).

Sealed with the user's x25519 identity; keyed hashes use keys derived from it, so the repository reveals only sizes, counts and timing.

## Local state (`%LOCALAPPDATA%\dev.sessionrelay.desktop\`)

| Item | Purpose |
|---|---|
| `store/` | Clone of the storage repo (sparse checkout of one project while saving). `refs/session-relay/last` = newest snapshot this clone fetched or pushed; statuses are computed against it, also offline |
| `base/<project hash>.json` | Per file, what was last in sync (length, mtime, keyed hash) + highest generation seen |
| `links.json` | Remote → local checkout root; Claude project dir → project key |
| `backups/` | Encrypted copies of anything a sync overwrote (pruned by age and total size) |
| `pending/<dir>/<stamp>` | Auto-save markers, one file per Claude turn |
| `storage-blocked` | Present while the storage repo is unusable (public, foreign, re-keyed): hook workers do not save |
| `activity.jsonl`, `logs/` | Outcome per sync (source gui/hook); app logs (error codes, no content) |
| `sync.lock`, `auth.lock` | Cross-process file locks |

## Project identity

A project is `(remote, subpath)`: the normalized GitHub `origin` of the checkout (`github.com/owner/repo`) plus the folder Claude ran in relative to the checkout root. A Claude project dir is mapped to it once (from the `cwd` in its transcripts) and recorded in `links.json`; a second checkout of the same repo is `Secondary` and never synced. Claude's dir name encoding: every UTF-16 unit outside `[A-Za-z0-9]` becomes `-`; names over 200 chars are cut and suffixed with a base36 hash.

## Sync

### File classes and chunking
- Synced files: `<session>.jsonl`, `<session>/subagents/agent-*.{jsonl,meta.json}`, `<session>/tool-results/*`, `memory/*.md` (root project only).
- Append files (transcripts) are cut at line boundaries near 4 MiB (hard cut at 32 MiB), so a growing transcript re-uploads only its last chunk. Whole files are cut every 32 MiB.
- Absolute paths of this machine's Claude project dir inside the content are replaced by a placeholder (`\x01SR_PROJECT_DIR\x01`) on save and expanded to the local dir on restore.

### Three-way state per file (`state.rs`)

Local (size, mtime, content hash) vs base vs remote (manifest entry):

| State | Meaning |
|---|---|
| `in_sync` | local = remote |
| `local_only` / `remote_only` | exists on one side only, never synced |
| `local_ahead` / `remote_ahead` | one side changed since base. A transcript is only `remote_ahead` after proving the cloud copy extends the local one (its last chunk starts with the local tail) |
| `diverged` | both changed a transcript and neither extends the other |
| `local_deleted` | Claude deleted it here (cleanup), cloud unchanged: not resurrected unless the user restores it |
| `remote_deleted` | deleted from the cloud on purpose, unchanged here: not re-uploaded |

Whole files changed on both sides use last-writer-wins on `modified_at` (`conflict` flag, loser backed up). Metadata decides without reading content whenever base allows.

Project status (`evaluate::aggregate`): any `diverged` → `diverged`; push and pull work → `both`; push only → `local_ahead`; pull only → `remote_ahead`; else `synced`. `not_linked` = in the cloud, no checkout linked here.

### A sync (`sync::sync_project`)
Under `sync.lock`: fetch → check marker identity → read manifest → rollback check → evaluate → pull files (skipped while their Claude session is live; local copy backed up first when needed) → push: sparse worktree of the project, chunks + sealed manifest (`generation + 1`), orphan commit, `others_unchanged` guard (no other path of the snapshot may change), lease push. A lease rejection (another machine pushed) retries up to 3 times; a git error rebuilds the clone once.

Modes: `auto` (both directions, skip diverged), `push_only` (hook worker: never pulls, skips conflicts), `force_local` / `force_remote` (optionally per file; the overwritten side is backed up).

## Git

- `GitEnv` runs git with `GIT_CONFIG_NOSYSTEM`, `GIT_CONFIG_GLOBAL=NUL`, no hooks, https only, `LC_ALL=C`, our exe as the only credential helper (`session-relay.exe git-credential get`, answers only `github.com` over https), pinned `GIT_DIR`, and timeouts. Transfers use `--progress` and are killed only after 120 s without output.
- Errors are classified from stderr: credentials refused → `auth_rejected`, unreachable → `network`, everything else → `git_failed`. Only `git_failed` may rebuild the clone.
- Manifests of a snapshot are read with one `git cat-file --batch`.
- `git_clone.rs` is the exception: it clones a project repo with the user's own git configuration and credentials.

## GitHub App auth

Device flow (`start_login`, polled in a thread, `auth-changed` event). User tokens expire after 8 h and refresh without a client secret; the refresh token rotates, so refresh, sign-in and sign-out run under `auth.lock`. Tokens (one JSON entry) and the unlocked identity live in Windows Credential Manager with Local persistence. The storage check uses the REST API only (installation, `GET /repos/{login}/{name}`, root listing, marker and `keys/identity.age` contents), so a new machine unlocks without downloading the store.

## Dashboard pipeline

```
open window ─▶ local_projects (no network, last snapshot on disk) ─▶ list shown
            └▶ list_projects(fetch=true): wait lock [Waiting] ─▶ fetch [Checking, Download %]
               ─▶ manifests (cached per snapshot) ─▶ evaluate projects in parallel (≤4) [Evaluate i/n]
               ─▶ view cached + tray attention updated ─▶ projects-changed
watcher (60 s; 5 min offline): ls-remote; refresh when the remote head, the clone's last snapshot
         or the activity file changed; finishes orphaned auto-save markers; weekly gc when idle;
         storage re-check every 24 h
```

## Auto-save (hooks)

```
Claude turn ends ─▶ session-relay.exe hook-save   (Stop / SessionEnd, exec form, timeout 10 s)
   reads stdin (≤16 MB, rest drained) ─▶ transcript must resolve inside <claude_home>/projects
   ─▶ pending/<dir>/<stamp> ─▶ CreateProcessW(hook-worker, no inherited handles, new process
   group, breakaway from Claude's job) ─▶ exit 0, no stdout
hook-worker: sleep 120 s (SessionEnd 5 s) ─▶ exit if a newer marker exists (unless the oldest
   is older than 10 min) ─▶ exit if storage-blocked ─▶ classify dir ─▶ sync push_only
   ─▶ clear markers ≤ stamp (kept on busy/network/auth/io errors)
GUI: markers older than 15 min are finished by the watcher (with backoff); Quit finishes all (≤30 s)
```

Hooks are registered in `<claude_home>/settings.json` (both events, one group each, backups kept, malformed files never written); the switch reads that file as the source of truth. The NSIS pre-uninstall hook runs `hook-uninstall`.

## IPC contract

| Command | Returns |
|---|---|
| `get_app_state` | `AppState { git_version?, git_ok, has_client_id, signed_in, identity_unlocked, repo?, machine_name, claude_home, workspace_roots, language?, autostart, hooks, claude_untested? }` |
| `start_login` / `logout` | `LoginCode { user_code, verification_uri, expires_in }` / () |
| `check_storage` | `StorageCheck { state: no_installation\|repo_missing\|repo_public\|repo_foreign\|needs_new_key\|needs_unlock\|ready, user, repo?, install_url, create_repo_url }` |
| `passphrase_strength(passphrase)` | 0–4 |
| `create_key(passphrase)` / `unlock_key(passphrase)` | () |
| `save_settings(patch)` · `set_auto_save(enabled)` | `AppState` |
| `list_projects(fetch)` · `local_projects` | `Dashboard { projects, unmanaged, secondary, errors, offline, fetched_at?, auto_save_failures }` · same or null before any snapshot |
| `project_activity(key_hash)` | last 20 `Activity { ts, key_hash, source, action, result, pushed, pulled }` |
| `sync_project(key_hash, mode, files?)` | `SyncReport { pushed, pulled, skipped: [rel, reason][] }` |
| `save_all` | `SaveAllItem { key_hash, report?, error? }[]` |
| `link_project(key_hash, local_root, force)` | `{ origin_matches, found_remote? }` |
| `delete_remote_session(key_hash, session_id)` · `open_project_folder(key_hash)` · `clear_local_data` | () |
| `scan_workspaces` · `clone_project(key_hash, root)` · `cancel_clone` | `Checkout { remote, path }[]` · destination path · () |

`ProjectView { key_hash, remote, owner, name, subpath, status, local_root?, files: FileRow[], unreadable }`; `FileRow { rel, state, conflict, title?, size?, saved_by?, saved_at?, local_size?, local_modified? }`.

Events: `projects-changed(Dashboard)`, `sync-progress { step: waiting|checking|download|upload|clone (percent)|restore|save|evaluate (current, total) }`, `auth-changed { signed_in, reason? }`, `storage-changed`.

Errors: `{ code, detail }`; `code` is stable (`engine::error::Error::code`), `detail` is English for logs. The UI maps codes through `errors.<code>` in the locale files.
