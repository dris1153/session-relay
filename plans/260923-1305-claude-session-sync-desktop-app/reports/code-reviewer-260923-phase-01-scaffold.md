# Code Review — Phase 1 Scaffold (session-relay)

Date: 2026-09-23 · Reviewer: code-reviewer · Score: **7.5/10**

## Scope
- Files: `src-tauri/{build.rs,tauri.conf.json,Cargo.toml,src/main.rs,src/lib.rs,capabilities/default.json}`, `src/{main.tsx,app.tsx,styles/design-tokens.css}`, `index.html`, `vite.config.ts`, `.env.example`, `.gitignore`
- LOC: ~230 (excluding lockfiles/icons)
- Focus: new scaffold. Checked against phase-01, plan.md §Conventions/§Key Dependencies, DESIGN.md, and phases 2–7 where the scaffold config matters later.
- How verified: build.rs parser run in a scratch crate (CRLF, quotes, BOM, UTF-16, `export`, inline comment, empty env var, missing `.env`); Tailwind utilities compiled with the project's `@tailwindcss/node`; built `dist/` CSS inspected; Tauri 2.11.6 / tauri-utils 2.9.3 sources read for CSP nonce and dev behaviour; generated ACL manifests read for default permission sets; Tauri NSIS template read for default install dir; `tsc --noEmit` passes. The NSIS installer exists (`target/release/bundle/nsis/session-relay_0.1.0_x64-setup.exe`).

## Overall Assessment
The scaffold is clean and small, and it is close to the plan. The CSP is explicit and tight, `withGlobalTauri` is off, the fonts are bundled with their Vietnamese subsets, and every DESIGN token becomes the intended utility. There are two real problems: one CSS rule shrinks the whole rem-based spacing scale, and the plan's data directory is the same folder the installer uses. Several smaller items will cause trouble in Phases 2–7 if they are left alone.

## Critical Issues
None.

## High Priority

### H1. `html { font-size: 14px }` changes `rem` and breaks the 8px spacing grid
`src/styles/design-tokens.css:40-46` applies `font-size: var(--text-body)` to `html` as well as `body`. Tailwind v4 spacing is `--spacing: .25rem`, so the whole rem scale now uses 14px instead of 16px. `p-2` = 7px, `p-4`/`gap-4` = 14px, and `px-16` = 56px instead of DESIGN's 64px. Default text sizes shrink too. This was confirmed in the built CSS: `html,body{...font-size:var(--text-body)}`. Every UI phase would silently drift off DESIGN.md's 8px grid.
Fix: set font-size on `body` only.
```css
html, body { background: var(--color-bone-parchment); color: var(--color-carbon-ink); }
body { font-size: var(--text-body); }
```
(`font-family` is redundant here, because preflight already uses `--default-font-family: var(--font-sans)`.)

### H2. The plan's data dir is the NSIS install dir, and neither matches where Tauri/WebView2 writes
- `tauri.conf.json:3` sets `productName: "session-relay"`. Tauri's NSIS `currentUser` mode (the default) installs to `$LOCALAPPDATA\${PRODUCTNAME}`, which is `%LOCALAPPDATA%\dev.sessionrelay.desktop\`. That is exactly the folder plan.md §Conventions reserves for **all app data** (settings, clones, backups).
- `tauri.conf.json:5` sets `identifier: "dev.sessionrelay.app"`. WebView2's profile (`EBWebView`, which holds localStorage and caches) goes to `%LOCALAPPDATA%\dev.sessionrelay.app\`. The NSIS "delete app data" checkbox removes `%APPDATA%\<identifier>` and `%LOCALAPPDATA%\<identifier>`, never `session-relay`.
- Consequences: Phase 7's "Xoá dữ liệu cục bộ" (delete local data) would delete the installed exe and uninstaller if it runs `remove_dir_all(app_dir)`. On Windows the running exe is locked, so it fails partway and leaves a broken install. The README claim "all data in %LOCALAPPDATA%\dev.sessionrelay.desktop" would be false. The uninstaller checkbox would be misleading.
- Fix (decide **before Phase 2** writes `paths.rs`): make the app dir `%LOCALAPPDATA%\<identifier>`, the same as Tauri `app_local_data_dir()`. Compute it from `LOCALAPPDATA` + an `IDENTIFIER` const so the hook subprocess, which has no Tauri context, agrees. Then update plan.md §Conventions and phases 2/3/7. If you change the identifier, do it now, because it is effectively permanent: it fixes the data path and the WebView2 profile. Tauri CLI also warns about identifiers ending in `.app`.

## Medium Priority

