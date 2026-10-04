---
phase: 2
title: "Unlock with the recovery key"
status: pending
priority: P2
effort: "0.75d"
dependencies: [1]
---

# Phase 2: Unlock with the recovery key

## Context Links
- [plan.md](./plan.md) · Code: `engine/{key_setup,crypto,publish,store_repo,lock}.rs`, `commands/setup.rs`, `step-passphrase.tsx`

## Overview
On the unlock screen, "I have a recovery key" takes the code plus a new passphrase, unwraps the identity from `keys/recovery.age`, re-wraps `keys/identity.age` with the new passphrase and unlocks. The old passphrase stops working; identity and data are unchanged.

## Requirements
- Functional: one IPC `recover_key(code, new_passphrase)`; nothing is remembered locally unless the re-wrap push succeeded; errors: `invalid_recovery_code`, `no_recovery_key` (store predates phase 1), `decrypt_failed` (wrong code), `weak_passphrase`, `wrong_identity`, `lease_rejected`.
- Non-functional: other machines keep working without any action (same identity); works with the REST-only `KeyFiles` for the unwrap, and a fetch only for the write.

## Architecture
```
recover_key(files, code, new_pass)                       // engine/key_rewrap.rs
  check_strength(new_pass); code = RecoveryCode::parse
  keys = unwrap_identity(files.recovery.ok_or(NoRecoveryKey), code.canonical())
  marker_matches(files, &keys) else WrongIdentity
  under SyncLock: rewrap_identity(store, &keys, new_pass)
rewrap_identity(store, keys, new_pass):                  // shared with phase 4
  recover → fetch → sha (None = StoreReset)
  state must be NeedsUnlock and marker matches
  prepare_worktree(sha, ["/session-relay.json", "/.gitattributes", "/keys/"])   // no p/ checkout
  write keys/identity.age = wrap_identity(keys, new_pass)
  commit = snapshot_commit; guard: root entries other than `keys` identical to sha's
  push_lease(commit, sha)
```
UI: `StepPassphrase` unlock mode gets a link to a new `StepRecoveryUnlock` (code field + `NewPassphraseFields`); success → `machine` step.

## Related Code Files
- Create: `engine/key_rewrap.rs`, `features/onboarding/new-passphrase-fields.tsx` (passphrase + strength meter + confirm + ack, extracted from `step-passphrase.tsx`; reused in phases 3 and 4), `features/onboarding/step-recovery-unlock.tsx`, `commands/key_recovery.rs` (`recover_key`; later phases add commands here)
- Modify: `engine/mod.rs`, `commands/mod.rs`, `commands/setup.rs` (`checked_repo`, `remember_key` → `pub(super)`), `lib.rs` (register command), `step-passphrase.tsx` (use the extracted fields, add the link), `tauri-commands.ts`, locales
- Delete: none

## Implementation Steps
1. `key_rewrap.rs`: `rewrap_identity` + the keys-only guard (compare root `ls_tree` entries minus `keys`, like `publish::others_unchanged`); unit-test the guard.
2. `recover_key` in the engine + command (acquire `SyncLock` 30 s like `create_key`; `remember_key` only after success).
3. Extract `NewPassphraseFields`; `StepPassphrase` behaviour must not change (same strength debounce, same ack).
4. `StepRecoveryUnlock`: code input accepts pasted text with dashes/spaces/lowercase; map `decrypt_failed` → `onboarding.recovery.wrong`.
5. Locale keys: `onboarding.passphrase.use_recovery`, `onboarding.recovery_unlock.{title,lead,field,submit}`, `onboarding.recovery.wrong`.
6. Tests (`tests/key_recovery.rs`, real git, `bare_remote` + `test_store`): create on A; B recovers with the code and a new passphrase; then the old passphrase → `Decrypt`, the new one unlocks to the same identity; `keys/recovery.age` unchanged; head moved and only `keys/` differs; wrong code → `Decrypt`; malformed → `InvalidRecoveryCode`; store without `recovery.age` → `NoRecoveryKey`; weak new passphrase rejected before any push.
7. Synced-data case: `Machine` with a real key (use `create_key`'s keys for `Machine::new`), sync a project, recover on another store handle, then the first machine still syncs (identity unchanged) and `p/` tree sha is identical before/after.

## Success Criteria
- [ ] Recovery works end to end in tests, including with synced project data untouched.
- [ ] A failed push (lease rejected) leaves no cached identity and no engine.
- [ ] clippy, tests, tsc, build green; locales equal.

## Risk Assessment
- `fetch --depth 1` downloads the whole store before the write (needed for the snapshot tree). Accepted: runs once, progress events already exist. A partial-clone filter is a later optimisation.
- Two machines racing a re-wrap: second gets `lease_rejected`; UI says retry (reuse existing `errors.lease_rejected`).
- Someone with the recovery code can change the passphrase: that is the recovery key's purpose; README says to store it like the passphrase.
