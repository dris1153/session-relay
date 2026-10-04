---
phase: 1
title: "Recovery key at key creation"
status: pending
priority: P2
effort: "1d"
dependencies: []
---

# Phase 1: Recovery key at key creation

## Context Links
- [plan.md](./plan.md) · [brainstorm](../reports/brainstorm-261005-0316-forgotten-passphrase-recovery.md)
- Code: `src-tauri/src/engine/{key_setup,crypto,storage_check,error}.rs`, `commands/setup.rs`, `src/features/onboarding/{onboarding-flow,step-passphrase}.tsx`, `src/lib/tauri-commands.ts`, `tests/key_setup.rs`

## Overview
Creating the key also creates a recovery code, wrapped into `keys/recovery.age` in the same snapshot, and shown once in the UI before onboarding continues.

## Requirements
- Functional: `create_key` publishes `identity.age` + `recovery.age` atomically (one orphan commit); the UI shows the code once with Copy and an "I saved it" checkbox before the next step; `KeyFiles` carries `recovery` so later phases can read it.
- Non-functional: code from the OS CSPRNG; never logged, never in argv/env; shown only in memory (no persistence in settings or the clone).

## Architecture
```
create_key(store, passphrase)
  check_strength → recover → fetch → state must be NeedsNewKey
  keys = Keys::generate(); code = RecoveryCode::generate()
  publish_key_files(store, sha, &keys, passphrase, &code)   // shared by phase 3
      write marker, .gitattributes, keys/identity.age (wrap passphrase), keys/recovery.age (wrap code.canonical())
      snapshot_commit → push_lease(commit, sha)
  → (keys, code)
```
`RecoveryCode` (`engine/recovery_code.rs`): `generate()`, `Display` = grouped `ABCD-…`, `parse(&str)` (strip `-` and whitespace, uppercase, 32 chars of `A–Z2–7`, else `InvalidRecoveryCode`), `canonical()` = the 32 chars used as the scrypt "passphrase". Base32 hand-written (~25 lines).

IPC: `create_key(passphrase) -> String` (grouped code). `onboarding-flow` gets a `recovery_key` view between `passphrase` and `machine`; the key is already unlocked/remembered when it shows.

## Related Code Files
- Create: `src-tauri/src/engine/recovery_code.rs` (with unit tests), `src/components/recovery-key-display.tsx` (code, Copy, ack; reused in phase 4), `src/features/onboarding/step-recovery-key.tsx`
- Modify: `engine/mod.rs`, `engine/key_setup.rs` (`RECOVERY_FILE = "keys/recovery.age"`, `KeyFiles.recovery`, `publish_key_files`, `create_key` returns `(Keys, RecoveryCode)`), `engine/storage_check.rs` (`remote_key_files` reads `keys/recovery.age`), `engine/error.rs`, `Cargo.toml`, `commands/setup.rs` (return the code), `src/lib/tauri-commands.ts`, `onboarding-flow.tsx`, `step-passphrase.tsx` (`onDone(code?)`), `src/locales/{en,vi}.json`, `tests/key_setup.rs`
- Delete: none

## Implementation Steps
1. Add `getrandom`; write `recovery_code.rs` + unit tests (round trip, dash/space/lowercase tolerant parse, rejects wrong length and chars `0 1 8 9`, two codes differ).
2. Extract `publish_key_files` from `create_key`; add `RECOVERY_FILE`, `KeyFiles.recovery`; keep `store_key_state` logic unchanged (recovery presence does not affect state).
3. Add the two error variants and codes.
4. `remote_key_files`: read `keys/recovery.age` through the same `keys` dir listing (one extra REST call only when `keys` exists).
5. `create_key` command returns the code; update `tauri-commands.ts`.
6. UI: `recovery-key-display`, `step-recovery-key` (Continue disabled until the ack is checked); route `passphrase → recovery_key → machine` in `onboarding-flow`; update `onboarding.passphrase.ack` text (recovery key now exists).
7. Locale keys: `onboarding.recovery.{title,lead,copy,copied,ack,continue}`, `errors.invalid_recovery_code`, `errors.no_recovery_key` (both files, same keys).
8. Update `tests/key_setup.rs` call sites; add: created store has `keys/recovery.age`; it unwraps with the returned code to the same identity (`chunk_name` equal); a second `create_key` still gives `KeyExists` and leaves the head untouched.

## Success Criteria
- [ ] New store root has `session-relay.json`, `.gitattributes`, `keys/identity.age`, `keys/recovery.age`; passphrase and code both unwrap to the same identity.
- [ ] Onboarding cannot leave the recovery screen without the ack; the code is not in logs (grep logs after a manual run).
- [ ] clippy, tests, tsc, build green; locale files have identical key sets.

## Risk Assessment
- User skips the ack by closing the app: key exists, code never saved. Mitigation: Settings "new recovery key" (phase 4); the screen warns that it is shown once.
- Old stores (no `recovery.age`) must still unlock: `recovery` is `Option`, nothing requires it.
- `create_key` signature change breaks callers: only `commands/setup.rs` and `tests/key_setup.rs`.
