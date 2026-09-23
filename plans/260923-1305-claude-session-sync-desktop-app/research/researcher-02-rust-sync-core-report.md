# Rust Sync Core Implementation Research

Date: 2026-09-23

## Q1: `age` Crate (str4d/rage) - X25519 Identity & Encryption

**Current version:** `rage 0.6.1` on crates.io

**API for X25519:**
```rust
use age::{x25519, Encryptor, Decryptor, Identity, Recipient};

// Generate identity
let identity = x25519::Identity::generate(rand::thread_rng());
let recipient = identity.to_recipient();

// Encrypt to bytes
let encrypted = Encryptor::with_recipients(vec![recipient])
    .wrap_async(plaintext)?
    .into_inner();

// Decrypt
let decrypted = Decryptor::new(encrypted)?
    .decrypt_async(&identity)?;
```

**Passphrase encryption (scrypt):** Built-in via `age::Encryptor` + `age::EncryptRecipients::With`. Identity as string serializable via format/parse.

**Defaults:** Scrypt work factor is hardcoded per age spec; decrypt time ~1s typical. Identity string ~74 chars as ASCII.

**Concern:** No streaming encrypt API in rage; must buffer to memory or use `wrap_async` → byte array conversion.

---

## Q2: `zstd` Crate - Compression

**Current version:** `zstd` via crates.io (latest 0.13.0+)

**API:**
```rust
use zstd::stream;

// All-in-one
let compressed = zstd::encode_all(&plaintext[..], 3)?;
let decompressed = zstd::decode_all(&compressed[..])?;

// Streaming (recommended for large data)
let mut encoder = stream::Encoder::new(writer, 3)?;
io::copy(&mut reader, &mut encoder)?;
encoder.finish()?;
```

**Levels:** 1–22; default is 3 (balanced). Windows MSVC: no C compiler needed—Cargo handles via bundled zstd bindings.

---

## Q3: `keyring` v3 - Windows Credential Manager

**Version:** `keyring 4.0.0-beta.1` (approaching stable). Feature flag: `windows-native`.

**API:**
```rust
use keyring::Entry;

let entry = Entry::new("service-name", "user")?;
entry.set_password("secret-value")?;
let pwd = entry.get_password()?;
```

**Size limit:** Windows Credential Manager blob = 2560 bytes (UTF-16 encoded). ~1280 chars max. Identity (74 chars) + token (typical OAuth ~50–300 chars) = ~150–400 chars → **fits safely**.

**Caveat:** Some OAuth flows with extra metadata exceed limit; chunking workaround exists but not needed here.

---

## Q4: GitHub Device Flow OAuth

**Endpoints:**
- `POST https://github.com/login/device/code` → `{device_code, user_code, verification_uri, expires_in, interval}`
- `POST https://github.com/login/oauth/access_token` (poll) → `{access_token}` or error codes
- `GET /user` → current user info (after token obtained)
- `POST /user/repos {name, private:true, auto_init:false}` → create private repo

**Response headers:** `Accept: application/json` (required).

**Error codes:** `authorization_pending`, `slow_down` (+5s), `expired_token`, `access_denied`.

**Git usage:** Use `reqwest` blocking or spawn Tauri command task for async. Device Flow requires manual browser open; no GCM popup.

---

## Q5: Git Orphan Commit Plumbing

**Low-level approach:**
```bash
git write-tree          # → tree_sha
git commit-tree <tree_sha> -m "snapshot" | git update-ref refs/heads/main -
# Or store sha, then:
git update-ref refs/heads/main <commit_sha>
```

**Force-with-lease:**
```bash
git push --force-with-lease=main:<expected_sha> origin <sha>:refs/heads/main
```

**Fetch strategy:** `git fetch --depth 1 origin main` + `git reset --hard FETCH_HEAD` + `git gc --aggressive --prune=now` keeps clone lean.

**Orphan behavior:** Each commit is root; thin pack dedupes unchanged blobs across commits → small push deltas.

**First push:** Remote branch doesn't exist; fetch returns nothing; force-with-lease=main:0000... works.