### M1. build.rs fails silently, so a build without a client ID "succeeds"
`src-tauri/build.rs:11,20`. When a key doesn't resolve, nothing is emitted and there is no warning. `option_env!` (Phase 3) then yields `None`, and the failure only shows up at runtime when the user tries to log in. Tested cases that resolve to `None` silently:
- `.env` written by Windows PowerShell 5.1 `Out-File` / `>`: UTF-16LE, so `read_to_string` errors and `unwrap_or_default()` returns `""`. Both keys are `None`.
- UTF-8 BOM before the first key: `trim()` does not strip U+FEFF. The first key is `None`.
- `export SR_GITHUB_CLIENT_ID=...`: `None`.
- A process env var that is set but empty (for example a CI secret that isn't available): the `.env` fallback is skipped, **and** `option_env!` returns `Some("")` because rustc inherits the process env (verified).
Other lenient cases: `'single-quoted'` values keep their quotes, and `value # comment` keeps the comment.
What already works (tested): CRLF, `"double quotes"`, `#`-prefixed comment lines, and `=` inside the value.
Fix: don't grow a dotenv parser. Validate the value's charset and warn loudly:
```rust
let file = std::fs::read_to_string("../.env").unwrap_or_default();
let file = file.trim_start_matches('\u{feff}');
// ...
let value = std::env::var(key).ok().filter(|v| !v.is_empty()).or_else(|| { /* same lookup */ });
match value.filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')) {
    Some(v) => println!("cargo:rustc-env={key}={v}"),
    None => println!("cargo:warning={key} missing or invalid (see .env.example; save .env as UTF-8)"),
}
```
Optionally fail when `PROFILE == "release"`, so an installer can never ship without an app ID. Phase 3 should still treat `Some("")` as absent.

### M2. CSP is not enforced during `tauri dev` on desktop
In Tauri 2.11.6 the CSP header is only attached to embedded assets served by the tauri protocol (`protocol/tauri.rs`). Dev-server proxying exists only for `cfg(all(dev, mobile))`. On desktop, the webview loads `devUrl` directly from Vite, so `csp`/`devCsp` have no effect. Any CSP violation (a new image host, a frontend `fetch`, an inline `<style>`) passes in dev and breaks only in the installed app. Phase 1's success criterion only checks that the installer is produced.
Fix: add to each UI phase's acceptance: "run `npm run tauri build -- --debug` (embedded assets, devtools available) and confirm there are no CSP errors in the console".

### M3. `panic = "abort"` in release is wrong for a resident tray app plus a hook worker
`src-tauri/Cargo.toml:33`. With abort, a panic anywhere kills the whole process: an async command, the Phase 4 60-second watcher loop, or parsing Claude's undocumented `sessions/<pid>.json`. With unwind, tokio would isolate it in the task. Abort also skips `Drop` cleanup (tempfile guards, lock/marker guards) in `hook-save`/`hook-worker`. Combined with `windows_subsystem = "windows"` and `strip = true`, the app simply disappears with no trace.
Fix: remove the `panic = "abort"` line (the size cost is small). Plan a file logger plus a panic hook in `%LOCALAPPDATA%\<app>\logs` in Phase 2/4.

### M4. Capabilities are additive, so `opener:default` must be *replaced* in Phase 3, not extended
`src-tauri/capabilities/default.json:8`. `opener:default` = `allow-open-url` + `allow-reveal-item-in-dir` + a scope of `http://*`, `https://*`, `mailto:*`, `tel:*`. The scaffold doesn't use it. Phase 3 plans "`opener:allow-open-url` limited to github.com". If that is added next to `opener:default`, the broad scope stays active. `core:default` (line 7) also grants `event:allow-emit`, `image:allow-from-path`, and full tray/menu creation. That is acceptable for a scaffold. In Phase 4, trim it to an explicit list (event listen/unlisten, the window APIs actually used, `app:default`).

## Low Priority
- **L1** `build.rs:10`: with `../.env` missing (CI, or env-var-only users), `rerun-if-changed` makes every `cargo build/test` rerun the build script and recompile `session_relay_lib` (verified: "the file `../.env` is missing"). This is fine for one-shot CI. It is noticeable in Phase 2's `cargo test` loop without a `.env`. Accept it and document it, or tell those users to create an empty `.env`.
- **L2** `Cargo.toml:16`: `crate-type = ["staticlib", "cdylib", "rlib"]` exists for mobile only (out of scope). Release currently also produces `session_relay_lib.dll` and `.lib`, each with a fat-LTO link. Use `["rlib"]` (still works for the bin and Phase 2 integration tests).
- **L3** `tauri.conf.json:24`: optional hardening: `base-uri 'none'; form-action 'none'; object-src 'none'` (the first two are not covered by `default-src`), plus `app.security.freezePrototype: true`. Latent trap: an inline `<style>` added to `index.html` makes Tauri inject a style nonce into `style-src` (`manager/mod.rs` `replace_csp_nonce`). That silently disables `'unsafe-inline'` for any runtime-injected styles. Keep `index.html` free of inline styles.
- **L4** `design-tokens.css:1`: Tailwind auto-detection scans `plans/`, `docs/`, `DESIGN.md`, and `.claude/`. The built CSS contains utilities the app never uses (`.absolute`, `.grid`, `.truncate`, `.ring`, `.shadow`, …), and more will appear as the docs grow. Use `@import "tailwindcss" source("..");` (resolves to `src/`).
- **L5** `.gitignore:27`: only `.env` is ignored. Vite also loads `.env.production` / `.env.development`. Add `.env.*` + `!.env.example`. (`.env` is correctly ignored, verified with `git check-ignore`. No commits exist yet.)
- **L6** `design-tokens.css:49-57`: the cursor rule misses `[aria-disabled="true"]` (`:disabled` never matches `role=button` divs), radio labels, and `select:disabled` → `not-allowed`.
- **L7** `index.html:2` has `lang="en"`. Phase 4 should set `document.documentElement.lang` from the active locale (vi/en).
- **L8** `lib.rs:6`: `.expect(...)` with the GUI subsystem means that on startup failure (for example missing WebView2) the app exits with no message. This is covered by the M3 logging.
- Info, Phase 4: autostart with `--minimized` needs the `main` window to be `"visible": false` in config (shown explicitly), or it will flash on boot.

## Edge Cases Found by Scout
- Windows PowerShell 5.1 `>` produces a UTF-16 `.env`, and the client ID silently disappears (M1).
- An empty inherited env var shadows `.env` and gives `option_env! == Some("")` (M1).
- The NSIS currentUser INSTDIR collides with the planned data dir. WebView2 data lives under the identifier dir, and the uninstall checkbox targets the identifier dir only (H2).
- CSP is untested in dev on desktop (M2).
- Adding `opener` permissions later won't narrow the scope (M4).
- A missing `.env` forces crate recompilation on every build (L1).

## Positive Observations
- The CSP matches the plan exactly. IPC (`ipc:` + `http://ipc.localhost`), the avatar host, bundled fonts, and inline styles are all covered, and there are no remote script or connect origins. The GitHub API goes through Rust.
- Tauri currently adds no nonce (the built `index.html` has no inline script or style), so `'unsafe-inline'` is effective as intended.
- Fonts are all emitted as files, none inlined as `data:`, so `font-src 'self'` holds. Both families include the `vietnamese` subset. The family names (`Inter Variable`, `Source Serif 4 Variable`) match the fontsource CSS.
- Every token compiles to the intended utility: `text-{caption,body,heading-sm,heading}` with line-heights, `font-serif`/`font-sans`, `rounded-{control,card,card-elevated}`, `shadow-{soft,featured}` (including `/alpha`), and all colors.
- In build.rs, `rerun-if-env-changed` is emitted per key, real env takes precedence over `.env`, the path is correctly relative to the manifest dir, and CRLF, double quotes, `=` in values, and comment lines are handled.
- `.env.example` clearly states the values are public identifiers and that forks need their own app. Vite never exposes `SR_*`, since only `VITE_` is exposed and `server.fs.deny` covers `.env*`.
- File naming follows the conventions (kebab-case TS, snake_case Rust), all literals are English, and `tsc` is clean.

## Recommended Actions
1. H1: move `font-size` to `body` (one line).
2. H2: decide the app data dir before Phase 2 (recommended: `%LOCALAPPDATA%\<identifier>`), and settle the identifier now. Update plan.md §Conventions and phases 2/3/7.
3. M1: harden build.rs (strip the BOM, filter empty env, validate the charset, `cargo:warning`, optional release hard-fail).
4. M3: drop `panic = "abort"`. Plan the logger and panic hook.
5. M2/M4: add "`tauri build --debug`, check the console for CSP errors" to UI-phase acceptance. In Phase 3, replace (don't add to) `opener:default`.
6. Low items when convenient (L2, L4, L5 are one-liners).

## Metrics
- Type coverage: strict TS, `tsc --noEmit` exits 0
- Tests: none (scaffold, as expected)
- Lint: no ESLint configured; Rust not clippy-checked in this review
- Build: debug and release built; NSIS installer produced

## Plan Status
The phase-01 TODO list is all checked. Success criteria are still unchecked: the installer exists, but the "serif heading on parchment canvas" check and the CSP check of the installed build (see M2) should be confirmed before ticking. The plan.md table still shows Phase 1 as "Pending".

## Unresolved Questions
1. Is a single `%LOCALAPPDATA%` app dir meant to also hold the WebView2 profile? That decides H2 option A (identifier dir) versus documenting two dirs.
2. Should a release build hard-fail without `SR_GITHUB_CLIENT_ID` (safer), or only warn (lets forks build unconfigured)?
