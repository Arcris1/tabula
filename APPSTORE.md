# Publishing Tabula to the Mac App Store

This is the **Mac App Store** route (listed in the store, sandboxed, reviewed by Apple).
It is more involved than a normal signed download. Read the **Reality check** first.

> The everyday download build (`npm run tauri build`) is unchanged. The App Store build
> uses the separate `tauri.appstore.conf.json` + `entitlements.appstore.plist` files, so
> nothing here affects your normal dev or Developer-ID builds.

---

## Reality check (read this)

- **App Sandbox is mandatory.** Apple only accepts sandboxed macOS apps. The entitlements
  in `src-tauri/entitlements.appstore.plist` declare what Tabula needs (network + user-picked
  files). See **Sandbox caveats** below for the one behavior that needs verifying.
- **You do the account parts, I do the code/config parts.** Certificates, the App ID,
  provisioning profile, App Store Connect, and the upload all happen on your Mac with your
  Apple ID — I can't (and shouldn't) hold your signing credentials.
- **Expect iteration.** The first sandboxed build almost always surfaces a file-access or
  entitlement issue in testing. Budget a couple of round-trips before it's clean.

---

## 0. Prerequisites

- **Apple Developer Program** membership (paid, $99/yr) — you have this.
- **Xcode** installed (for the signing toolchain: `productbuild`, `codesign`), or at least the
  command line tools plus **Transporter** from the Mac App Store.

---

## 1. One-time Apple setup (in your account)

Do these once in the Apple Developer portal and App Store Connect.

1. **Register the App ID** — https://developer.apple.com/account/resources/identifiers
   - Bundle ID: **`com.arcris.dbclient`** (must match `identifier` in `tauri.conf.json`).
   - Capabilities: leave defaults (App Sandbox is expressed via the entitlements file, not here).

2. **Create two certificates** — https://developer.apple.com/account/resources/certificates
   - **Apple Distribution** — signs the `.app`.
   - **Mac Installer Distribution** — signs the `.pkg` you upload.
   - Download both and double-click to install into your login Keychain.

3. **Create a Mac App Store provisioning profile** —
   https://developer.apple.com/account/resources/profiles
   - Type: **Mac App Store**, App ID: `com.arcris.dbclient`, cert: your Apple Distribution cert.
   - Download it. You'll embed it in the app in step 3.

4. **Create the app record in App Store Connect** — https://appstoreconnect.apple.com
   - New app → platform macOS → name (e.g. **Tabula**) → bundle ID `com.arcris.dbclient` → SKU.
   - Fill listing metadata: category (Developer Tools), description, keywords, support URL,
     **privacy policy URL** (required), screenshots, and a privacy "nutrition label"
     (Tabula stores connection details locally; declare accordingly).

Find your **Team ID** at https://developer.apple.com/account (top right, under Membership).

---

## 2. Fill in your signing identity

Edit **`src-tauri/tauri.appstore.conf.json`** — replace the two placeholders:

```json
"signingIdentity": "Apple Distribution: YOUR NAME (YOURTEAMID)",
"providerShortName": "YOURTEAMID"
```

Use the exact certificate name as shown by:

```bash
security find-identity -v -p codesigning
```

---

## 3. Build, package, upload

```bash
# 1. Build the sandboxed, App-Store-signed .app (only the app bundle)
npm run tauri build -- --config src-tauri/tauri.appstore.conf.json --bundles app

# 2. Embed the provisioning profile you downloaded in step 1.3
cp ~/Downloads/Tabula_AppStore.provisionprofile \
   "src-tauri/target/release/bundle/macos/Tabula.app/Contents/embedded.provisionprofile"

# 3. Re-sign the app so the embedded profile is covered by the signature
codesign --force --deep \
  --sign "Apple Distribution: YOUR NAME (YOURTEAMID)" \
  --entitlements src-tauri/entitlements.appstore.plist \
  "src-tauri/target/release/bundle/macos/Tabula.app"

# 4. Wrap it in an installer package signed with the INSTALLER cert
productbuild --component \
  "src-tauri/target/release/bundle/macos/Tabula.app" /Applications \
  --sign "3rd Party Mac Developer Installer: YOUR NAME (YOURTEAMID)" \
  Tabula.pkg

# 5. Upload to App Store Connect (choose ONE):
#    a) open the Transporter app, drag in Tabula.pkg, Deliver.
#    b) CLI:
xcrun altool --upload-app -f Tabula.pkg -t macos \
  --apple-id YOUR_APPLE_ID_EMAIL --password "APP_SPECIFIC_PASSWORD"
```

- **App-specific password**: create at https://account.apple.com → Sign-In & Security →
  App-Specific Passwords (your normal Apple password will not work here).
- After upload, the build appears in App Store Connect after processing (a few minutes to an hour).

---

## 4. Submit for review

In App Store Connect: attach the processed build to your app version, complete the review
information, and **Submit for Review**. First macOS reviews commonly take 1–3 days.

---

## Sandbox caveats to verify (important)

Everything network — MySQL, Postgres, SQL Server, Redis, and **SSH tunnels** — works under the
sandbox with the entitlements provided (`network.client` + `network.server`). The things to test
in the sandboxed build before submitting:

1. **SQLite files — handled, verify on your signed build.** Under the sandbox the app can only
   touch files the user picks through the system dialog; a typed path grants nothing, and a saved
   connection reopened by path on the next launch would be denied. This is now addressed:
   - The connection form uses a **Browse…** picker for SQLite (the dialog is what grants access).
   - `tauri-plugin-fs` + `tauri-plugin-persisted-scope` are registered, and picking a file calls
     `remember_file`, which adds it to the fs scope so persisted-scope stores a macOS
     **security-scoped bookmark** and restores access on the next launch.

   Because this can only be exercised in a real sandboxed (signed) build, **verify on your first
   App Store build**: create a SQLite connection via Browse…, quit, relaunch, and confirm it
   reconnects. If it doesn't, the fallback is direct security-scoped bookmarks stored per
   connection — tell me and I'll switch to that. (Network engines are unaffected either way.)
   Existing SQLite connections created by *typing* a path must be re-picked with Browse… once.

2. **Keychain (saved passwords).** The `keyring` crate uses the macOS Keychain; under the
   sandbox items live in the app's own keychain group. Verify saving/loading a connection
   password works in the sandboxed build; if Keychain is denied, we add a
   `keychain-access-groups` entitlement matching the provisioning profile.

3. **Exports/imports.** CSV/JSON export and SQL/CSV import go through the file dialog, so they're
   covered by `files.user-selected.read-write`. Verify once.

---

## What's in the repo for this

| File | Purpose |
|------|---------|
| `src-tauri/entitlements.appstore.plist` | App Store sandbox entitlements |
| `src-tauri/tauri.appstore.conf.json` | App Store build overrides (category, signing, entitlements) — **edit the placeholders** |
| `APPSTORE.md` | this guide |

The normal `tauri.conf.json` and `.github/workflows/build-windows.yml` are untouched.
