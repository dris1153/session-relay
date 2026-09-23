---
phase: 3
title: "GitHub App Auth, Key Setup & Onboarding"
status: completed
priority: P1
effort: "2.5d"
dependencies: [2]
---

# Phase 3: GitHub App Auth, Key Setup & Onboarding

## Context Links
- plan.md §IPC Contract; Red Team #4, #13, #15
- GitHub App user tokens: https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-a-user-access-token-for-a-github-app
- Refresh: https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/refreshing-user-access-tokens
- `reports/spike-results.md` Spike C

## Implementation Notes (2026-09-23)
- Engine modules: `secrets`, `github_api`, `auth` (refresh under `auth.lock`), `settings`, `key_setup` (store-level, tested with a bare repo), `storage_check` (GitHub API + store). Tauri: `app_state`, `app_paths` (identifier test), `login` (poll thread), `git_credential`, `commands/{mod,setup}`.
- `reqwest` blocking with `native-tls` (schannel) — avoids aws-lc build tooling.
- Opener allowlist: github.com and git-scm.com (Git download link on the preflight screen).
- Private/owner re-check runs on every app start (onboarding flow calls `check_storage`); the 24 h re-check while running moves to the Phase 4 watcher.
- Manual E2E by the user: fresh login → repo `claude-sessions` created → app installed on it → passphrase → ready. `.git/config` of the real clone holds no token. UX fix after feedback: steps stay busy until the next screen replaces them; no second storage round trip after key setup.
- After review: `check_storage` and `unlock_key` read only the root listing, the marker and `keys/identity.age` through the REST contents API (`?ref=main`, raw media type). A second machine unlocks without downloading the store; the full fetch starts with the Phase 4 dashboard. Only `create_key` uses git, on a store that is empty or holds only scaffolding.
- Key state is a pure function of `KeyFiles{root, marker, identity}`. Marker plus project data without a key is `repo_foreign`, never re-keyed. The engine refuses to publish into an empty remote (`store_reset`).
- Tokens are one JSON blob (`github-tokens`) in Credential Manager, written only under `auth.lock` (refresh, sign-in, sign-out).

## Overview
First-run: git preflight → GitHub App device-flow login → storage repo (user creates private repo + installs app on it only) → passphrase create/unlock → machine name → workspace roots. Tokens + identity in Credential Manager (Local persistence).

## Key Insights
- GitHub App user token = only repos where app is installed ∩ user access; expires 8 h, refresh token 6 months; device-flow tokens refresh without client_secret.
- App cannot create the repo without broad Administration rights → user creates it via pre-filled link (one time per account); each new machine only logs in + unlocks.
- Logout cannot revoke server-side (needs secret) → delete local tokens and link to github.com/settings/apps/authorizations; 8 h expiry bounds exposure.
- Credential Manager is readable by any same-user process (incl. Claude-run shell commands) → honest threat model in README; Local persistence avoids roaming.

## Requirements
- Functional: git preflight (`git --version` ≥ 2.35 else blocking screen with download link); login/logout; token refresh (in `git-credential get` and API client); storage check states per IPC contract; create_key / unlock_key; settings (machine name default `COMPUTERNAME`, workspace roots, claude_home default `CLAUDE_CONFIG_DIR` else `%USERPROFILE%\.claude`, editable).
- Non-functional: device-flow polling honors `interval` and `slow_down` (+5 s), retries transient network errors and non-JSON (HTML) bodies (seen in Spike C); expired code → restart; every refresh returns a new refresh token → store the rotated one atomically; passphrase zxcvbn score ≥ 3 + confirm + checkbox "Tôi hiểu mất passphrase là mất toàn bộ dữ liệu"; unlock runs scrypt in `spawn_blocking`.

## Architecture
```
engine/secrets.rs      keyring-core + windows-native-keyring-store, modifier persistence=Local (Spike C): "github-tokens" (JSON), "age-identity"
engine/github_api.rs   reqwest blocking: device_code(client_id), poll_token, refresh_token, get_user,
                     installations(), repo(owner, name), root_names(repo), file(repo, path)
                     client_id/slug: option_env!("SR_GITHUB_CLIENT_ID"/"SR_GITHUB_APP_SLUG") (build-time; forks set their own)
                     verification_uri must equal https://github.com/login/device before opening
engine/settings.rs     %LOCALAPPDATA%\dev.sessionrelay.desktop\settings.json {repo{owner,name}, machine_name, claude_home,
                     workspace_roots[], links{}}; atomic write
engine/key_setup.rs    check_storage(): installation? → repo accessible? → private && owner==login? → marker/empty? →
                     identity.age present? ; create_key / unlock_key
git_credential.rs    `session-relay.exe git-credential get`: read token (refresh if < 5 min left) → print
                     username=x-access-token / password=<token>; ignore store/erase
commands.rs          get_app_state, start_login, logout, check_storage, create_key, unlock_key, save_settings
UI src/features/onboarding/
  onboarding-flow.tsx          routes by AppState + StorageCheck
  step-git-missing.tsx
  step-github-login.tsx        big monospace user_code + copy, "Mở GitHub", waiting spinner
  step-storage-repo.tsx        per state: repo_missing → "Tạo repo private" (github.com/new?name=claude-sessions&visibility=private);
                               no_installation → "Cài app vào repo" (github.com/apps/<slug>/installations/new, pick only that repo);
                               repo_public / repo_foreign → blocking message; "Kiểm tra lại"
  step-passphrase.tsx          create (2 fields + strength meter + checkbox) | unlock (1 field)
  step-machine-and-roots.tsx   machine name + claude_home + workspace roots (dialog)
```

