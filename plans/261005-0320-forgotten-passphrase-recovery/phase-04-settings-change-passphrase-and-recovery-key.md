---
phase: 4
title: "Settings: change passphrase, new recovery key"
status: completed
priority: P3
effort: "0.5d"
dependencies: [1, 2]
---

# Phase 4: Settings: change passphrase, new recovery key

## Context Links
- [plan.md](./plan.md) · Code: `engine/key_rewrap.rs` (phase 2), `commands/key_recovery.rs`, `features/settings/settings-page.tsx` (123 lines), `components/recovery-key-display.tsx` (phase 1), `new-passphrase-fields.tsx` (phase 2)

## Overview
A machine that is unlocked (scenario A: forgot the passphrase but one machine still works) can set a new passphrase, and can create a new recovery key (also the way for stores created before this feature to get one). No old secret is asked: the unlocked machine holds the identity already.

## Requirements
- Functional: `change_passphrase(new)` re-wraps `keys/identity.age`; `new_recovery_key()` writes a fresh `keys/recovery.age` and returns the code once (the previous code stops working; the UI says so); both require an unlocked engine and a ready store.
- Non-functional: other machines unaffected; same lease/guard path as phase 2.

## Architecture
Generalise `rewrap_identity` from phase 2 into `rewrite_key_files(store, keys, KeyChange { identity: Option<passphrase>, recovery: Option<&RecoveryCode> })`; recover (phase 2) = identity only, change passphrase = identity only, new recovery key = recovery only. Commands take `state.engine()` keys, `state.store_for(&state.repo()?)`, `SyncLock`.

UI: new `features/settings/key-settings.tsx` section "Encryption" (the page stays < 200 lines): "Change passphrase" opens a dialog with `NewPassphraseFields`; "Create a new recovery key" shows a confirm (old one stops working) then `RecoveryKeyDisplay`.

## Related Code Files
- Create: `features/settings/key-settings.tsx`
- Modify: `engine/key_rewrap.rs`, `commands/key_recovery.rs`, `lib.rs`, `settings-page.tsx` (render the section), `tauri-commands.ts`, locales (`settings.encryption`, `settings.change_passphrase*`, `settings.new_recovery*`)
- Delete: none

## Implementation Steps
1. Generalise the rewrite helper; keep phase 2 tests green.
2. Add the two commands; return `not_logged_in`/`invalid_data` when no engine.
3. Settings section + dialogs; reuse `RecoveryKeyDisplay`.
4. Tests (`tests/key_recovery.rs`): unlocked machine A changes the passphrase → fresh store handle unlocks with the new one only; synced project still readable on B (same identity) and `p/` tree unchanged; `new_recovery_key` → new code unlocks, old code → `Decrypt`, passphrase still works; weak passphrase rejected with no push.

## Implementation Notes (2026-10-05)
- Change passphrase is an inline panel (not a dialog); replacing the recovery key uses `ConfirmDialog`. Both commands use `engine.repo` / `engine.keys` under the engine's `sync.lock`.
- After review: the new recovery key is held in a module variable until the user ticks "I saved it", so leaving Settings while the command runs cannot lose a key that is already the only valid one.
- Not done (low): rebuild the clone on a git error (`engine.with_store`), a test that runs the rewrite on the engine's own sparse clone, a test for lease rejection while changing keys.

## Success Criteria
- [ ] Both actions work against a store holding synced data, with `p/` byte-identical afterwards.
- [ ] Settings page and every new file < 200 lines; locales equal; clippy, tests, tsc, build green.

## Risk Assessment
- Anyone at an unlocked machine can change the passphrase: they could already read the identity from Credential Manager (README threat model). Documented, not mitigated.
- Replacing the recovery key by mistake invalidates a code the user saved: the confirm dialog says so.
