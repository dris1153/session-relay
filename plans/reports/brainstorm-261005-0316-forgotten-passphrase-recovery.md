# Brainstorm: forgotten passphrase recovery

Date: 2026-10-05 · Status: design agreed, no plan yet

## Problem
Passphrase forgotten, no way back. Current store (v0.2.1) cannot be recovered; user confirmed every machine asks for the passphrase.

Facts from code:
- Identity = random x25519 (`crypto.rs` `Keys::generate`). Data + names + MAC derive from it.
- `keys/identity.age` = identity wrapped by scrypt(passphrase), wf 18. zxcvbn >= 3 on create. No backdoor, brute force impractical.
- Unlocked machine caches identity in Credential Manager (`age-identity`, `secrets.rs`). `check_storage` = Ready only if cached identity matches store marker, else `NeedsUnlock`.
- No change-passphrase / recovery feature exists (grep clean). README "no key rotation" conflates rotate with re-wrap; re-wrap is cheap because identity is unchanged.
- `Sign out` runs `clear_all()` -> deletes cached identity. Never sign out while it is the only unlocked copy.

## Scenarios
| | Situation | Recoverable? |
|---|---|---|
| A | Forgot, >=1 machine still unlocked | Yes, fully: re-wrap identity with new passphrase, push `identity.age` |
| B | Forgot, no unlocked machine | Cloud data lost by design. Local originals remain in `~/.claude/projects`; only sessions Claude cleaned (30 d) and that exist only in the cloud are lost |

User is in B (store keeps asking). Unverified: Credential Manager not inspected (check denied); user to look for `session-relay` entries.

## Options evaluated
| Option | Verdict |
|---|---|
| Re-wrap on change passphrase (A) | Adopt. Reuses wrap/unwrap, no data re-encryption |
| Recovery key (random code, second wrap) | Adopt. Industry standard, user keeps the code |
| Start over in-app (new key, force new snapshot) | Adopt. Only exit for B; keeps GitHub App install (deleting repo would drop it) |
| Start over via delete/recreate repo only | Rejected: loses app installation, user must reinstall |
| Shamir / escrow server | Rejected: breaks no-server model, YAGNI |
| Brute-force helper in app | Rejected: manual `age -d` loop is enough if user remembers variants |
| BIP39 24 words | Rejected: wordlist + dep for single-user app |

## Agreed design
1. **Recovery key at key creation.** 160-bit random, base32 in groups, shown once with Copy + "I saved it" checkbox. Wrapped with existing `wrap_identity` into `keys/recovery.age` (no zxcvbn: already random).
2. **Unlock with recovery key.** Link in passphrase step. After success, force a new passphrase -> re-wrap `identity.age`. Other machines unaffected (identity unchanged).
3. **Settings: Change passphrase, Add recovery key** (unlocked only, scenario A and migration of stores that are still unlocked). Writes through the existing lease push + `others_unchanged` guard.
4. **"Forgot passphrase? Start over" in-app.** New key, new orphan snapshot (marker + `.gitattributes` + `identity.age` + `recovery.age`), lease against the current remote sha. Typed confirmation (destroys cloud data). Clears local `store/`, `base/`, backups (sealed with the old key), `storage-blocked`. Other machines see marker mismatch -> `NeedsUnlock` -> unlock with new passphrase, `remember_key` overwrites cached identity. Then Save from local sessions.

## Risks
- Machine B holding old `refs/session-relay/last` and `base/` after A resets: needs a two-machine test in `tests/support` (fetch of unrelated orphan history, rollback check, stale `base/<old hash>.json`).
- `links.json` (remote -> checkout, dir -> key) must survive reset; verify it holds no old-key hashes.
- Hook workers must not push with the old identity during reset (`storage-blocked` + `SyncLock`).
- Reset is destructive: guard with typed confirmation, lease on current sha, never on `Foreign` / public repo.
- Change passphrase does not invalidate the old passphrase for old commits still reachable on GitHub; real revocation = key rotation (out of scope).
- Existing store of the user cannot get a recovery key (needs unlock); new store will.

## Success criteria
- Tests (real git + crypto, two machines): recovery code unlocks and forces new passphrase; old passphrase then fails, new one works; reset on A -> B re-unlocks and syncs without errors; local sessions untouched; weak/typo recovery code rejected.
- Both locale files updated with identical keys; new error codes mapped in `errors.*`.
- `cargo clippy --all-targets -- -D warnings`, `cargo test`, `pnpm exec tsc --noEmit -p .`, `pnpm build` green.
- Docs: README (security model, "no key rotation" wording), `system-architecture.md` (storage layout, IPC), changelog.

## Next steps
1. User: do not sign out anywhere; optionally check Credential Manager for `session-relay` entries.
2. User today: no workaround inside the app. Manual reset = delete/recreate repo + reinstall the GitHub App, or wait for feature 4.
3. `/ck:plan` for the 4 parts; suggested order: 1 -> 2 -> 4 -> 3 (reset unblocks the user soonest after recovery key exists, so new store is protected).

## Unresolved questions
- Does any machine still hold a matching `age-identity`? (not verified)
- Exact content of `links.json` after key change (to confirm it survives reset).
- Where to surface the reset entry point if `check_storage` fails before onboarding reaches the passphrase step.
