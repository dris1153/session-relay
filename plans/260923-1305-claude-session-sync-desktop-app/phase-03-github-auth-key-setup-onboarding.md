---
phase: 3
title: "GitHub App Auth, Key Setup & Onboarding"
status: pending
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

## Overview
First-run: git preflight → GitHub App device-flow login → storage repo (user creates private repo + installs app on it only) → passphrase create/unlock → machine name → workspace roots. Tokens + identity in Credential Manager (Local persistence).

## Key Insights
- GitHub App user token = only repos where app is installed ∩ user access; expires 8 h, refresh token 6 months; device-flow tokens refresh without client_secret.
- App cannot create the repo without broad Administration rights → user creates it via pre-filled link (one time per account); each new machine only logs in + unlocks.
- Logout cannot revoke server-side (needs secret) → delete local tokens and link to github.com/settings/apps/authorizations; 8 h expiry bounds exposure.
- Credential Manager is readable by any same-user process (incl. Claude-run shell commands) → honest threat model in README; Local persistence avoids roaming.

## Requirements
- Functional: git preflight (`git --version` ≥ 2.32 else blocking screen with download link); login/logout; token refresh (in `git-credential get` and API client); storage check states per IPC contract; create_key / unlock_key; settings (machine name default `COMPUTERNAME`, workspace roots, claude_home default `CLAUDE_CONFIG_DIR` else `%USERPROFILE%\.claude`, editable).
- Non-functional: device-flow polling honors `interval` and `slow_down` (+5 s), retries transient network errors and non-JSON (HTML) bodies (seen in Spike C); expired code → restart; every refresh returns a new refresh token → store the rotated one atomically; passphrase zxcvbn score ≥ 3 + confirm + checkbox "Tôi hiểu mất passphrase là mất toàn bộ dữ liệu"; unlock runs scrypt in `spawn_blocking`.

## Architecture
```
core/secrets.rs      keyring-core + windows-native-keyring-store, modifier persistence=Local (Spike C): "access-token", "refresh-token", "token-expiry", "age-identity"
core/github_api.rs   reqwest async: device_code(client_id), poll_token, refresh_token, get_user,
                     list_installations(), installation_repos(id), get_repo(owner, name)
                     client_id/slug: option_env!("SR_GITHUB_CLIENT_ID"/"SR_GITHUB_APP_SLUG") (build-time; forks set their own)
                     verification_uri must equal https://github.com/login/device before opening
core/settings.rs     %LOCALAPPDATA%\dev.sessionrelay.desktop\settings.json {repo{owner,name}, machine_name, claude_home,
                     workspace_roots[], links{}}; atomic write
core/key_setup.rs    check_storage(): installation? → repo accessible? → private && owner==login? → marker/empty? →
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
- Create: `src-tauri/src/core/{secrets,github_api,settings,key_setup}.rs`, `src-tauri/src/git_credential.rs`, `src-tauri/src/commands.rs`
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
- [ ] GitHub App registration doc + build env vars
- [ ] github_api (device flow, refresh, installations)
- [ ] secrets (Local persistence) + git-credential subcommand
- [ ] settings
- [ ] key_setup (states, create race, unlock)
- [ ] periodic private re-check
- [ ] i18n helper + vi/en locale files
- [ ] onboarding UI + startup routing
- [ ] logout + revoke link

## Success Criteria
- [ ] Fresh account: login → create repo link → install app → passphrase → dashboard, < 3 min
- [ ] Second machine: login → unlock → dashboard; wrong passphrase → error, no crash
- [ ] Two machines creating key simultaneously → loser lands on unlock, one identity in repo
- [ ] Token works for storage repo only (API call to another private repo → 404); refresh after 8 h transparent to git

## Risk Assessment
- `github.com/new` query params unsupported → show manual instructions (name + Private).
- User installs app on "All repositories" → still bounded by Contents permission; UI recommends "Only select".

## Security Considerations
- Refuse public/foreign repo; periodic re-check.
- Passphrase never stored; identity cached in Credential Manager (Local); README states same-user processes can read it.
- Opener allowed only for github.com URLs.
