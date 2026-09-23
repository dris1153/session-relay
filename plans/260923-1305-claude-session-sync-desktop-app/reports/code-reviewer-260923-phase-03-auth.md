## Code Review Summary: Phase 3 (GitHub App auth, key setup, onboarding), adversarial

### Scope
- Rust: `engine/{secrets,github_api,auth,settings,key_setup,storage_check}.rs`, `{main,lib,app_paths,app_state,login,git_credential}.rs`, `commands/{mod,setup}.rs`, `capabilities/default.json`, `tauri.conf.json`, `build.rs`, `tests/key_setup.rs`
- Frontend: `src/lib/{i18n,tauri-commands}.ts`, `src/components/*`, `src/features/onboarding/*`, `src/app.tsx`, `src/locales/{vi,en}.json`
- About 1.9k LOC new or changed. Cross-checked against the Phase 2 engine (`store_repo`, `sync`, `publish`, `context`, `git_process`, `lock`).
- Verification:
  - `cargo clippy --all-targets -- -D warnings` shows 0 warnings.
  - `cargo test`: 47 passed, 1 ignored.
  - `npx tsc --noEmit -p .` shows 0 errors.
  - H1 was reproduced with scratch repos in the session scratchpad. No Credential Manager entries were touched, no GitHub calls were made, and the GUI was not run.

### Overall Assessment
Token hygiene is good:
- The token never reaches argv, env, `.git/config`, logs or UI copy.
- `verification_uri` is pinned.
- The opener scope is a glob limited to `https://github.com/*` and `https://git-scm.com/*`.
- The CSP is tight.
- Errors reach the UI as codes.
- Refresh runs under a cross-process lock with a re-check after the lock is taken.
- The create-key race resolves through the lease.

The weak spots are elsewhere:
- **Onboarding paths reuse the Phase 2 store without its crash hygiene.** A single killed fetch can block onboarding for good (H1).
- The storage check downloads the whole store under a 180 s cap (H2).
- Session lifecycle edges: logout vs refresh, 5xx during refresh, auth failures surfacing as git failures.
- A few UI state-machine holes.

**Score: 7/10.** H1 and H2 are localized fixes. With them plus M1–M6, the phase is 8.5+.

---

### Critical
None.

### High

**H1. Stale git lock files are never cleared on onboarding paths, so onboarding stays blocked for good (reproduced)**
- `storage_check.rs:69-71` and `commands/setup.rs:132-135` take `sync.lock`, then `ensure_clone` + `fetch`, but never call `store.clear_stale_locks()` or `recover()`. `sync_project` does call it (`sync.rs:53`).
- `maintenance::on_startup` needs an `Engine`. The engine is only installed after `check_storage` returns `Ready`, so nothing cleans up before unlock.
- Scenario:
  1. A second machine's first `fetch --depth 1` exceeds `NETWORK = 180 s` (`store_repo.rs:9,54`).
  2. `kill_tree` (taskkill /F) leaves `.git/shallow.lock` behind.
  3. Every later `check_storage` fails with `fatal: Unable to create '.../shallow.lock': File exists` (`git_failed`).
  4. The user is stuck on the "failed" card. The engine never gets installed, so the startup maintenance never runs either.
- The same happens to an already-set-up machine: `advance()` always routes through `check_storage`.
- Verified with Git 2.54 on scratch repos: a stale `shallow.lock` makes `fetch --depth 1` exit 128.
- Fix: call `store.clear_stale_locks()` right after acquiring `sync.lock` in both places, ideally `store.recover()`, the same as `sync_project`.

