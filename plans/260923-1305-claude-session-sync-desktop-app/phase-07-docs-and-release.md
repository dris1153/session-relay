---
phase: 7
title: "Docs & Release"
status: pending
priority: P2
effort: "1d"
dependencies: [6]
---

# Phase 7: Docs & Release

## Context Links
- plan.md; Red Team #4, #13, #14, #15
- User rules: docs in `./docs` (roadmap, changelog, architecture, code standards)

## Overview
Honest README (unofficial, threat model), project docs, supported-version note, installer build with checksums, two-machine end-to-end validation.

## Requirements
- README: what/why; **unofficial, not affiliated with Anthropic**; setup (git ≥ 2.35, GitHub App registration → `docs/setup-github-app.md`, build env `SR_GITHUB_CLIENT_ID` / `SR_GITHUB_APP_SLUG`, forks use their own app); security model (age encryption, keyed names, repo-scoped expiring token, Credential Manager readable by same-user processes incl. Claude-run commands, passphrase loss = data loss, key rotation = new identity + new repo + delete old); data locations (`%LOCALAPPDATA%\dev.sessionrelay.desktop`, "Xoá dữ liệu cục bộ"); limitations (Windows only, no file-history, Claude `cleanupPeriodDays`, tested Claude Code versions).
- Supported-version check: read `version` from newest local transcript/registry; newer minor than tested → non-blocking settings notice "Chưa kiểm thử với Claude Code {v}".
- Docs: `docs/system-architecture.md`, `docs/code-standards.md`, `docs/development-roadmap.md`, `docs/project-changelog.md` (delegate docs-manager).
- Release: `npm run tauri build` (NSIS), `SHA256SUMS.txt`; GitHub Release notes.
- E2E on two real machines (different user profiles): onboarding both → A save → B clone+restore → continue in VS Code → auto-save (Stop) → A restore → diverged scenario → resolve → LocalDeleted after cleanup simulation.

## Related Code Files
- Create: `README.md`, `docs/{system-architecture,code-standards,development-roadmap,project-changelog}.md`
- Modify: `src-tauri/src/commands.rs` (version notice in AppState), `src/features/settings/settings-page.tsx`

## Implementation Steps
1. Version notice (tested version list constant, updated per release).
2. README + docs.
3. Build installer + checksums; install/uninstall test (uninstall keeps `%LOCALAPPDATA%` data unless user clicked "Xoá dữ liệu cục bộ" first — documented).
4. Two-machine E2E checklist; record results in `plans/260923-1305-claude-session-sync-desktop-app/reports/e2e-results.md`.

## Todo List
- [ ] version notice
- [ ] README + docs
- [ ] installer + SHA256SUMS
- [ ] two-machine E2E + results report

## Success Criteria
- [ ] All E2E steps pass on two machines with different profiles
- [ ] Remote repo size ≈ compressed current data (check after E2E); machine switch < 30 s for a typical project

## Risk Assessment
- SmartScreen warns on unsigned installer → documented (single user).

## Security Considerations
- Release contains no client_id secrets (none exist); checksums published with release.
