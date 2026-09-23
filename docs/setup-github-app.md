# Setting up the GitHub App

Session Relay signs in with a **GitHub App** through the device flow. The app only gets access to the one private repository that stores your encrypted sessions. Each person who builds Session Relay registers their own app; no secret is involved.

## 1. Register the app (once)

1. Open <https://github.com/settings/apps/new>.
2. **GitHub App name**: something unique, e.g. `session-relay-<your-username>`.
3. **Homepage URL**: any URL, e.g. your GitHub profile.
4. **Identifying and authorizing users**:
   - leave **Callback URL** empty,
   - keep **Expire user authorization tokens** checked,
   - check **Enable Device Flow**,
   - leave **Request user authorization (OAuth) during installation** unchecked.
5. **Webhook**: uncheck **Active**.
6. **Repository permissions** → **Contents: Read and write** (Metadata becomes read-only automatically). No other permissions.
7. **Where can this GitHub App be installed?** → **Only on this account**.
8. Create the app. You do not need a client secret or a private key.

## 2. Configure the build

Copy `.env.example` to `.env` and fill in:

```
SR_GITHUB_CLIENT_ID=<Client ID shown on the app page, starts with Iv23li>
SR_GITHUB_APP_SLUG=<last part of https://github.com/apps/<slug>>
```

Both values are public identifiers. `.env` is git-ignored; CI can pass the same names as environment variables instead.

## 3. First run

The onboarding screens walk through the rest:

1. Sign in with the code shown by the app.
2. Create an **empty private** repository named `claude-sessions` (the app links to a pre-filled form).
3. Install your GitHub App and select **only** that repository.
4. Choose a passphrase. It encrypts everything before it leaves your machine; **losing it means losing the data**.

Other machines only sign in and unlock with the same passphrase.

## Security notes

- The access token expires after 8 hours and is refreshed automatically; the refresh token rotates on every use.
- Tokens and the unlocked key live in Windows Credential Manager with *Local* persistence (they do not roam). Any program running as your Windows user can read them, including commands Claude runs for you.
- Signing out removes them from this machine. To revoke the app's access completely, use <https://github.com/settings/apps/authorizations>.
