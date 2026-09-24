# Code Standards

The conventions this codebase actually follows. Principles: YAGNI, KISS, DRY.

## Layout

| Path | Content |
|---|---|
| `src-tauri/src/engine/` | Sync core, no Tauri types: git, crypto, manifests, three-way state, auth, key setup, hooks config, workspace scan, clone |
| `src-tauri/src/*.rs` | App level: `lib.rs` (Tauri setup), `app_state.rs`, `dashboard.rs`, `watcher.rs`, `tray.rs`, `autostart.rs`, `auto_save.rs`, `hook.rs` / `hook_worker.rs` / `pending.rs` (auto-save), `git_credential.rs`, `login.rs` |
| `src-tauri/src/commands/` | Tauri commands, grouped by screen (`setup`, `settings`, `dashboard`, `workspace`) |
| `src-tauri/tests/` | Integration tests; `tests/support/mod.rs` simulates machines sharing a local bare repo |
| `src/features/` | React screens (`onboarding`, `dashboard`, `settings`) |
| `src/components/`, `src/lib/` | Shared components; hooks (`use-*.ts`), IPC wrappers (`tauri-commands.ts`), i18n, formatting |
| `src/locales/{en,vi}.json` | All UI copy |

## Files

- One purpose per file, **under 200 lines**; split before crossing it.
- Rust files `snake_case.rs`; TypeScript files `kebab-case.ts(x)` with names that say what they hold (`project-detail-pane.tsx`, `use-workspace-scan.ts`).

## Rust

- Edition 2021, Rust 1.89+. `cargo clippy --all-targets -- -D warnings` must pass.
- Errors: one `engine::error::Error` enum (`thiserror`), `Result<T>` alias. Every variant has a stable `code()`; commands return `{ code, detail }` and the UI maps the code to text. Rust never returns UI copy.
- Error classes matter: only `Error::Git` means "the store clone may be broken" and may trigger a rebuild; `Auth`, `Network`, `Io` must not (see `git_failure.rs`).
- Blocking work (git, crypto, file I/O) runs in `commands::blocking` (`spawn_blocking`), never on the async runtime.
- Anything that writes the store clone holds `SyncLock` (`sync.lock`); every token write holds `auth.lock`.
- Git always runs through `GitEnv` (isolated config, our credential helper, timeouts) except `git_clone.rs`, which deliberately uses the user's own git setup.
- `unsafe` only for Win32 calls, each with a `// SAFETY:` comment.
- No secrets in logs, argv, env or `.git/config`. Logs carry error codes; the app log rotates at 1 MB (`logs/session-relay.log`, one previous file kept).

## TypeScript / React

- `tsc` strict mode (`noUnusedLocals`, `noUnusedParameters`); `pnpm build` runs `tsc` then Vite. The package manager is pnpm (`pnpm-lock.yaml`); do not mix in npm.
- Commands are called through `src/lib/tauri-commands.ts` only; errors become codes via `errorCode()`.
- One user action at a time through `useDashboard().run(label, action)`; progress comes from the `sync-progress` event.
- Styling: Tailwind v4 with the tokens in `src/styles/design-tokens.css` (from `DESIGN.md`): monochrome, Clay only as an accent, serif for headings only, `cursor: pointer` on every interactive element (global rule), `motion-reduce` respected.

## Copy and localization

- Code, comments, identifiers and string literals are English. UI text lives only in `src/locales/en.json` and `vi.json`, read with `t("key", params)`; tray labels are read from the same files.
- Both locale files must have exactly the same keys.

## Comments

- Only what the code cannot say: constraints, invariants, protocol quirks, safety notes, non-obvious "why". One or two lines.
- No narration, no step numbering, no banner blocks.

## Tests

- Engine behaviour is tested against real git and real crypto in temporary folders: `tests/support` creates machines A and B sharing a local bare repo (`file://`). No mocks.
- Tests never touch `~/.claude`, the real app data folder, Windows Credential Manager entries of service `session-relay`, or GitHub.
- A small pure function gets a unit test in its own file (`#[cfg(test)] mod tests`).
- Name tests after the behaviour they prove (`an_unreachable_remote_keeps_the_clone_and_the_last_snapshot`).

## Checks before a commit

```bash
cd src-tauri
cargo clippy --all-targets -- -D warnings
cargo test
cd ..
pnpm exec tsc --noEmit -p .
pnpm build
```

There is no CI yet; run these locally.

## Commits

- Conventional Commits (`feat:`, `fix:`, `perf:`, `docs:`, `chore:`…), optional scope (`feat(auto-save): …`), imperative subject, body explains why.
- Never commit `.env` or anything secret. Before committing, scan the staged diff for tokens (`ghu_`, `gho_`…) and the GitHub App client ID.