**H2. The storage check downloads the whole encrypted store under a hard 180 s timeout**
- `storage_check.rs:71` and `key_setup.rs:45,68` call `store.fetch()`. That is `fetch --depth 1`, which pulls every blob of the snapshot.
- The key state only needs `session-relay.json`, `keys/identity.age` and a root listing.
- With the Spike E estimate (~420 MB store), any link under ~2.3 MB/s times out. The killed fetch restarts from zero every time, and H1 then makes the failure permanent.
- This blocks the success criterion "Second machine: login → unlock → dashboard". During the check the UI shows only "Working…" or "Waiting for approval" (M5), with no progress.
- Fix:
  - Read those 2 blobs + the root listing via REST: `GET /repos/{o}/{r}/contents/...`. The token has Contents:read, and 404 on an empty repo maps to `NeedsNewKey`.
  - Alternative: `fetch --filter=blob:none` + targeted `cat-file`. Verify the promisor setup first.
  - Keep the full fetch for the Phase 4 dashboard, with progress and a stall-based timeout instead of an absolute one.

### Medium

**M1. Logout and login do not serialize with token refresh, so a signed-out machine can get its tokens back**
- `logout` (`setup.rs:88-95`) and the login poll's `save_tokens` (`login.rs:36`) do not take `auth.lock`.
- Scenario: a hook worker or the credential helper is inside `valid_token` (`auth.rs:22-29`) during a refresh when the user signs out. `clear_all` runs, then the refresher's `save_tokens(&fresh)` writes new tokens. The machine is signed in again, silently.
- The reverse also happens: a new login for account B can be overwritten by an in-flight refresh for account A.
- Fix: `logout` and the poll's save acquire `auth.lock` (via `acquire_within`) around `clear_all` / `save_tokens`. `valid_token` already holds the lock for the whole refresh, so this is enough.

**M2. Refresh failure modes force a re-login when they need not**
- `github_api.rs:43-46`: `TokenResponse` has only `Option` fields, so any 5xx JSON body parses as a final answer and is not retried. `refresh` then returns `Auth("refresh failed")`, the UI routes to login, and the credential helper fails.
- Fix: in `send`, `if status >= 500 { last = …; continue; }` before parsing.
- `auth.rs:28-29`: if `save_tokens` fails after a successful refresh, the rotated refresh token is lost (the old one is already consumed) and the fresh access token is discarded.
- Fix: retry the save once, log at error level, and return `fresh.access` anyway so the current operation succeeds. The next process will need a re-login, which is unavoidable at that point.

**M3. Auth failures inside git look like git corruption (cross-phase, triggered by Phase 3)**
- The credential helper exits 1 when there is no or a bad token (`git_credential.rs:29-32`). Git then fails with "could not read Username … terminal prompts disabled", which maps to `Error::Git`.
- `sync.rs:62-68` and `context.rs:62-67` treat `Error::Git` as a broken clone and call `rebuild()`, which deletes the store clone.
- Result: an expired 6-month refresh token or a revoked grant wipes the clone (a full re-download later), and the user sees "Git failed" instead of "sign in again".
- Fix: classify auth stderr in `GitEnv::check` / `push_lease` ("could not read Username", "Authentication failed", "terminal prompts disabled", HTTP 401/403) as `Error::Auth`, not `Error::Git`.

**M4. Device-flow polling keeps going for 15 min after a permanent error**
- `login.rs:43` logs and keeps polling for every `Err`.
- `poll_device` maps unknown errors to `Error::Auth(code)` (`github_api.rs:100`). That covers `incorrect_client_credentials`, `incorrect_device_code` (for example, a retried POST after the token was already issued), `unsupported_grant_type` and `device_flow_disabled`.
- The UI shows "Waiting…" until expiry.
- Fix: break with `reason: "auth_rejected"` on `Error::Auth`. Keep looping only on `Error::Network`.

**M5. Login step state bugs** (`step-github-login.tsx`, `onboarding-flow.tsx`)
- `step-github-login.tsx:18-21`: the effect depends on `[failure]`. When the same failure happens twice in a row (a code expires, the user takes a new one, that expires too), the prop stays `"expired"`, so the effect does not re-run. The expired code stays on screen with "Waiting for you to approve…" forever. Denied twice behaves the same.
- Fix: add a sequence number to the `login` view and use `key={seq}` on `StepGithubLogin`.
- `step-github-login.tsx:12,20`: `t(\`onboarding.login.${failure}\`)` only has keys for `expired|denied|not_logged_in|auth_rejected`. `login.rs:38` can emit `reason: "secrets"` (and M4 would add more), and those render as the raw key `onboarding.login.secrets`.
- Fix: fall back to `errorText(failure)` when `t(key) === key`.
- After approval, `auth-changed` triggers `advance()`, which runs `check_storage` (slow, see H2) while the login card still says "Waiting for approval", and "Get a new code" is still clickable.
- Fix: switch to the `loading` view or pass `busy` when `signed_in` is true.

