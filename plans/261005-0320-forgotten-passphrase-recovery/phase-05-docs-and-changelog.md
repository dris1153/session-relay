---
phase: 5
title: "Docs and changelog"
status: completed
priority: P3
effort: "0.25d"
dependencies: [1, 2, 3, 4]
---

# Phase 5: Docs and changelog

## Overview
Bring the written docs in line with the shipped behaviour. No version bump here (release is a separate chore commit).

## Related Code Files
- Modify: `README.md`, `docs/system-architecture.md`, `docs/project-changelog.md`, `docs/development-roadmap.md`
- Create / Delete: none

## Implementation Steps
1. README: first run step 4 (recovery key shown once; keep it with the passphrase); Security model (second wrap in `keys/recovery.age`; "no key rotation" stays but says changing the passphrase is supported; start over replaces the store); Limitations ("losing both = losing the cloud data; Start over keeps your local sessions").
2. `system-architecture.md`: storage layout (`keys/recovery.age`), a "Key recovery" section (re-wrap, start over, what other machines see), IPC table rows for `create_key` (returns code), `recover_key`, `reset_store`, `change_passphrase`, `new_recovery_key`; error codes.
3. Changelog `Unreleased`: Added (recovery key, unlock with it, change passphrase, new recovery key, start over). Roadmap: a short "recovery" entry; Deferred: show whether a recovery key exists, partial-clone fetch for re-wrap.
4. Check README and docs for the old text "Nobody can recover it" (also `onboarding.passphrase.ack` in both locales, done in phase 1) and fix links.

## Success Criteria
- [ ] `grep -ri "no key rotation\|nobody can recover"` in README/docs/locales is consistent with the new behaviour.
- [ ] Docs describe the real command names and file paths (spot-check against code).

## Risk Assessment
- Docs drift if phase scope changes: do this last, from the merged code.
