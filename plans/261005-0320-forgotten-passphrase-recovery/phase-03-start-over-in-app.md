---
phase: 3
title: "Start over in the app"
status: completed
priority: P1
effort: "1d"
dependencies: [1]
---

# Phase 3: Start over in the app

## Context Links
- [plan.md](./plan.md) · Code: `engine/{key_setup,store_repo,storage_check,context,backup,paths}.rs`, `commands/{setup,dashboard}.rs`, `app_state.rs`, `watcher.rs`, `hook_worker.rs`, `use-dashboard.ts`, `onboarding-flow.tsx`

## Overview
"Forgot passphrase? Start over" on the unlock screen replaces the cloud store with a fresh one (new identity, new passphrase, new recovery key) in the same repository, then the user saves again from the sessions still on their machines. Destroys all cloud data: typed confirmation.

## Requirements
- Functional: allowed only when the storage state is `NeedsUnlock` (private, own repo, key present); never on `Foreign`/public/missing; the new snapshot contains only `session-relay.json`, `.gitattributes`, `keys/identity.age`, `keys/recovery.age`; the lease is the remote head read just before; other machines fall back to the unlock screen on their own.
- Non-functional: no download of the old store; hook workers cannot push with the old key during the swap.

## Architecture
```
reset_store(passphrase) -> recovery code               // engine/key_reset.rs, command in commands/key_recovery.rs
  check_strength; checked_repo(state) must be NeedsUnlock
  SyncLock (30 s); store.rebuild()            // drops the old clone and its objects
  head = store.remote_head()?  (None → Invalid: nothing to replace)
  prepare_worktree(None, ["/*"]); keys = Keys::generate(); code = RecoveryCode::generate()
  publish_key_files(store, expected = head, ...)  // phase 1 helper: orphan commit, push_lease(commit, head)
  clear base/ and backups/ (sealed with the old key, unreadable now); keep links.json, pending/
  remember_key(new keys)  → install_engine (clears dashboard + transcript caches)
```
Other machine B, old key cached: next fetch/sync → `check_identity` → `WrongIdentity` (`use-dashboard` already maps it to `onStorageProblem`) → `check_storage` → marker mismatch → `NeedsUnlock` + `storage-blocked` + engine dropped → unlock screen → `unlock_key` overwrites the cached identity. Old `base/<hash>.json` there are keyed by old hashes and are never read again.

UI: unlock mode → "Forgot passphrase? Start over" → `ConfirmDialog` variant with a text input: user types `owner/name` of the repo to enable the button → create form (`NewPassphraseFields`) → `reset_store` → recovery key screen (phase 1) → `machine`.

## Related Code Files
- Create: `engine/key_reset.rs`, `features/onboarding/step-start-over.tsx`
- Modify: `engine/mod.rs`, `engine/key_setup.rs` (make `publish_key_files` take `expected`), `commands/key_recovery.rs`, `lib.rs`, `step-passphrase.tsx` (link), `onboarding-flow.tsx` (route to the recovery key view), `tauri-commands.ts`, locales
- Delete: none

## Implementation Steps
1. `key_reset.rs`: `reset_store` as above plus `clear_key_bound_state(cfg)` (remove `base/`, `backups/` dirs). Unit-test `clear_key_bound_state`.
2. Command: acquire lock, set the same guards as `create_key`; on any error before the push nothing local changed; on `LeaseRejected` return it as is (UI: retry).
3. After success clear `storage-blocked` (`check_storage` path already does when ready; call it or `set_storage_blocked(false)`).
4. UI `step-start-over`: warning text lists what is lost (cloud copies; sessions Claude already deleted locally exist only in the cloud) and what is kept (sessions on this machine).
5. Locale keys: `onboarding.start_over.{link,title,warning,type_to_confirm,confirm}`; reuse `onboarding.recovery.*` for the new code.
6. Tests (`tests/key_reset.rs`): machines A and B on one bare remote with a real key and a synced project; A resets. Assert: remote root = marker + `.gitattributes` + `keys`; no `p/`; state `NeedsUnlock`; old passphrase and old recovery code fail; new passphrase and new code unlock to a different identity; A's local sessions untouched; B's engine → `WrongIdentity`; B re-created with the new keys (same `appdata`, stale `base/` present) syncs the project back without error; A refuses reset when state is `Foreign`; stale `expected` → `LeaseRejected`, remote untouched.
7. Manual (two real machines, after build): reset on one, watch the other drop to unlock, unlock, Save; confirm hook auto-save did not push between (check `activity.jsonl`).

## Implementation Notes (2026-10-05)
- Reordered after review: under `SyncLock` the command reads the remote head FIRST (`ls-remote`), then the key files (REST), then pushes with the head as lease, so a push by anyone in between gives `lease_rejected` instead of being overwritten (`reset_store(store, expected_head, files, passphrase)`). After the push: `remember_key`, clear `storage-blocked`, then best-effort cleanup of `base/` and `backups/` (a failure there is only logged, the recovery key is never lost).
- Typed confirmation (`owner/name`) is a field of one start-over card instead of a `ConfirmDialog`; `NewPassphraseFields` sits in the same card.
- `fs_util::remove_dir` moved from `commands/dashboard.rs`. Not covered by a test: the command-level guard (only `NeedsUnlock` may reset; `Ready` is refused by the command, the engine only checks that a key exists).

## Success Criteria
- [ ] After reset the repo holds exactly the four new files; remote head differs; local Claude files unchanged.
- [ ] Second machine recovers via unlock screen with the new passphrase, then Save works.
- [ ] Reset is impossible without the typed confirmation in the UI and on non-`NeedsUnlock` states in the engine.
- [ ] clippy, tests, tsc, build green.

## Risk Assessment
- Destructive by design: typed confirmation, lease on the current head, state guard; the old head stays reachable in GitHub only until its gc (unreadable anyway).
- Hook worker on B uses the old identity: `check_identity` fails before any write (WrongIdentity). `hook_worker` does not retry that error, so it clears the pending markers: those sessions are not auto-saved until B unlocks and the user presses Save (nothing is lost locally). Pre-existing behaviour, not changed.
- `rebuild()` removes the clone while a dashboard fetch might run: the `SyncLock` serialises; `watcher` `ls-remote` is read-only.
- Clearing `backups/` loses the only copy of files a past sync overwrote on this machine. They are sealed with a key nobody can open anymore (user chose this path); the warning does not mention them, so state it in the dialog.
