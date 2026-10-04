---
title: "Forgotten passphrase recovery"
description: "Recovery key, unlock with it, in-app start over, and change passphrase / new recovery key in Settings."
status: completed
priority: P2
effort: 3.5d
branch: main
tags: [feature, security, rust, onboarding, settings]
blockedBy: []
blocks: []
created: 2026-10-05
---

# Forgotten passphrase recovery

## Overview
Today a forgotten passphrase = lost cloud data, and the app has no way out (no change-passphrase, no reset). Identity (x25519) never changes; the passphrase only wraps it (`keys/identity.age`). So recovery = wrap the same identity a second way, or re-wrap it, or (last resort) start a new store.

Design + rejected options: [brainstorm report](../reports/brainstorm-261005-0316-forgotten-passphrase-recovery.md).

## Phases

| Phase | Name | Effort | Status |
|-------|------|--------|--------|
| 1 | [Recovery key at key creation](./phase-01-recovery-key-at-key-creation.md) | 1d | Complete |
| 2 | [Unlock with the recovery key](./phase-02-unlock-with-recovery-key.md) | 0.75d | Complete |
| 3 | [Start over in the app](./phase-03-start-over-in-app.md) | 1d | Complete |
| 4 | [Settings: change passphrase, new recovery key](./phase-04-settings-change-passphrase-and-recovery-key.md) | 0.5d | Complete |
| 5 | [Docs and changelog](./phase-05-docs-and-changelog.md) | 0.25d | Complete |

Sequential 1 → 2 → 3 → 4 → 5. Phase 3 is what unblocks a user who already forgot the passphrase; it comes after 1 because the new store it creates must get a recovery key.

## Key decisions
- Identity unchanged by recovery / change passphrase: no data re-encryption, other machines unaffected.
- Recovery code = 160-bit random, RFC 4648 base32 in groups of 4 (`ABCD-EFGH-…`, 8 groups), wrapped with the existing `wrap_identity` into `keys/recovery.age`. No zxcvbn (already random), no checksum (a typo fails age auth = "wrong recovery key").
- Start over: no download of the old store (`ls-remote` head as lease, `rebuild()` the clone, orphan snapshot with only marker, `.gitattributes`, `keys/*`). Keeps the GitHub App installation (deleting the repo would not).
- Change passphrase / new recovery key need no old secret: an unlocked machine already holds the identity.
- No store migration: a store nobody can unlock cannot get a recovery key. Unlocked stores add one from Settings (phase 4).

## Dependencies / overlap
- No overlap with open plans: `260923-…claude-session-sync-desktop-app` is only waiting on the Phase 7 E2E (no key code left in it). Not blocked.
- Interim, before phase 3 ships: the stuck user can delete the storage repo, recreate it (same name) and add it to the GitHub App installation. Loses the cloud copy; local sessions stay.
- New crate: `getrandom` (direct; already in `Cargo.lock` transitively, pick the version `cargo tree -i getrandom` shows for `age`).

## Cross-cutting rules
- Files < 200 lines (`key_setup.rs` is 134, `setup.rs` 173, `tauri-commands.ts` 186: new code goes in new files). Code and comments English; locale keys identical in `en.json` / `vi.json`; errors only as stable codes; tests on real git + crypto, no mocks.
- New error variants (`error.rs`): `InvalidRecoveryCode` → `invalid_recovery_code`, `NoRecoveryKey` → `no_recovery_key`. A well-formed but wrong code surfaces as `decrypt_failed` (UI shows "wrong recovery key" in that mode).
- Checks before each phase is done: `cargo clippy --all-targets -- -D warnings`, `cargo test`, `pnpm exec tsc --noEmit -p .`, `pnpm build`.

## Unresolved
- Confirm on a real GitHub repo that `git push --force-with-lease=refs/heads/main:<sha>` with an orphan commit and no local tracking ref is accepted (the tests use a local bare repo; `push_lease` already works that way, so low risk).
- Whether to show "recovery key exists" in Settings (needs a REST read of `keys/recovery.age`). Left out (YAGNI): Settings only offers "Create a new recovery key" (replaces the old).
