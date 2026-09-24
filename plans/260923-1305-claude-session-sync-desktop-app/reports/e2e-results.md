# E2E Results — v0.1.0

Machines: **A** = ______ (Windows __, user profile ______) · **B** = ______ (different user profile / path layout)
Build: `session-relay_0.1.0_x64-setup.exe`, SHA-256 matches `SHA256SUMS.txt`: [ ]
Claude Code: A ______ · B ______ · Git: A ______ · B ______

Fill in Result (pass / fail + note) as you go. A failure: note what you saw and the time, then attach `%LOCALAPPDATA%\dev.sessionrelay.desktop\logs\session-relay.log`.

## 1. Install and onboarding

| # | Step | Expected | Result |
|---|---|---|---|
| 1.1 | A: run the installer | SmartScreen warns (unsigned); installs to `%LOCALAPPDATA%\session-relay`, starts | |
| 1.2 | A: onboarding (sign in, create repo + install app, new passphrase, machine name, workspace folders, auto-save on) | reaches the dashboard; `~/.claude/settings.json` has the Stop + SessionEnd hooks and a `settings.json.bak-*` exists | |
| 1.3 | B: same installer, onboarding with the same repo | asks to unlock (not create); the same passphrase works | |

## 2. Save on A, restore on B

| # | Step | Expected | Result |
|---|---|---|---|
| 2.1 | A: chat in project P (VS Code), click Save | status becomes In sync; activity shows the push | |
| 2.2 | B: P is not cloned yet; open it in the dashboard | link panel offers "Clone to …" (or "Found at …" if a checkout exists) | |
| 2.3 | B: clone, then restore | ≤ 3 clicks after onboarding; progress shows "Cloning the project… %" | |
| 2.4 | B: open P in VS Code → past conversations | A's session is listed, opens, and tool results / paths resolve to B's folders | |
| 2.5 | Time A save → B restore for a typical project | < 30 s | |

## 3. Auto-save (Phase 5 manual checks)

| # | Step | Expected | Result |
|---|---|---|---|
| 3.1 | B: continue the session in VS Code (new session after enabling auto-save), stop replying | ~2 min later A's dashboard shows "The cloud is newer" from B; Claude never waited on the hook | |
| 3.2 | CLI: `claude -p "hi"` in P | returns immediately; the save lands ~5 s after exit | |
| 3.3 | Kill the `session-relay.exe hook-worker` process during its wait (Task Manager) | marker stays; the open app finishes the save within ~15–20 min, or on Quit | |
| 3.4 | A: Restore | B's turns appear on A | |

## 4. Conflicts and cleanup

| # | Step | Expected | Result |
|---|---|---|---|
| 4.1 | Offline/without saving, continue the same session on A and on B, then save A, then open B | B shows "The two machines diverged"; auto-save on B does not overwrite | |
| 4.2 | B: Resolve… → keep this machine's (one file), keep the cloud's (another, if available) | each choice applies; the discarded copy exists in `backups\` (encrypted) | |
| 4.3 | Resolve a file whose Claude session is still open | dialog shows why it was skipped | |
| 4.4 | Delete a synced session file locally (cleanup simulation), refresh | shows as only left in the cloud; not re-uploaded; per-session Restore brings it back | |

## 5. Uninstall

| # | Step | Expected | Result |
|---|---|---|---|
| 5.1 | Add an unrelated hook to `~/.claude/settings.json`, then uninstall on B | our two hooks removed, the other hook byte-identical, backup file exists | |
| 5.2 | After uninstall | `%LOCALAPPDATA%\dev.sessionrelay.desktop` still there; Credential Manager entries remain until sign-out (documented) | |
| 5.3 | Reinstall over an existing install (update mode) | hooks untouched during the update | |

## 6. Storage

| # | Check | Expected | Result |
|---|---|---|---|
| 6.1 | Size of the storage repo on GitHub vs size of the synced project dirs | about the compressed size of the current data (one snapshot, no history) | |
| 6.2 | Browse the repo on GitHub | only `session-relay.json`, `.gitattributes`, `keys/identity.age`, `p/<hash>/…`; no readable names or content | |
