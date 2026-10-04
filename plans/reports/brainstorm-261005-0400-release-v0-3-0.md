# Brainstorm: release v0.3.0

Date: 2026-10-05 · Status: design approved, not executed

## Scope
v0.3.0 = passphrase recovery (recovery key, unlock with it, start over, change passphrase / new recovery key, git `NUL` fix). Minor bump: new feature, old stores still unlock by passphrase.

## Decisions (user)
- No manual smoke test before release (user choice). Risk accepted: UI and `Start over` never run on the real app / real GitHub; tests use local bare repos only.
- Published as normal Latest release (not pre-release). Advisor recommended pre-release as free insurance; declined.
- Publish via `gh` (installed by winget; user runs `gh auth login`, advisor never touches the token).

## Facts
- Past releases: `chore(release): x.y.z` commit (package.json, Cargo.toml, Cargo.lock, tauri.conf.json, changelog, roadmap) + tag `vX.Y.Z` + NSIS installer + `SHA256SUMS.txt`.
- `gh` not installed; `.env` missing (build.rs reads only `../.env`; `.env.local` holds both values, both gitignored).
- Installer unsigned: SmartScreen warns (same as before).
- `path_decode` unit test fails here only because TEMP is an 8.3 short path; use a long TEMP for the gate.

## Steps
1. Gate: clippy -D warnings, cargo test (long TEMP), tsc, pnpm build.
2. Release commit: bump 4 version files, changelog `[Unreleased]` -> `[0.3.0] - 2026-10-05`, roadmap entry.
3. Create `.env` from `.env.local`; `pnpm tauri build`; `SHA256SUMS.txt`; confirm no `SR_GITHUB_* is not set` warning.
4. Tag `v0.3.0`, push main + tag (ask right before).
5. `gh release create` with exe + sums, English notes (new features, keep the recovery key, Start over deletes the cloud copy, SmartScreen).
6. After: install over 0.2.1, try recovery flow on a throwaway repo.

## Risks
- Start over leased push untested on real GitHub; failure mode = lease_rejected/git error, no data loss beyond the intended reset.
- Latest release exposes untested UI to every user at once.
- Out of scope: code signing, CI, auto-update.

## Unresolved
- Whether the installer upgrade path (0.2.1 -> 0.3.0) keeps hooks and autostart (post-install hook exists; not re-verified).