**M6. Trust boundary: key commands trust `settings.repo`, and the engine outlives a failed check**
- `setup.rs:102-104` saves `check.repo` for every state, including `RepoPublic` and `RepoForeign`.
- `create_key`/`unlock_key` (`setup.rs:121-137`) use `state.repo()` without re-checking private + owner. Only the UI guards them. A `create_key` IPC call (or a repo made public between check and create) would push `keys/identity.age` into a public repo, which invites offline brute force.
- `setup.rs:105-108` never drops the engine when a later check returns `RepoPublic`, `NoInstallation` or `RepoMissing`. `identity_unlocked` stays true, and Phase 4/5 would keep syncing to a now-public repo. The plan says "public → block sync".
- Fix:
  - Save the repo only when it is private and owned by the user.
  - In `with_store_key`, re-validate with `GET /repos/{owner}/{name}`.
  - Call `drop_engine()` on any definitive non-`Ready` state.

**M7. A marker without an identity file counts as "new key", which can split the store**
- `key_setup.rs:28-29`: marker present + no `keys/identity.age` gives `NeedsNewKey`.
- `publish.rs:29-32`: the Phase 2 engine publishes into an empty remote with the marker but **without** `identity.age`.
- Scenario:
  1. The user deletes and re-creates `claude-sessions` to "reset".
  2. Machine A (engine installed) auto-saves, and the repo now holds marker + `p/*` with no identity file.
  3. Machine B sees `NeedsNewKey`, mints a new identity and overwrites the marker. It keeps A's `p/*` dirs, which B cannot read.
  4. Machine A's next sync hits `WrongIdentity`, and A's own startup check also shows "Create a passphrase".
- Fix:
  - `NeedsNewKey` only when the root holds nothing but marker/`.gitattributes`. Otherwise report a blocking state such as `repo_foreign`.
  - The engine should refuse to publish when the fetched snapshot lacks `keys/identity.age` (map it to a "store reset" code that sends the user to onboarding).

