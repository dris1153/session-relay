# Brainstorm: build says "no GitHub App configured" despite .env

Date: 2026-10-05 · Status: fixed, released as v0.3.1

## Problem
v0.3.0 (installer + portable) showed "Build not configured for GitHub App"; sign-in impossible.

## Root cause
`.env` had `SR_GITHUB_APP_SLUG=https://github.com/apps/session-relay-app` (whole URL). `build.rs` accepts only `[A-Za-z0-9-]`, emitted a cargo *warning* and dropped the value -> `has_client_id` false at runtime. Client ID was fine.
Release process miss: build log had "SR_GITHUB_APP_SLUG has invalid characters"; the check only looked for "is not set" and for the client ID in the binary, never the slug.

## Decisions (user)
- build.rs: invalid (present but malformed) value now panics with variable name + expected form; missing value still only warns (dev builds need no .env). Rejected: auto-extract slug from URL (hides config errors).
- Release: v0.3.1 as Latest; v0.3.0 notes say it cannot sign in, its assets deleted. Rejected: overwrite v0.3.0 assets (downloaders keep the bad build silently).

## Verification
- build.rs with bad slug fails with the message; with good .env, `cargo check` has no warnings.
- Release exe contains both `Iv23li…` and `session-relay-app`, not the URL form.

## Lessons
- Release gate must check both env values in the binary, and grep the build log for any `SR_GITHUB` warning, not only "not set".
- Users who already installed 0.3.0 only need to install 0.3.1 (data untouched).