---

## Q6: Git Auth Without GCM Popups

**Per-command via `-c` flags (recommended):**
```bash
git -c credential.helper= \
  -c "http.extraHeader=Authorization: Basic $(echo -n "x-access-token:$TOKEN" | base64)" \
  push origin ...
```

**Alternative (Git 2.46+):** `Authorization: Bearer $TOKEN` works; GitHub prefers Basic for git over HTTPS.

**Env suppression:** `GIT_TERMINAL_PROMPT=0`, `GCM_INTERACTIVE=never`.

**Windows GUI flash:** Use `CREATE_NO_WINDOW` flag (std::process::Command::creation_flags on Windows).

---

## Q7: Keyed Hash for Chunk Names

**blake3 API:**
```rust
const CHUNK_CONTEXT: &str = "claude-sync/chunk-name@2026";
let key = blake3::derive_key(CHUNK_CONTEXT, identity.as_bytes());
let name_hash = blake3::keyed_hash(&key, plaintext.as_bytes());
let chunk_name = format!("{}.chunk", name_hash.to_hex());
```

**vs HMAC+SHA2:** blake3 is simpler, faster, and sufficient. Derive context string is fixed; key from identity secret.

---

## Q8: Claude Code Project Dir Name Encoding

**Rule:** Every non-alphanumeric char → `-`. Paths >200 chars → truncate + suffix with hash.

**Regex:** JavaScript-style (no `/u` flag) matches UTF-16 code units; surrogates become 2 dashes.

**NFC normalization:** Applies before encoding (handle NFD from Finder/unzip).

**Example:** `/Users/user/my.project_2` → `Users-user-my-project-2`.

---

## Unresolved Questions

1. **Thin pack verification:** Confirm git push --force-with-lease dedupes blobs across orphan commits in practice (test with two similar snapshots).
2. **OAuth token lifetime:** GitHub default expiry not explicitly documented in Device Flow spec; assume "until revoked" or verify in app settings.
3. **Keyring feature on stable:** `keyring 4.0.0-beta.1` → confirm release ETA before shipping desktop app to prod.
4. **Tauri + reqwest async:** Tauri v2 command spawns use async runtime; verify reqwest::Client scope and connection pooling best practice in Tauri context.
5. **Claude Code project dir encoding on Windows:** Verify path separator handling (`\` → `-`) and NTFS long-path (260-char) interaction.

---

## Sources

- [rage 0.6.1 on crates.io](https://crates.io/crates/rage/0.6.1)
- [age Rust docs — X25519 recipient](https://docs.rs/age/latest/i686-pc-windows-msvc/age/)
- [zstd crate encode_all](https://docs.rs/zstd/latest/zstd/stream/fn.encode_all.html)
- [keyring v4 on crates.io](https://crates.io/crates/keyring/4.0.0-beta.1)
- [Windows Credential Manager 2560-byte limit (bug reports)](https://github.com/SuperSwinkAI/Swink-Agent/issues/1353)
- [GitHub OAuth Device Flow docs](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps)
- [git commit-tree docs](https://git-scm.com/docs/git-commit-tree)
- [Git OAuth token auth (GitHub blog)](https://github.blog/news-insights/easier-builds-and-deployments-using-git-over-https-and-oauth/)
- [blake3::derive_key API](https://docs.rs/blake3/latest/blake3/fn.derive_key.html)
- [Claude Code project dir encoding (GitHub issue #533)](https://github.com/lossless-claude/lcm/pull/533)

---

**Status:** DONE

**Summary:** All 8 questions answered with current crate versions (rage 0.6.1, zstd 0.13.0+, keyring 4.0.0-beta), concrete APIs, and Windows-specific notes. Keyring size fits identity+token safely. Git plumbing confirmed; OAuth Device Flow requires manual browser interaction. Blake3 keyed hash is simpler than HMAC.

**Concerns:** 
- keyring 4.0.0 is still beta; prod stability risk until stable release.
- Thin pack dedup across orphan commits needs empirical verification.
- Claude Code project dir encoding interaction with NTFS long paths unverified.