**M8. Repo lookup does not scale and assumes a single installation**
- `github_api.rs:151,171` use `per_page=100` and never paginate. With "All repositories" (a case the plan's risk section calls out) and more than 100 repos, `claude-sessions` can land on page 2 and the result is `RepoMissing` forever.
- `storage_check.rs:54` takes the first installation of the slug. That is fine for "Only on this account" apps, but wrong once an app is installed on an org too.
- Fix: after confirming an installation exists, call `GET /repos/{login}/{wanted}` (the plan's `get_repo`). 200 → check `private`/`owner`; 404 → `RepoMissing`. Optionally warn when `repository_selection == "all"`.

**M9. Startup needs the network: an offline start cannot reach the dashboard**
- `onboarding-flow.tsx:38` always runs `checkStorage` (GitHub API + ls-remote + fetch).
- The engine is installed only in that path (`setup.rs:105-108`).
- Offline, or with GitHub down, the result is the "failed" card, even with a cached identity and `settings.repo`. Plan step 8 intended routing from `get_app_state`.
- Fix (before Phase 4): install the engine from the cached identity + `settings.repo` at startup, run `check_storage` in the background, and block only on a definitive non-ready answer.

**M10. Plan and implementation drift**
- The todo "[x] logout + revoke link" is ticked, but the revoke link exists only in `docs/setup-github-app.md:46`. No UI surface or locale key has it.
- Fix: add `https://github.com/settings/apps/authorizations` to the ready/login card after sign-out. It is already inside the opener scope.
- The plan.md IPC contract is stale:
  - `start_login` returns no `interval`.
  - `get_app_state` returns `signed_in`/`has_client_id`/`git_version`, not `user`/`hooks`.
  - `check_storage` adds `user`/`repo`.
  - `passphrase_strength` is missing from the contract.
  - Red Team #14 was exactly about this kind of drift.
- The phase architecture still lists 3 token entries (`access-token`/`refresh-token`/`token-expiry`). The code stores one atomic JSON blob, `github-tokens`, which is better; update the plan text.

### Low
- **L1 Credential helper**
  - `git_credential.rs:20` checks the host but not `protocol=https`. That is defense in depth only, since `protocol.allow=never`.
  - `app_paths.rs:14` double-quotes the exe path for `sh`, so a `$` or backtick in the profile path gets expanded. Use single quotes with `'` → `'\''`.
- **L2 Hard-fail loads**
  - Unparsable `github-tokens` JSON (`secrets.rs:55`) makes `get_app_state` fail (`setup.rs:53`). The "failed" card has no sign-out button, so there is no recovery in the UI. Treat it as `None` + log.
  - A bad `settings.json` makes `lib.rs:13-18` call `exit(1)` with no UI at all.
- **L3 Input validation at the IPC boundary**
  - `save_settings` (`setup.rs:150-161`) accepts any `claude_home` (empty or relative) and any `language` string.
  - `i18n.ts:31` then throws on an unknown language (`dictionaries[current]` is undefined), which blanks the app.
  - Fix: validate in Rust (use an enum for language, require `claude_home` to be absolute and existing) and guard `setLanguage`.
- **L4 Error mapping**
  - A corrupt `identity.age` or a work factor above 22 shows "Wrong passphrase" (`crypto.rs:82`). Distinguish header/work-factor errors from a failed decrypt.
  - `marker_matches` treats a marker without `key_check` as a mismatch, but `Engine::check_identity` treats it as OK (`context.rs:54-57`). Pick one.
  - A 404 from `api()` maps to `network` ("Cannot reach GitHub").
- **L5** `onboarding-flow.tsx:30-49`: `advance()` has no generation guard, so the last resolver wins. Under StrictMode in dev, two concurrent `check_storage` calls contend for `sync.lock`: the second can return `busy` after 30 s and overwrite a good view.
- **L6 UI details**
  - Error strings are stored already translated (`step-github-login.tsx:12`, `step-passphrase.tsx:44-46`), so they do not follow a language switch.
  - The "failed" card is always titled "Internal error." (`onboarding-flow.tsx:66`).
  - Unlock mode uses `autoComplete="new-password"` (`step-passphrase.tsx:63`); it should be `current-password`.
  - The `clipboard.writeText`, `openUrl` and `passphraseStrength` rejections are unhandled.
  - The machine/roots step appears only right after the passphrase step. Closing the app there skips it for good; defaults apply.
- **L7 Accessibility**
  - Step dots are `aria-hidden` and there is no textual "Step 2 of 4".
  - The language buttons are labelled "vi"/"en" with no `aria-label` or group label.
  - The "Copied" feedback is not announced.
- **L8 DESIGN.md**
  - Serif is used for the step numerals (`step-storage-repo.tsx:38`). The serif is meant for headings only.
  - Spacing is off the 8px scale in several places: `gap-3`/`mt-3`/`py-3`/`px-3` (12px), `gap-5` (20px), `gap-1.5` (6px).
  - Buttons, weights (≤500), the card and Clay usage are all correct.
- **L9 YAGNI and hygiene**
  - `DeviceCode` derives `Debug` + `Serialize` while it carries `device_code` (`github_api.rs:52`), and neither derive is needed.
  - Unused locale keys: `common.continue`, `common.language`.
  - `avatar_url` and the CSP avatar host are unused for now.
  - `create_repo_url` is not URL-encoded (`storage_check.rs:52`).
  - There is a new reqwest client per call.
  - The untracked `pnpm-lock.yaml` sits next to `package-lock.json`. Pick one package manager.
- **L10 Tests**
  - The test "loser lands on unlock" covers sequential creation only. The `LeaseRejected → KeyExists` mapping (`key_setup.rs:59-61`), a marker-only store, `WrongIdentity` on unlock and an empty-store unlock are untested.
  - Credential-helper parsing is not unit-testable. Extract `fn answer(action, stdin) -> Option<String>`.

### Edge Cases Found by Scout
- A killed shallow fetch leaves `shallow.lock`, and onboarding never clears it (H1, reproduced).
- A second machine with a large store on a slow link: the fetch times out every time (H2).
- Logout or a new login racing a refresh in another process (M1).
- A 5xx JSON response during refresh; save failure after rotation (M2).
- An expired or revoked grant during a hook sync makes the clone rebuild (M3).
- The same device-flow failure twice in a row (M5).
- A repo made public after setup: the engine stays installed (M6).
- The repo is deleted and re-created while a machine is still set up (M7).
- "All repositories" installs with more than 100 repos (M8).
- An offline start (M9).

### Positive Observations
- The token never reaches argv, env or `.git/config`:
  - the helper is our own exe, reached via `GIT_CONFIG_*`;
  - `GIT_*`/`SSH_*` env is stripped, which blocks `GIT_TRACE_CURL`;
  - logs carry error codes only;
  - `Tokens` has no `Debug`.
- Refresh uses a cross-process lock with a double check, and the tokens are one blob, so the rotated pair is written atomically.
- Lock order is always `sync.lock` → `auth.lock`, so there is no deadlock with the helper.
- `verification_uri` is pinned. The opener is limited to a glob scope (github.com, git-scm.com; query strings match). `dialog:allow-open` only. The CSP has no remote script or connect sources.
- Key creation: a check before generating, a lease on push, `KeyExists` without any overwrite, and the identity is cached only after the push succeeds. A crash after the push recovers through unlock with the same passphrase.
- scrypt runs in `spawn_blocking`. zxcvbn is enforced server-side (it truncates input to 100 chars, so there is no DoS).
- The i18n helper is tiny, `vi`/`en` have identical key sets, and every UI string goes through keys.
- The global cursor rule is there and handles disabled states. Buttons are monochrome. The card, radii and serif h1 match DESIGN.md.

### Recommended Actions
1. H1: `clear_stale_locks()`/`recover()` in `storage_check::check` and `with_store_key`.
2. H2: read the key state via the REST contents API (or a blobless fetch). Move the full fetch to the dashboard with progress and a stall timeout.
3. M1 + M2: `auth.lock` around logout and the login save; retry 5xx; keep the fresh token when the save fails.
4. M3: map git auth stderr to `Error::Auth` so sync does not rebuild the clone.
5. M5 + M4: remount the login step per attempt; fall back for unknown reasons; stop polling on permanent errors.
6. M6 + M7: validate the repo at key commands; drop the engine on non-ready states; tighten the marker-only rule and make publish refuse a snapshot without `identity.age`.
7. M8, M9, M10 before Phase 4: `get_repo`, offline-capable startup routing, revoke link, and the IPC table update.

### Metrics
- Type coverage: TS strict, `tsc` shows 0 errors. The Rust build is clean.
- Tests: 47 Rust tests pass (Phase 3 adds 2 integration tests and 1 unit test). There are no frontend tests.
- Lint: clippy 0 warnings.
- Files: all under 200 lines (largest is `commands/setup.rs`, 165). All code literals are in English.

### Unresolved Questions
1. Was the manual E2E run with a **release** build? Release is GUI subsystem; debug is console. Please confirm that the credential helper's stdout reaches git in release, and test a profile path with non-ASCII characters (for example `C:\Users\Dũng`).
2. Does GitHub revoke the grant when a consumed refresh token is reused (the M2 retry after a lost response)? If so, a single lost response ends the session. That is acceptable, but it should be documented.
3. Should the machine/roots step be re-offered when the user never finished it (needs an `onboarded` flag), or are the defaults enough?