## Related Code Files
- Create: `src-tauri/src/engine/{secrets,github_api,settings,key_setup}.rs`, `src-tauri/src/git_credential.rs`, `src-tauri/src/commands.rs`
- Create: `src/features/onboarding/*.tsx`, `src/lib/tauri-commands.ts` (typed wrappers mirroring IPC contract), `src/lib/i18n.ts`, `src/locales/{vi,en}.json`
- Modify: `src-tauri/src/main.rs` (dispatch `git-credential`), `src-tauri/src/lib.rs`, `src-tauri/capabilities/default.json` (**replace** `opener:default` with an `opener:allow-open-url` scope limited to `https://github.com/*` — permissions are additive; `dialog:allow-open`)
- Create: `docs/setup-github-app.md`

## Implementation Steps
1. Doc + register GitHub App: name, homepage, no callback needed, "Enable Device Flow", "Expire user authorization tokens" on, permissions Contents RW + Metadata R, installable "Only on this account".
2. `github_api.rs` device flow + refresh + installation/repo endpoints (`User-Agent`, `Accept: application/vnd.github+json`, `X-GitHub-Api-Version`).
3. `secrets.rs` per Spike C; `git_credential.rs`.
4. `key_setup.rs`: states; repo non-empty without marker → `repo_foreign`; create_key: fresh fetch → if identity exists → switch to unlock; else identity gen → wrap → write marker + `.gitattributes` + `keys/identity.age` → push lease empty → LeaseRejected → refetch → identity now present → return `needs_unlock` (no overwrite) → on success cache identity.
5. Re-check repo private + owner at app start and every 24 h (cached); public → block sync with message.
6. Create `src/lib/i18n.ts` (~20-line `t(key, params)`, no library) + `src/locales/{vi,en}.json`; every onboarding string via keys (Vietnamese copy in this plan = `vi` values).
7. Onboarding UI per DESIGN.md (parchment canvas, white 24 px card, serif 30 px step title, Ashen helper, dark filled primary, Clay step dots).
8. Startup routing from `get_app_state` (no event race): git missing → no token → storage not ready → identity locked → dashboard.
9. Logout: delete tokens + identity; show revoke link.

## Todo List
- [x] GitHub App registration doc + build env vars
- [x] github_api (device flow, refresh, installations)
- [x] secrets (Local persistence) + git-credential subcommand
- [x] settings
- [x] key_setup (states, create race, unlock)
- [ ] periodic private re-check (on start: done; every 24 h while running → Phase 4 watcher)
- [x] i18n helper + vi/en locale files
- [x] onboarding UI + startup routing
- [x] logout
- [ ] revoke link in the UI → Phase 4 settings page (docs only for now)

## Success Criteria
- [x] Fresh account: login → create repo link → install app → passphrase → ready (user E2E, 2026-09-23)
- [ ] Second machine: login → unlock → dashboard; wrong passphrase → error, no crash (store level covered by tests/key_setup.rs; real second machine pending)
- [x] Two machines creating key simultaneously → loser gets KeyExists → unlock, one identity in repo (tests/key_setup.rs)
- [x] Token scoped to the storage repo (Spike C); refresh without secret (Spike C). [ ] 8 h refresh transparent to git — verify when a token expires during use

## Risk Assessment
- `github.com/new` query params unsupported → show manual instructions (name + Private).
- User installs app on "All repositories" → still bounded by Contents permission; UI recommends "Only select".

## Security Considerations
- Refuse public/foreign repo; periodic re-check.
- Passphrase never stored; identity cached in Credential Manager (Local); README states same-user processes can read it.
- Opener allowed only for github.com URLs.

## Review Log (2026-09-23)
Code review 7/10 (`reports/code-reviewer-260923-phase-03-auth.md`), tester: no bugs (`reports/tester-260923-phase-03-auth.md`). Fixed:
- H1/H2: onboarding no longer clones or fetches: REST reads only; `create_key` runs `recover()` (stale git locks) under `sync.lock`.
- M1: sign-in and sign-out take `auth.lock`. M2: 5xx is retried; a refreshed token is used even if storing it fails.
- M3: git credential refusals map to `auth_rejected`, so they no longer rebuild the clone.
- M4: polling stops on permanent device-flow errors. M5: the login step remounts per attempt, unknown reasons fall back to error text, and approval shows the loading view.
- M6: key commands re-run the storage check; the repo is saved only once usable; the engine is dropped on any non-ready state.
- M7: marker + data without key → `repo_foreign`; publish into an empty remote → `store_reset`. M8: `GET /repos/{login}/{name}` replaces the paginated listing.
- M10: IPC table and token storage updated; revoke link stays a Phase 4 todo.
- Lows: helper checks `protocol=https`, single-quoted exe path; unreadable tokens = signed out; broken settings fall back to defaults; settings validated; stale `advance()` results ignored; errors stored as codes (re-render on language switch); unlock uses `current-password`; screen-reader step text and language group label; serif step numerals removed.
- Deferred: M9 offline start (install the engine from the cached identity, check in the background) → Phase 4. L4 corrupt identity vs wrong passphrase share one message. `LeaseRejected → KeyExists` race stays untested (needs an injected push between fetch and push).
