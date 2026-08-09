# Tabula

A fast, native, TablePlus-style database client — **Tauri 2 + Rust + Vue 3**.

Created by **Arcris Silang**.

**Supported databases:** MySQL / MariaDB, PostgreSQL, **SQL Server (MSSQL)**, SQLite, and Redis.

## Features

- **Multiple connections in one window** — open several databases at once and switch
  between them via top-bar connection tabs; each keeps its own tables, query tabs, and selection.
- **Browse & edit data** — filter, sort, paginate, inline edit / insert / delete with a
  reviewable changeset (Preview SQL → Commit), optimistic-concurrency safety, and production write guards.
- **Query editor** — CodeMirror with per-engine SQL dialects, autocomplete, multi-statement
  runs, streaming results, transactions, cancellation, and query history.
- **Saved queries** — name, save (⌘S), and reopen; each opens without clobbering your current work.
- **Structure & DDL** — view/create/alter tables, indexes, triggers; primary & foreign keys.
- **Foreign-key navigation** — click an FK cell to jump to the referenced row.
- **JSON/JSONB viewer**, **copy/export** (TSV, CSV, JSON, `INSERT`), **pinned tables**,
  **resizable panels**, and a **Redis key browser**.
- **Secure credentials** — passwords are stored in the OS keychain (macOS Keychain,
  Windows Credential Manager, Linux Secret Service), never in plain text.
- **SSH tunnels** with trust-on-first-use host-key verification.

---

## Install (end users)

Tabula is currently **unsigned** (no Apple/Windows code-signing certificate), so each OS
shows a one-time warning the first time you launch it. It's safe to allow — see below.

### macOS

1. Open the DMG and drag **Tabula** into your **Applications** folder.
2. First launch: **right-click** `Tabula.app` → **Open** → **Open** in the dialog.
   (Double-clicking the first time shows "Apple cannot check it for malicious software"
   because it isn't notarized — right-click → Open bypasses that, once.)
3. If macOS says the app is **"damaged and can't be opened,"** it isn't — that's the
   download quarantine flag. Clear it once:

   ```bash
   xattr -cr /Applications/Tabula.app
   ```

   Then open it normally.

### Windows

1. Run the installer — either `Tabula_<version>_x64_en-US.msi` or `Tabula_<version>_x64-setup.exe`.
2. Windows SmartScreen may say **"Windows protected your PC."** Click **More info → Run anyway**
   (this appears because the app isn't code-signed).
3. Launch **Tabula** from the Start menu.

> **Keychain / Credential Manager prompt:** the first time Tabula saves or reads a connection
> password on a machine, the OS asks permission to access its secure store. Approve it
> (on macOS choose **Always Allow**). This is expected for unsigned apps and may reappear
> after an app update.

---

## Build from source

You need the platform's native toolchain — **you cannot cross-compile between macOS and
Windows** (Tauri needs each OS's own webview and linker). Build on the OS you're targeting.

### Prerequisites (both platforms)

- [Node.js](https://nodejs.org) 20+
- [Rust](https://rustup.rs) (stable, MSVC toolchain on Windows)

### macOS

```bash
# one-time: Xcode command line tools
xcode-select --install

npm install
npm run tauri build
```

Output:
- App: `src-tauri/target/release/bundle/macos/Tabula.app`
- DMG: `src-tauri/target/release/bundle/dmg/Tabula_<version>_aarch64.dmg`

### Windows

**One-time setup — install these first:**

1. [**Rust**](https://rustup.rs) — run `rustup-init.exe`; accept the default **MSVC** toolchain.
2. [**Node.js**](https://nodejs.org) 20 or newer.
3. [**Microsoft C++ Build Tools**](https://visualstudio.microsoft.com/visual-cpp-build-tools/) —
   in the installer, tick the **"Desktop development with C++"** workload (this provides the
   linker Rust needs). Visual Studio Community with that workload works too.
4. [**WebView2 Runtime**](https://developer.microsoft.com/microsoft-edge/webview2/) — already on
   Windows 11; install the Evergreen runtime on Windows 10.

**Get the code and build** (PowerShell):

```powershell
git clone https://github.com/Arcris1/tabula.git
cd tabula
npm install
npm run tauri build
```

That produces installers you can double-click:
- MSI: `src-tauri\target\release\bundle\msi\Tabula_<version>_x64_en-US.msi`
- Setup EXE: `src-tauri\target\release\bundle\nsis\Tabula_<version>_x64-setup.exe`

**Just want to run it without installing?** `npm run tauri dev` launches the app directly
(first run compiles the Rust backend, so give it a few minutes).

### Build Windows from macOS/Linux (via GitHub Actions)

No Windows machine needed — a CI workflow builds the installers for you:

1. Push your commits to GitHub (this repo is already at `github.com/Arcris1/tabula`).
2. **Actions** tab → **Build Windows** → **Run workflow** (or push a `v*` tag).
3. Download the **`tabula-windows-installers`** artifact (contains the `.msi` and `.exe`).

The workflow lives at `.github/workflows/build-windows.yml`.

---

## Develop

```bash
npm install
npm run tauri dev      # hot-reloading dev build
```

## Test

```bash
npm run test                                  # frontend (Vitest)
cd src-tauri && cargo test                    # rust unit + sqlite integration
```

Integration tests against real engines (Docker):

```bash
docker compose -f docker-compose.test.yml up -d
cd src-tauri
DBCLIENT_TEST_MYSQL_URL="mysql://root:test@127.0.0.1:33061/dbclient_test" \
DBCLIENT_TEST_PG_URL="postgres://postgres:test@127.0.0.1:54321/dbclient_test" \
DBCLIENT_TEST_REDIS_URL="redis://127.0.0.1:63791/1" cargo test
```

SSH tunnel test (sshd container ships in the compose file):

```bash
DBCLIENT_TEST_SSH=1 cargo test --test tunnel_it
```

Live SQL Server tests run against a local server and are ignored by default:

```bash
cargo test --lib mssql -- --ignored --test-threads=1
```

## Notes

- **SSH host keys** are verified trust-on-first-use — the first fingerprint seen for a
  `host:port` is recorded in `known_hosts.json`; a changed key is refused (possible MITM).
  Use "Trust new key & reconnect" after a legitimate server key rotation.
- Design docs: `docs/superpowers/specs/`, plans: `docs/superpowers/plans/`.
# tabula
