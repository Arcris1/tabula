# M1 — Connect & Browse Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A launchable Tauri desktop app where the user saves color-coded connections to MySQL, PostgreSQL, SQLite, and Redis, browses tables in a virtualized data grid with sort/filter/pagination, and browses Redis keys by pattern — read-only.

**Architecture:** Tauri v2 app. Rust owns all DB access behind two traits (`SqlDriver` over sqlx for MySQL/Postgres/SQLite; `KvDriver` over the redis crate). Vue 3 + Pinia frontend calls Rust via `invoke` only. Connection configs in a JSON app-data file; passwords in macOS Keychain via `keyring`.

**Tech Stack:** Tauri 2, Rust (sqlx 0.8, redis 0.27, keyring 3, async-trait, thiserror), Vue 3 + TypeScript, Pinia, Tailwind CSS v4, Vitest.

## Global Constraints

- macOS only; app identifier `com.arcris.dbclient`; product name `dbclient`.
- All four engines (MySQL/MariaDB, PostgreSQL, SQLite, Redis) supported in M1, read-only.
- Passwords/secrets NEVER written to disk in plaintext — Keychain only (`keyring` crate).
- All Rust structs crossing the bridge use `#[serde(rename_all = "camelCase")]`.
- All errors crossing the bridge are `AppError { kind, message, detail }`, kind ∈ `ConnectionFailed | TunnelFailed | QueryError | ConflictOnCommit | Timeout | Internal`.
- Redis key listing uses `SCAN` (never `KEYS`).
- UI: Linear-style — compact density, subtle borders, dark mode default.
- Rust tests: `cd src-tauri && cargo test`. Frontend tests: `npm run test`. Integration tests for MySQL/PG/Redis skip gracefully when env URLs are unset.
- Commit after every task (steps include the commit).

## File Structure

```
dbclient/
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── src/
│   │   ├── main.rs              # template entry, calls dbclient_lib::run()
│   │   ├── lib.rs               # builder, AppState, command registration
│   │   ├── error.rs             # AppError + ErrorKind
│   │   ├── config.rs            # ConnectionConfig + ConfigStore (JSON file)
│   │   ├── credentials.rs       # Keychain wrapper
│   │   ├── registry.rs          # ConnectionRegistry of live connections
│   │   ├── commands.rs          # all #[tauri::command] fns
│   │   └── drivers/
│   │       ├── mod.rs           # traits + shared types + SELECT builder
│   │       ├── sqlite.rs
│   │       ├── mysql.rs
│   │       ├── postgres.rs
│   │       ├── redis_kv.rs      # KvDriver impl
│   │       └── keytree.rs       # key list -> namespace tree
│   └── tests/
│       ├── common/mod.rs        # shared browse battery + seeding SQL
│       ├── sqlite_it.rs
│       ├── mysql_it.rs
│       ├── postgres_it.rs
│       └── redis_it.rs
├── src/
│   ├── main.ts                  # createApp + pinia
│   ├── App.vue                  # launcher vs workspace switch
│   ├── style.css                # tailwind + design tokens
│   ├── lib/api.ts               # typed invoke wrappers + shared TS types
│   ├── lib/virtual.ts           # visibleRange() for the grid
│   ├── stores/connections.ts
│   ├── stores/workspace.ts
│   └── components/
│       ├── ConnectionLauncher.vue
│       ├── ConnectionForm.vue
│       ├── SchemaSidebar.vue
│       ├── DataGrid.vue
│       ├── KeyBrowser.vue
│       └── RedisValuePane.vue
├── tests/                       # Vitest
│   ├── connections.store.test.ts
│   └── virtual.test.ts
├── docker-compose.test.yml
└── docs/superpowers/...
```

---

### Task 1: Scaffold Tauri v2 + Vue 3 + Tailwind + Pinia + Vitest

**Files:**
- Create: entire app scaffold at repo root (template output), `vite.config.ts` edits, `src/style.css`, `tests/smoke.test.ts`
- Modify: `src-tauri/tauri.conf.json`, `package.json`

**Interfaces:**
- Produces: a running `npm run tauri dev` app; `npm run test` runs Vitest; lib crate named `dbclient_lib`.

- [ ] **Step 1: Scaffold via create-tauri-app into a temp dir, merge into repo root**

```bash
cd ~/Development/dbclient
npm create tauri-app@latest tmp-scaffold -- --template vue-ts --manager npm --yes
rsync -a tmp-scaffold/ ./
rm -rf tmp-scaffold
npm install
```

- [ ] **Step 2: Set identifier and product name**

In `src-tauri/tauri.conf.json` set `"productName": "dbclient"`, `"identifier": "com.arcris.dbclient"`. In `src-tauri/Cargo.toml` confirm `[lib] name = "dbclient_lib"` (rename to this if the template used another name, and update `src-tauri/src/main.rs` accordingly).

- [ ] **Step 3: Add frontend deps and wire Tailwind v4 + Vitest**

```bash
npm i pinia
npm i -D tailwindcss @tailwindcss/vite vitest jsdom @vue/test-utils
```

`vite.config.ts` — add the plugin and test block:

```ts
import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [vue(), tailwindcss()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  test: { environment: "jsdom", include: ["tests/**/*.test.ts"] },
});
```

Replace `src/style.css` content with:

```css
@import "tailwindcss";

:root {
  color-scheme: dark;
}
html, body, #app { height: 100%; }
body {
  @apply bg-zinc-950 text-zinc-200 text-[13px] antialiased;
  font-family: -apple-system, "SF Pro Text", Inter, sans-serif;
}
```

Add to `package.json` scripts: `"test": "vitest run"`.

- [ ] **Step 4: Add a smoke test and run it**

`tests/smoke.test.ts`:

```ts
import { describe, it, expect } from "vitest";
describe("smoke", () => {
  it("runs", () => expect(1 + 1).toBe(2));
});
```

Run: `npm run test` — Expected: 1 passed.

- [ ] **Step 5: Verify the app builds and launches**

Run: `npm run tauri dev` — Expected: native window opens with the template page. Close it. Then `cd src-tauri && cargo test` — Expected: compiles, 0 tests.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "chore: scaffold tauri v2 + vue3 + tailwind + pinia + vitest"
```

---

### Task 2: Rust error module

**Files:**
- Create: `src-tauri/src/error.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod error;`), `src-tauri/Cargo.toml`

**Interfaces:**
- Produces: `AppError { kind: ErrorKind, message: String, detail: Option<String> }`, constructors `AppError::connection(msg)`, `AppError::query(msg)`, `AppError::internal(msg)`, `AppError::timeout(msg)`; `From<sqlx::Error>`, `From<redis::RedisError>`, `From<std::io::Error>`, `From<serde_json::Error>`. Serializes camelCase with lowercase-camel `kind` values (`connectionFailed`, …).

- [ ] **Step 1: Add deps**

In `src-tauri/Cargo.toml` under `[dependencies]` add:

```toml
thiserror = "2"
async-trait = "0.1"
uuid = { version = "1", features = ["v4"] }
tokio = { version = "1", features = ["full"] }
sqlx = { version = "0.8", default-features = false, features = ["runtime-tokio", "tls-rustls", "mysql", "postgres", "sqlite", "chrono", "json"] }
redis = { version = "0.27", features = ["tokio-comp"] }
keyring = { version = "3", features = ["apple-native"] }
base64 = "0.22"
chrono = { version = "0.4", features = ["serde"] }
```

- [ ] **Step 2: Write failing test inside `error.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_to_kind_message_detail() {
        let e = AppError::query("syntax error").with_detail("near SELECT");
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "queryError");
        assert_eq!(v["message"], "syntax error");
        assert_eq!(v["detail"], "near SELECT");
    }

    #[test]
    fn maps_sqlx_error() {
        let e: AppError = sqlx::Error::PoolTimedOut.into();
        assert!(matches!(e.kind, ErrorKind::Timeout));
    }
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `cd src-tauri && cargo test error` — Expected: compile FAIL (AppError not defined).

- [ ] **Step 4: Implement `error.rs`**

```rust
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    ConnectionFailed,
    TunnelFailed,
    QueryError,
    ConflictOnCommit,
    Timeout,
    Internal,
}

#[derive(Debug, Serialize, thiserror::Error)]
#[error("{message}")]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub kind: ErrorKind,
    pub message: String,
    pub detail: Option<String>,
}

impl AppError {
    fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into(), detail: None }
    }
    pub fn connection(m: impl Into<String>) -> Self { Self::new(ErrorKind::ConnectionFailed, m) }
    pub fn query(m: impl Into<String>) -> Self { Self::new(ErrorKind::QueryError, m) }
    pub fn timeout(m: impl Into<String>) -> Self { Self::new(ErrorKind::Timeout, m) }
    pub fn internal(m: impl Into<String>) -> Self { Self::new(ErrorKind::Internal, m) }
    pub fn with_detail(mut self, d: impl Into<String>) -> Self {
        self.detail = Some(d.into());
        self
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        match &e {
            sqlx::Error::PoolTimedOut => AppError::timeout("database connection timed out"),
            sqlx::Error::Io(_) | sqlx::Error::Tls(_) | sqlx::Error::Configuration(_) => {
                AppError::connection(e.to_string())
            }
            sqlx::Error::Database(db) => AppError::query(db.message().to_string()),
            _ => AppError::query(e.to_string()),
        }
    }
}

impl From<redis::RedisError> for AppError {
    fn from(e: redis::RedisError) -> Self {
        if e.is_connection_refusal() || e.is_io_error() {
            AppError::connection(e.to_string())
        } else {
            AppError::query(e.to_string())
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self { AppError::internal(e.to_string()) }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self { AppError::internal(e.to_string()) }
}
```

Add `pub mod error;` at the top of `src-tauri/src/lib.rs`.

- [ ] **Step 5: Run tests**

Run: `cd src-tauri && cargo test error` — Expected: 2 passed.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat: typed AppError with kind/message/detail and sqlx/redis mapping"
```

---

### Task 3: Connection config model + JSON ConfigStore

**Files:**
- Create: `src-tauri/src/config.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod config;`), `src-tauri/Cargo.toml` (add `tempfile = "3"` under `[dev-dependencies]`)

**Interfaces:**
- Produces: `Engine { Mysql, Postgres, Sqlite, Redis }` (serialized `"mysql" | "postgres" | "sqlite" | "redis"`), `EnvColor { Gray, Blue, Green, Amber, Red }` (lowercase), `ConnectionConfig { id, name, engine, color, host: Option<String>, port: Option<u16>, username: Option<String>, database: Option<String>, is_production: bool }`, and `ConfigStore::new(path)`, `.list() -> Vec<ConnectionConfig>`, `.upsert(cfg)`, `.delete(id)` — all returning `Result<_, AppError>`.

- [ ] **Step 1: Write failing tests in `config.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(id: &str, name: &str) -> ConnectionConfig {
        ConnectionConfig {
            id: id.into(), name: name.into(), engine: Engine::Sqlite,
            color: EnvColor::Gray, host: None, port: None, username: None,
            database: Some("/tmp/x.db".into()), is_production: false,
        }
    }

    #[test]
    fn upsert_list_delete_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().join("connections.json"));
        assert!(store.list().unwrap().is_empty());
        store.upsert(cfg("a", "one")).unwrap();
        store.upsert(cfg("b", "two")).unwrap();
        store.upsert(cfg("a", "one-renamed")).unwrap(); // upsert replaces
        let all = store.list().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all.iter().find(|c| c.id == "a").unwrap().name, "one-renamed");
        store.delete("a").unwrap();
        assert_eq!(store.list().unwrap().len(), 1);
    }

    #[test]
    fn engine_serializes_lowercase() {
        assert_eq!(serde_json::to_value(Engine::Postgres).unwrap(), "postgres");
        assert_eq!(serde_json::to_value(EnvColor::Red).unwrap(), "red");
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd src-tauri && cargo test config` — Expected: compile FAIL.

- [ ] **Step 3: Implement `config.rs`**

```rust
use crate::error::AppError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Engine { Mysql, Postgres, Sqlite, Redis }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EnvColor { Gray, Blue, Green, Amber, Red }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionConfig {
    pub id: String,
    pub name: String,
    pub engine: Engine,
    pub color: EnvColor,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    /// database name for MySQL/Postgres, file path for SQLite, db index (as string) for Redis
    pub database: Option<String>,
    pub is_production: bool,
}

pub struct ConfigStore { path: PathBuf }

impl ConfigStore {
    pub fn new(path: PathBuf) -> Self { Self { path } }

    pub fn list(&self) -> Result<Vec<ConnectionConfig>, AppError> {
        if !self.path.exists() { return Ok(vec![]); }
        let raw = std::fs::read_to_string(&self.path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn upsert(&self, cfg: ConnectionConfig) -> Result<(), AppError> {
        let mut all = self.list()?;
        all.retain(|c| c.id != cfg.id);
        all.push(cfg);
        self.write(&all)
    }

    pub fn delete(&self, id: &str) -> Result<(), AppError> {
        let mut all = self.list()?;
        all.retain(|c| c.id != id);
        self.write(&all)
    }

    fn write(&self, all: &[ConnectionConfig]) -> Result<(), AppError> {
        if let Some(dir) = self.path.parent() { std::fs::create_dir_all(dir)?; }
        std::fs::write(&self.path, serde_json::to_string_pretty(all)?)?;
        Ok(())
    }
}
```

Add `pub mod config;` to `lib.rs` and `tempfile = "3"` to `[dev-dependencies]`.

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test config` — Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: connection config model + JSON config store"
```

---

### Task 4: Credential store (Keychain via keyring)

**Files:**
- Create: `src-tauri/src/credentials.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod credentials;`)

**Interfaces:**
- Produces: `CredentialStore::set(conn_id, secret)`, `::get(conn_id) -> Option<String>`, `::delete(conn_id)` — associated fns, `Result<_, AppError>`. Service name is `com.arcris.dbclient`.

- [ ] **Step 1: Write failing tests in `credentials.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_delete_roundtrip() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        CredentialStore::set("conn-1", "s3cret").unwrap();
        assert_eq!(CredentialStore::get("conn-1").unwrap(), Some("s3cret".into()));
        CredentialStore::delete("conn-1").unwrap();
        assert_eq!(CredentialStore::get("conn-1").unwrap(), None);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd src-tauri && cargo test credentials` — Expected: compile FAIL.

- [ ] **Step 3: Implement `credentials.rs`**

```rust
use crate::error::AppError;

const SERVICE: &str = "com.arcris.dbclient";

pub struct CredentialStore;

impl CredentialStore {
    fn entry(conn_id: &str) -> Result<keyring::Entry, AppError> {
        keyring::Entry::new(SERVICE, conn_id).map_err(|e| AppError::internal(e.to_string()))
    }

    pub fn set(conn_id: &str, secret: &str) -> Result<(), AppError> {
        Self::entry(conn_id)?
            .set_password(secret)
            .map_err(|e| AppError::internal(e.to_string()))
    }

    pub fn get(conn_id: &str) -> Result<Option<String>, AppError> {
        match Self::entry(conn_id)?.get_password() {
            Ok(p) => Ok(Some(p)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AppError::internal(e.to_string())),
        }
    }

    pub fn delete(conn_id: &str) -> Result<(), AppError> {
        match Self::entry(conn_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AppError::internal(e.to_string())),
        }
    }
}
```

Add `pub mod credentials;` to `lib.rs`.

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test credentials` — Expected: 1 passed.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: keychain credential store"
```

---

### Task 5: Driver traits, shared types, and SELECT builder

**Files:**
- Create: `src-tauri/src/drivers/mod.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod drivers;`)

**Interfaces:**
- Produces (used by every driver and by commands):

```rust
pub struct TableInfo { pub schema: Option<String>, pub name: String, pub kind: TableKind } // TableKind::{Table,View}, camelCase
pub struct ColumnMeta { pub name: String, pub data_type: String }
pub struct RowsPage { pub columns: Vec<ColumnMeta>, pub rows: Vec<Vec<serde_json::Value>>, pub total_estimate: Option<i64> }
pub enum FilterOp { Contains, Equals, IsNull, NotNull }  // camelCase
pub struct Filter { pub column: String, pub op: FilterOp, pub value: String }
pub struct Sort { pub column: String, pub desc: bool }
pub struct FetchOptions { pub limit: i64, pub offset: i64, pub sort: Option<Sort>, pub filters: Vec<Filter> }

#[async_trait::async_trait]
pub trait SqlDriver: Send + Sync {
    async fn ping(&self) -> Result<(), AppError>;
    async fn list_tables(&self) -> Result<Vec<TableInfo>, AppError>;
    async fn fetch_rows(&self, table: &TableInfo, opts: &FetchOptions) -> Result<RowsPage, AppError>;
}

#[async_trait::async_trait]
pub trait KvDriver: Send + Sync {
    async fn ping(&self) -> Result<(), AppError>;
    async fn scan_keys(&self, pattern: &str, cursor: u64, count: u32) -> Result<KeyScanPage, AppError>;
    async fn get_value(&self, key: &str) -> Result<RedisEntry, AppError>;
}

pub struct KeyScanPage { pub keys: Vec<String>, pub cursor: u64 } // cursor 0 = done

```

`build_select` renders `SELECT * FROM <qualified> [WHERE ...] [ORDER BY ...] LIMIT <n> OFFSET <n>`. Text filters compare on a to-text cast of the column so the same builder works for every engine; each driver supplies its own cast syntax (`CAST(x AS TEXT)` for Postgres/SQLite, `CAST(x AS CHAR)` for MySQL) via the `cast_text` fn. Binding signature:

```rust
pub fn build_select(
    table: &TableInfo,
    opts: &FetchOptions,
    quote: fn(&str) -> String,        // identifier quoting
    cast_text: fn(&str) -> String,    // wraps an already-quoted column in a to-text cast
    placeholder: fn(usize) -> String, // 1-based index -> "?" or "$1"
) -> (String, Vec<String>)
```

- [ ] **Step 1: Write failing tests in `drivers/mod.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn q(s: &str) -> String { format!("\"{}\"", s) }
    fn c(s: &str) -> String { format!("CAST({s} AS TEXT)") }
    fn dollar(i: usize) -> String { format!("${i}") }
    fn qmark(_: usize) -> String { "?".into() }

    fn t() -> TableInfo {
        TableInfo { schema: Some("public".into()), name: "users".into(), kind: TableKind::Table }
    }

    #[test]
    fn plain_select_with_paging() {
        let opts = FetchOptions { limit: 500, offset: 1000, sort: None, filters: vec![] };
        let (sql, binds) = build_select(&t(), &opts, q, c, dollar);
        assert_eq!(sql, r#"SELECT * FROM "public"."users" LIMIT 500 OFFSET 1000"#);
        assert!(binds.is_empty());
    }

    #[test]
    fn filters_sort_and_placeholders() {
        let opts = FetchOptions {
            limit: 500, offset: 0,
            sort: Some(Sort { column: "id".into(), desc: true }),
            filters: vec![
                Filter { column: "email".into(), op: FilterOp::Contains, value: "gmail".into() },
                Filter { column: "name".into(), op: FilterOp::Equals, value: "bob".into() },
                Filter { column: "deleted_at".into(), op: FilterOp::IsNull, value: String::new() },
            ],
        };
        let (sql, binds) = build_select(&t(), &opts, q, c, dollar);
        assert_eq!(
            sql,
            r#"SELECT * FROM "public"."users" WHERE CAST("email" AS TEXT) LIKE $1 AND CAST("name" AS TEXT) = $2 AND "deleted_at" IS NULL ORDER BY "id" DESC LIMIT 500 OFFSET 0"#
        );
        assert_eq!(binds, vec!["%gmail%".to_string(), "bob".to_string()]);
    }

    #[test]
    fn qmark_placeholders_and_no_schema() {
        let table = TableInfo { schema: None, name: "t".into(), kind: TableKind::Table };
        let opts = FetchOptions {
            limit: 10, offset: 0, sort: None,
            filters: vec![Filter { column: "a".into(), op: FilterOp::Equals, value: "1".into() }],
        };
        let (sql, _) = build_select(&table, &opts, q, c, qmark);
        assert_eq!(sql, r#"SELECT * FROM "t" WHERE CAST("a" AS TEXT) = ? LIMIT 10 OFFSET 0"#);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd src-tauri && cargo test drivers` — Expected: compile FAIL.

- [ ] **Step 3: Implement `drivers/mod.rs`**

```rust
pub mod keytree;
pub mod mysql;
pub mod postgres;
pub mod redis_kv;
pub mod sqlite;

use crate::error::AppError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TableKind { Table, View }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableInfo {
    pub schema: Option<String>,
    pub name: String,
    pub kind: TableKind,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnMeta {
    pub name: String,
    pub data_type: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowsPage {
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub total_estimate: Option<i64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FilterOp { Contains, Equals, IsNull, NotNull }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
    pub column: String,
    pub op: FilterOp,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sort {
    pub column: String,
    pub desc: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchOptions {
    pub limit: i64,
    pub offset: i64,
    pub sort: Option<Sort>,
    pub filters: Vec<Filter>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyScanPage {
    pub keys: Vec<String>,
    pub cursor: u64,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RedisValueDto {
    String { value: String },
    Hash { fields: Vec<(String, String)> },
    List { items: Vec<String> },
    Set { members: Vec<String> },
    ZSet { members: Vec<(String, f64)> },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisEntry {
    pub key: String,
    /// seconds; -1 = no TTL, -2 = key missing
    pub ttl_secs: i64,
    pub value: RedisValueDto,
}

#[async_trait::async_trait]
pub trait SqlDriver: Send + Sync {
    async fn ping(&self) -> Result<(), AppError>;
    async fn list_tables(&self) -> Result<Vec<TableInfo>, AppError>;
    async fn fetch_rows(&self, table: &TableInfo, opts: &FetchOptions) -> Result<RowsPage, AppError>;
}

#[async_trait::async_trait]
pub trait KvDriver: Send + Sync {
    async fn ping(&self) -> Result<(), AppError>;
    async fn scan_keys(&self, pattern: &str, cursor: u64, count: u32) -> Result<KeyScanPage, AppError>;
    async fn get_value(&self, key: &str) -> Result<RedisEntry, AppError>;
}

pub fn build_select(
    table: &TableInfo,
    opts: &FetchOptions,
    quote: fn(&str) -> String,
    cast_text: fn(&str) -> String,
    placeholder: fn(usize) -> String,
) -> (String, Vec<String>) {
    let qualified = match &table.schema {
        Some(s) => format!("{}.{}", quote(s), quote(&table.name)),
        None => quote(&table.name),
    };
    let mut sql = format!("SELECT * FROM {qualified}");
    let mut binds: Vec<String> = vec![];
    let mut clauses: Vec<String> = vec![];
    for f in &opts.filters {
        let col = quote(&f.column);
        match f.op {
            FilterOp::Contains => {
                binds.push(format!("%{}%", f.value));
                clauses.push(format!("{} LIKE {}", cast_text(&col), placeholder(binds.len())));
            }
            FilterOp::Equals => {
                binds.push(f.value.clone());
                clauses.push(format!("{} = {}", cast_text(&col), placeholder(binds.len())));
            }
            FilterOp::IsNull => clauses.push(format!("{col} IS NULL")),
            FilterOp::NotNull => clauses.push(format!("{col} IS NOT NULL")),
        }
    }
    if !clauses.is_empty() {
        sql.push_str(&format!(" WHERE {}", clauses.join(" AND ")));
    }
    if let Some(sort) = &opts.sort {
        sql.push_str(&format!(
            " ORDER BY {} {}",
            quote(&sort.column),
            if sort.desc { "DESC" } else { "ASC" }
        ));
    }
    sql.push_str(&format!(" LIMIT {} OFFSET {}", opts.limit, opts.offset));
    (sql, binds)
}
```

Create empty placeholder files so the module compiles: `sqlite.rs`, `mysql.rs`, `postgres.rs`, `redis_kv.rs`, `keytree.rs` (each may be empty for now). Add `pub mod drivers;` to `lib.rs`.

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test drivers` — Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: driver traits, shared row types, engine-neutral SELECT builder"
```

---

### Task 6: SQLite driver + shared browse battery

**Files:**
- Create: `src-tauri/src/drivers/sqlite.rs`, `src-tauri/tests/common/mod.rs`, `src-tauri/tests/sqlite_it.rs`

**Interfaces:**
- Consumes: `SqlDriver`, `build_select`, `AppError`.
- Produces: `SqliteDriver::connect(path: &str) -> Result<SqliteDriver, AppError>` (path `":memory:"` allowed); battery fn `common::assert_browse_battery(driver: &dyn SqlDriver)` and `common::SEED_SQL: &[&str]` used by every SQL engine's integration test. Seed schema: table `people(id integer primary key, name text/varchar, email text/varchar, age integer)` with rows `(1,'Alice','alice@gmail.com',30)`, `(2,'Bob','bob@yahoo.com',25)`, `(3,'Carol','carol@gmail.com',41)`.

- [ ] **Step 1: Write the battery in `tests/common/mod.rs`**

```rust
use dbclient_lib::drivers::{FetchOptions, Filter, FilterOp, Sort, SqlDriver};

pub const SEED_SQL: &[&str] = &[
    "DROP TABLE IF EXISTS people",
    "CREATE TABLE people (id INTEGER PRIMARY KEY, name VARCHAR(64), email VARCHAR(128), age INTEGER)",
    "INSERT INTO people (id, name, email, age) VALUES (1, 'Alice', 'alice@gmail.com', 30)",
    "INSERT INTO people (id, name, email, age) VALUES (2, 'Bob', 'bob@yahoo.com', 25)",
    "INSERT INTO people (id, name, email, age) VALUES (3, 'Carol', 'carol@gmail.com', 41)",
];

fn no_opts() -> FetchOptions {
    FetchOptions { limit: 100, offset: 0, sort: None, filters: vec![] }
}

pub async fn assert_browse_battery(d: &dyn SqlDriver) {
    d.ping().await.expect("ping");

    let tables = d.list_tables().await.expect("list_tables");
    let people = tables.iter().find(|t| t.name == "people").expect("people table listed");

    // full read
    let page = d.fetch_rows(people, &no_opts()).await.expect("fetch");
    assert_eq!(page.rows.len(), 3);
    let name_idx = page.columns.iter().position(|c| c.name == "name").expect("name col");

    // sort desc by id -> Carol first
    let sorted = d.fetch_rows(people, &FetchOptions {
        sort: Some(Sort { column: "id".into(), desc: true }), ..no_opts()
    }).await.expect("sorted fetch");
    assert_eq!(sorted.rows[0][name_idx], serde_json::json!("Carol"));

    // filter contains
    let filtered = d.fetch_rows(people, &FetchOptions {
        filters: vec![Filter { column: "email".into(), op: FilterOp::Contains, value: "gmail".into() }],
        ..no_opts()
    }).await.expect("filtered fetch");
    assert_eq!(filtered.rows.len(), 2);

    // pagination
    let page2 = d.fetch_rows(people, &FetchOptions {
        limit: 2, offset: 2,
        sort: Some(Sort { column: "id".into(), desc: false }), ..no_opts()
    }).await.expect("paged fetch");
    assert_eq!(page2.rows.len(), 1);

    // errors are typed
    let missing = dbclient_lib::drivers::TableInfo {
        schema: None, name: "no_such_table".into(),
        kind: dbclient_lib::drivers::TableKind::Table,
    };
    let err = d.fetch_rows(&missing, &no_opts()).await.unwrap_err();
    assert_eq!(err.kind, dbclient_lib::error::ErrorKind::QueryError);
}
```

- [ ] **Step 2: Write `tests/sqlite_it.rs`**

```rust
mod common;
use dbclient_lib::drivers::sqlite::SqliteDriver;

#[tokio::test]
async fn sqlite_browse_battery() {
    let d = SqliteDriver::connect(":memory:").await.unwrap();
    for sql in common::SEED_SQL {
        d.execute_raw(sql).await.unwrap(); // test-support helper on the driver
    }
    common::assert_browse_battery(&d).await;
}
```

- [ ] **Step 3: Run to verify failure**

Run: `cd src-tauri && cargo test --test sqlite_it` — Expected: compile FAIL (SqliteDriver not defined).

- [ ] **Step 4: Implement `drivers/sqlite.rs`**

```rust
use super::{build_select, ColumnMeta, FetchOptions, RowsPage, SqlDriver, TableInfo, TableKind};
use crate::error::AppError;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow};
use sqlx::{Column, Row, SqlitePool};
use std::str::FromStr;

pub struct SqliteDriver { pool: SqlitePool }

fn quote(s: &str) -> String { format!("\"{}\"", s.replace('"', "\"\"")) }
fn cast_text(quoted: &str) -> String { format!("CAST({quoted} AS TEXT)") }
fn qmark(_: usize) -> String { "?".into() }

impl SqliteDriver {
    pub async fn connect(path: &str) -> Result<Self, AppError> {
        let opts = SqliteConnectOptions::from_str(&format!("sqlite:{path}"))
            .map_err(|e| AppError::connection(e.to_string()))?
            .create_if_missing(false);
        let opts = if path == ":memory:" { opts.create_if_missing(true) } else { opts };
        let pool = SqlitePoolOptions::new().max_connections(1).connect_with(opts).await?;
        Ok(Self { pool })
    }

    /// test support: run arbitrary DDL/DML (not exposed as a command in M1)
    pub async fn execute_raw(&self, sql: &str) -> Result<(), AppError> {
        sqlx::query(sql).execute(&self.pool).await?;
        Ok(())
    }
}

fn cell(row: &SqliteRow, i: usize) -> serde_json::Value {
    use base64::Engine as _;
    if let Ok(v) = row.try_get::<Option<i64>, _>(i) {
        return v.map(serde_json::Value::from).unwrap_or(serde_json::Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<f64>, _>(i) {
        return v.map(serde_json::Value::from).unwrap_or(serde_json::Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<String>, _>(i) {
        return v.map(serde_json::Value::from).unwrap_or(serde_json::Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(i) {
        return v
            .map(|b| serde_json::Value::from(base64::engine::general_purpose::STANDARD.encode(b)))
            .unwrap_or(serde_json::Value::Null);
    }
    serde_json::Value::Null
}

#[async_trait::async_trait]
impl SqlDriver for SqliteDriver {
    async fn ping(&self) -> Result<(), AppError> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    async fn list_tables(&self) -> Result<Vec<TableInfo>, AppError> {
        let rows = sqlx::query(
            "SELECT name, type FROM sqlite_master WHERE type IN ('table','view') \
             AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| TableInfo {
            schema: None,
            name: r.get::<String, _>(0),
            kind: if r.get::<String, _>(1) == "view" { TableKind::View } else { TableKind::Table },
        }).collect())
    }

    async fn fetch_rows(&self, table: &TableInfo, opts: &FetchOptions) -> Result<RowsPage, AppError> {
        let (sql, binds) = build_select(table, opts, quote, cast_text, qmark);
        let mut q = sqlx::query(&sql);
        for b in &binds { q = q.bind(b); }
        let rows = q.fetch_all(&self.pool).await?;

        let columns: Vec<ColumnMeta> = rows.first().map(|r| {
            r.columns().iter()
                .map(|c| ColumnMeta { name: c.name().to_string(), data_type: format!("{:?}", c.type_info()) })
                .collect()
        }).unwrap_or_default();

        let count_sql = format!("SELECT COUNT(*) FROM {}", quote(&table.name));
        let total: i64 = sqlx::query_scalar(&count_sql).fetch_one(&self.pool).await?;

        Ok(RowsPage {
            columns,
            rows: rows.iter().map(|r| (0..r.columns().len()).map(|i| cell(r, i)).collect()).collect(),
            total_estimate: Some(total),
        })
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cd src-tauri && cargo test --test sqlite_it` — Expected: 1 passed.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat: sqlite driver + shared browse battery"
```

---

### Task 7: MySQL + Postgres drivers, docker-compose for tests

**Files:**
- Create: `src-tauri/src/drivers/mysql.rs`, `src-tauri/src/drivers/postgres.rs`, `src-tauri/tests/mysql_it.rs`, `src-tauri/tests/postgres_it.rs`, `docker-compose.test.yml`

**Interfaces:**
- Consumes: `SqlDriver`, `build_select`, battery from Task 6.
- Produces: `MysqlDriver::connect_url(url) -> Result<Self, AppError>` and `MysqlDriver::connect(host, port, user, password, db)`; same pair on `PostgresDriver`. Both expose `execute_raw(sql)` (test support).

- [ ] **Step 1: Write `docker-compose.test.yml`**

```yaml
services:
  mysql:
    image: mysql:8.4
    environment:
      MYSQL_ROOT_PASSWORD: test
      MYSQL_DATABASE: dbclient_test
    ports: ["33061:3306"]
  postgres:
    image: postgres:16
    environment:
      POSTGRES_PASSWORD: test
      POSTGRES_DB: dbclient_test
    ports: ["54321:5432"]
  redis:
    image: redis:7
    ports: ["63791:6379"]
```

- [ ] **Step 2: Write integration tests (env-gated, skip when unset)**

`src-tauri/tests/mysql_it.rs`:

```rust
mod common;
use dbclient_lib::drivers::mysql::MysqlDriver;

#[tokio::test]
async fn mysql_browse_battery() {
    let Ok(url) = std::env::var("DBCLIENT_TEST_MYSQL_URL") else {
        eprintln!("skipping: DBCLIENT_TEST_MYSQL_URL not set");
        return;
    };
    let d = MysqlDriver::connect_url(&url).await.unwrap();
    for sql in common::SEED_SQL { d.execute_raw(sql).await.unwrap(); }
    common::assert_browse_battery(&d).await;
}
```

`src-tauri/tests/postgres_it.rs` — identical shape with `DBCLIENT_TEST_PG_URL` and `PostgresDriver`:

```rust
mod common;
use dbclient_lib::drivers::postgres::PostgresDriver;

#[tokio::test]
async fn postgres_browse_battery() {
    let Ok(url) = std::env::var("DBCLIENT_TEST_PG_URL") else {
        eprintln!("skipping: DBCLIENT_TEST_PG_URL not set");
        return;
    };
    let d = PostgresDriver::connect_url(&url).await.unwrap();
    for sql in common::SEED_SQL { d.execute_raw(sql).await.unwrap(); }
    common::assert_browse_battery(&d).await;
}
```

- [ ] **Step 3: Run to verify failure**

Run: `cd src-tauri && cargo test --test mysql_it` — Expected: compile FAIL.

- [ ] **Step 4: Implement `drivers/mysql.rs`**

```rust
use super::{build_select, ColumnMeta, FetchOptions, RowsPage, SqlDriver, TableInfo, TableKind};
use crate::error::AppError;
use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions, MySqlRow};
use sqlx::{Column, MySqlPool, Row};
use std::str::FromStr;

pub struct MysqlDriver { pool: MySqlPool, database: String }

fn quote(s: &str) -> String { format!("`{}`", s.replace('`', "``")) }
fn cast_text(quoted: &str) -> String { format!("CAST({quoted} AS CHAR)") }
fn qmark(_: usize) -> String { "?".into() }

impl MysqlDriver {
    pub async fn connect(host: &str, port: u16, user: &str, password: &str, db: &str) -> Result<Self, AppError> {
        let opts = MySqlConnectOptions::new()
            .host(host).port(port).username(user).password(password).database(db);
        Self::from_opts(opts, db).await
    }

    pub async fn connect_url(url: &str) -> Result<Self, AppError> {
        let opts = MySqlConnectOptions::from_str(url).map_err(|e| AppError::connection(e.to_string()))?;
        let db = opts.get_database().unwrap_or_default().to_string();
        Self::from_opts(opts, &db).await
    }

    async fn from_opts(opts: MySqlConnectOptions, db: &str) -> Result<Self, AppError> {
        let pool = MySqlPoolOptions::new().max_connections(4)
            .acquire_timeout(std::time::Duration::from_secs(8))
            .connect_with(opts).await?;
        Ok(Self { pool, database: db.to_string() })
    }

    pub async fn execute_raw(&self, sql: &str) -> Result<(), AppError> {
        sqlx::query(sql).execute(&self.pool).await?;
        Ok(())
    }
}

fn cell(row: &MySqlRow, i: usize) -> serde_json::Value {
    use base64::Engine as _;
    if let Ok(v) = row.try_get::<Option<i64>, _>(i) {
        return v.map(serde_json::Value::from).unwrap_or(serde_json::Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<f64>, _>(i) {
        return v.map(serde_json::Value::from).unwrap_or(serde_json::Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<String>, _>(i) {
        return v.map(serde_json::Value::from).unwrap_or(serde_json::Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<chrono::NaiveDateTime>, _>(i) {
        return v.map(|d| serde_json::Value::from(d.to_string())).unwrap_or(serde_json::Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<chrono::NaiveDate>, _>(i) {
        return v.map(|d| serde_json::Value::from(d.to_string())).unwrap_or(serde_json::Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<serde_json::Value>, _>(i) {
        return v.unwrap_or(serde_json::Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(i) {
        return v
            .map(|b| serde_json::Value::from(base64::engine::general_purpose::STANDARD.encode(b)))
            .unwrap_or(serde_json::Value::Null);
    }
    serde_json::Value::Null
}

#[async_trait::async_trait]
impl SqlDriver for MysqlDriver {
    async fn ping(&self) -> Result<(), AppError> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    async fn list_tables(&self) -> Result<Vec<TableInfo>, AppError> {
        let rows = sqlx::query(
            "SELECT table_name, table_type FROM information_schema.tables \
             WHERE table_schema = ? ORDER BY table_name",
        )
        .bind(&self.database)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| TableInfo {
            schema: None, // single-database connection; schema qualifier unnecessary
            name: r.get::<String, _>(0),
            kind: if r.get::<String, _>(1) == "VIEW" { TableKind::View } else { TableKind::Table },
        }).collect())
    }

    async fn fetch_rows(&self, table: &TableInfo, opts: &FetchOptions) -> Result<RowsPage, AppError> {
        let (sql, binds) = build_select(table, opts, quote, cast_text, qmark);
        let mut q = sqlx::query(&sql);
        for b in &binds { q = q.bind(b); }
        let rows = q.fetch_all(&self.pool).await?;

        let columns: Vec<ColumnMeta> = rows.first().map(|r| {
            r.columns().iter()
                .map(|c| ColumnMeta { name: c.name().to_string(), data_type: c.type_info().to_string() })
                .collect()
        }).unwrap_or_default();

        let total: Option<i64> = sqlx::query_scalar(
            "SELECT table_rows FROM information_schema.tables WHERE table_schema = ? AND table_name = ?",
        )
        .bind(&self.database).bind(&table.name)
        .fetch_optional(&self.pool).await?
        .flatten();

        Ok(RowsPage {
            columns,
            rows: rows.iter().map(|r| (0..r.columns().len()).map(|i| cell(r, i)).collect()).collect(),
            total_estimate: total,
        })
    }
}
```

Note: `information_schema.table_rows` is an estimate on InnoDB — that is exactly what the spec asks for on large tables.

- [ ] **Step 5: Implement `drivers/postgres.rs`**

```rust
use super::{build_select, ColumnMeta, FetchOptions, RowsPage, SqlDriver, TableInfo, TableKind};
use crate::error::AppError;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgRow};
use sqlx::{Column, PgPool, Row};
use std::str::FromStr;

pub struct PostgresDriver { pool: PgPool }

fn quote(s: &str) -> String { format!("\"{}\"", s.replace('"', "\"\"")) }
fn cast_text(quoted: &str) -> String { format!("CAST({quoted} AS TEXT)") }
fn dollar(i: usize) -> String { format!("${i}") }

impl PostgresDriver {
    pub async fn connect(host: &str, port: u16, user: &str, password: &str, db: &str) -> Result<Self, AppError> {
        let opts = PgConnectOptions::new()
            .host(host).port(port).username(user).password(password).database(db);
        Self::from_opts(opts).await
    }

    pub async fn connect_url(url: &str) -> Result<Self, AppError> {
        let opts = PgConnectOptions::from_str(url).map_err(|e| AppError::connection(e.to_string()))?;
        Self::from_opts(opts).await
    }

    async fn from_opts(opts: PgConnectOptions) -> Result<Self, AppError> {
        let pool = PgPoolOptions::new().max_connections(4)
            .acquire_timeout(std::time::Duration::from_secs(8))
            .connect_with(opts).await?;
        Ok(Self { pool })
    }

    pub async fn execute_raw(&self, sql: &str) -> Result<(), AppError> {
        sqlx::query(sql).execute(&self.pool).await?;
        Ok(())
    }
}

fn cell(row: &PgRow, i: usize) -> serde_json::Value {
    use base64::Engine as _;
    if let Ok(v) = row.try_get::<Option<i64>, _>(i) {
        return v.map(serde_json::Value::from).unwrap_or(serde_json::Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<i32>, _>(i) {
        return v.map(serde_json::Value::from).unwrap_or(serde_json::Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<f64>, _>(i) {
        return v.map(serde_json::Value::from).unwrap_or(serde_json::Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<bool>, _>(i) {
        return v.map(serde_json::Value::from).unwrap_or(serde_json::Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<String>, _>(i) {
        return v.map(serde_json::Value::from).unwrap_or(serde_json::Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<chrono::NaiveDateTime>, _>(i) {
        return v.map(|d| serde_json::Value::from(d.to_string())).unwrap_or(serde_json::Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<chrono::NaiveDate>, _>(i) {
        return v.map(|d| serde_json::Value::from(d.to_string())).unwrap_or(serde_json::Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<serde_json::Value>, _>(i) {
        return v.unwrap_or(serde_json::Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(i) {
        return v
            .map(|b| serde_json::Value::from(base64::engine::general_purpose::STANDARD.encode(b)))
            .unwrap_or(serde_json::Value::Null);
    }
    serde_json::Value::Null
}

#[async_trait::async_trait]
impl SqlDriver for PostgresDriver {
    async fn ping(&self) -> Result<(), AppError> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    async fn list_tables(&self) -> Result<Vec<TableInfo>, AppError> {
        let rows = sqlx::query(
            "SELECT table_schema, table_name, table_type FROM information_schema.tables \
             WHERE table_schema NOT IN ('pg_catalog','information_schema') \
             ORDER BY table_schema, table_name",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| TableInfo {
            schema: Some(r.get::<String, _>(0)),
            name: r.get::<String, _>(1),
            kind: if r.get::<String, _>(2) == "VIEW" { TableKind::View } else { TableKind::Table },
        }).collect())
    }

    async fn fetch_rows(&self, table: &TableInfo, opts: &FetchOptions) -> Result<RowsPage, AppError> {
        let (sql, binds) = build_select(table, opts, quote, cast_text, dollar);
        let mut q = sqlx::query(&sql);
        for b in &binds { q = q.bind(b); }
        let rows = q.fetch_all(&self.pool).await?;

        let columns: Vec<ColumnMeta> = rows.first().map(|r| {
            r.columns().iter()
                .map(|c| ColumnMeta { name: c.name().to_string(), data_type: c.type_info().to_string() })
                .collect()
        }).unwrap_or_default();

        let total: Option<i64> = sqlx::query_scalar::<_, f32>(
            "SELECT reltuples FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = $1 AND c.relname = $2",
        )
        .bind(table.schema.as_deref().unwrap_or("public")).bind(&table.name)
        .fetch_optional(&self.pool).await?
        .map(|estimate| estimate.max(0.0) as i64);

        Ok(RowsPage {
            columns,
            rows: rows.iter().map(|r| (0..r.columns().len()).map(|i| cell(r, i)).collect()).collect(),
            total_estimate: total,
        })
    }
}
```

- [ ] **Step 6: Run against real engines**

```bash
docker compose -f docker-compose.test.yml up -d
sleep 20   # mysql 8.4 cold start
cd src-tauri
DBCLIENT_TEST_MYSQL_URL="mysql://root:test@127.0.0.1:33061/dbclient_test" cargo test --test mysql_it -- --nocapture
DBCLIENT_TEST_PG_URL="postgres://postgres:test@127.0.0.1:54321/dbclient_test" cargo test --test postgres_it -- --nocapture
```

Expected: both `1 passed`. Also run `cargo test --test mysql_it` WITHOUT the env var — Expected: passes with "skipping" printed.

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "feat: mysql + postgres drivers with shared battery, docker test stack"
```

---

### Task 8: Redis driver + key tree

**Files:**
- Create: `src-tauri/src/drivers/redis_kv.rs`, `src-tauri/src/drivers/keytree.rs`, `src-tauri/tests/redis_it.rs`

**Interfaces:**
- Consumes: `KvDriver`, `KeyScanPage`, `RedisEntry`, `RedisValueDto`.
- Produces: `RedisDriver::connect(host, port, password: Option<&str>, db_index: i64) -> Result<Self, AppError>`, `RedisDriver::connect_url(url)`; `keytree::build(keys: &[String]) -> Vec<KeyTreeNode>` with `KeyTreeNode { name: String, full_key: Option<String>, children: Vec<KeyTreeNode>, count: usize }` (camelCase serialized; `count` = number of leaf keys under the node, `full_key` set when the node itself is a key).

- [ ] **Step 1: Write failing keytree tests in `keytree.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_namespace_tree() {
        let keys = vec![
            "cache:users:1".to_string(),
            "cache:users:2".to_string(),
            "cache:posts:9".to_string(),
            "session_abc".to_string(),
        ];
        let tree = build(&keys);
        assert_eq!(tree.len(), 2); // "cache", "session_abc"
        let cache = tree.iter().find(|n| n.name == "cache").unwrap();
        assert_eq!(cache.count, 3);
        assert_eq!(cache.full_key, None);
        let users = cache.children.iter().find(|n| n.name == "users").unwrap();
        assert_eq!(users.count, 2);
        assert_eq!(users.children[0].full_key.as_deref(), Some("cache:users:1"));
        let session = tree.iter().find(|n| n.name == "session_abc").unwrap();
        assert_eq!(session.full_key.as_deref(), Some("session_abc"));
    }

    #[test]
    fn node_can_be_both_key_and_namespace() {
        let keys = vec!["a".to_string(), "a:b".to_string()];
        let tree = build(&keys);
        let a = &tree[0];
        assert_eq!(a.full_key.as_deref(), Some("a"));
        assert_eq!(a.children.len(), 1);
        assert_eq!(a.count, 2);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd src-tauri && cargo test keytree` — Expected: compile FAIL.

- [ ] **Step 3: Implement `keytree.rs`**

```rust
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyTreeNode {
    pub name: String,
    pub full_key: Option<String>,
    pub children: Vec<KeyTreeNode>,
    pub count: usize,
}

#[derive(Default)]
struct Builder {
    is_key: Option<String>,
    children: BTreeMap<String, Builder>,
    count: usize,
}

pub fn build(keys: &[String]) -> Vec<KeyTreeNode> {
    let mut root = Builder::default();
    for key in keys {
        let mut node = &mut root;
        node.count += 1;
        for part in key.split(':') {
            node = node.children.entry(part.to_string()).or_default();
            node.count += 1;
        }
        node.is_key = Some(key.clone());
    }
    finish(root.children)
}

fn finish(children: BTreeMap<String, Builder>) -> Vec<KeyTreeNode> {
    children.into_iter().map(|(name, b)| KeyTreeNode {
        name,
        full_key: b.is_key,
        children: finish(b.children),
        count: b.count,
    }).collect()
}
```

- [ ] **Step 4: Run keytree tests**

Run: `cd src-tauri && cargo test keytree` — Expected: 2 passed.

- [ ] **Step 5: Write env-gated Redis integration test `tests/redis_it.rs`**

```rust
use dbclient_lib::drivers::redis_kv::RedisDriver;
use dbclient_lib::drivers::{KvDriver, RedisValueDto};

#[tokio::test]
async fn redis_scan_and_typed_values() {
    let Ok(url) = std::env::var("DBCLIENT_TEST_REDIS_URL") else {
        eprintln!("skipping: DBCLIENT_TEST_REDIS_URL not set");
        return;
    };
    let d = RedisDriver::connect_url(&url).await.unwrap();
    d.flush_and_seed().await.unwrap(); // test support: FLUSHDB + fixed fixtures

    // scan to exhaustion with a pattern
    let mut keys = vec![];
    let mut cursor = 0u64;
    loop {
        let page = d.scan_keys("fixture:*", cursor, 100).await.unwrap();
        keys.extend(page.keys);
        cursor = page.cursor;
        if cursor == 0 { break; }
    }
    assert_eq!(keys.len(), 4);

    let s = d.get_value("fixture:str").await.unwrap();
    assert_eq!(s.value, RedisValueDto::String { value: "hello".into() });
    assert_eq!(s.ttl_secs, -1);

    let h = d.get_value("fixture:hash").await.unwrap();
    assert!(matches!(h.value, RedisValueDto::Hash { ref fields } if fields.len() == 2));

    let l = d.get_value("fixture:list").await.unwrap();
    assert!(matches!(l.value, RedisValueDto::List { ref items } if items == &vec!["a".to_string(), "b".to_string()]));

    let z = d.get_value("fixture:zset").await.unwrap();
    assert!(matches!(z.value, RedisValueDto::ZSet { ref members } if members.len() == 2));
}
```

- [ ] **Step 6: Implement `drivers/redis_kv.rs`**

```rust
use super::{KeyScanPage, KvDriver, RedisEntry, RedisValueDto};
use crate::error::AppError;
use redis::aio::MultiplexedConnection;
use redis::AsyncCommands;

pub struct RedisDriver { conn: MultiplexedConnection }

impl RedisDriver {
    pub async fn connect(host: &str, port: u16, password: Option<&str>, db_index: i64) -> Result<Self, AppError> {
        let info = redis::ConnectionInfo {
            addr: redis::ConnectionAddr::Tcp(host.to_string(), port),
            redis: redis::RedisConnectionInfo {
                db: db_index,
                username: None,
                password: password.map(String::from),
                protocol: redis::ProtocolVersion::RESP2,
            },
        };
        let client = redis::Client::open(info)?;
        let conn = client.get_multiplexed_async_connection().await?;
        Ok(Self { conn })
    }

    pub async fn connect_url(url: &str) -> Result<Self, AppError> {
        let client = redis::Client::open(url)?;
        let conn = client.get_multiplexed_async_connection().await?;
        Ok(Self { conn })
    }

    /// test support only
    pub async fn flush_and_seed(&self) -> Result<(), AppError> {
        let mut c = self.conn.clone();
        redis::cmd("FLUSHDB").query_async::<()>(&mut c).await?;
        let _: () = c.set("fixture:str", "hello").await?;
        let _: () = c.hset_multiple("fixture:hash", &[("f1", "v1"), ("f2", "v2")]).await?;
        let _: () = c.rpush("fixture:list", &["a", "b"]).await?;
        let _: () = c.zadd_multiple("fixture:zset", &[(1.0, "one"), (2.0, "two")]).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl KvDriver for RedisDriver {
    async fn ping(&self) -> Result<(), AppError> {
        let mut c = self.conn.clone();
        redis::cmd("PING").query_async::<String>(&mut c).await?;
        Ok(())
    }

    async fn scan_keys(&self, pattern: &str, cursor: u64, count: u32) -> Result<KeyScanPage, AppError> {
        let mut c = self.conn.clone();
        let (next, keys): (u64, Vec<String>) = redis::cmd("SCAN")
            .arg(cursor).arg("MATCH").arg(pattern).arg("COUNT").arg(count)
            .query_async(&mut c).await?;
        Ok(KeyScanPage { keys, cursor: next })
    }

    async fn get_value(&self, key: &str) -> Result<RedisEntry, AppError> {
        let mut c = self.conn.clone();
        let ttl: i64 = c.ttl(key).await?;
        let kind: String = redis::cmd("TYPE").arg(key).query_async(&mut c).await?;
        let value = match kind.as_str() {
            "string" => RedisValueDto::String { value: c.get(key).await? },
            "hash" => {
                let map: Vec<(String, String)> = redis::cmd("HGETALL").arg(key).query_async(&mut c).await?;
                RedisValueDto::Hash { fields: map }
            }
            "list" => RedisValueDto::List { items: c.lrange(key, 0, 999).await? },
            "set" => RedisValueDto::Set { members: c.smembers(key).await? },
            "zset" => {
                let members: Vec<(String, f64)> =
                    c.zrange_withscores(key, 0, 999).await?;
                RedisValueDto::ZSet { members }
            }
            other => return Err(AppError::query(format!("unsupported redis type: {other}"))),
        };
        Ok(RedisEntry { key: key.to_string(), ttl_secs: ttl, value })
    }
}
```

Add `PartialEq` derive requirement check: `RedisValueDto` already derives `PartialEq` (Task 5).

- [ ] **Step 7: Run against real Redis**

```bash
cd src-tauri
DBCLIENT_TEST_REDIS_URL="redis://127.0.0.1:63791/0" cargo test --test redis_it -- --nocapture
```

Expected: 1 passed. Without env var: passes with "skipping".

- [ ] **Step 8: Commit**

```bash
git add -A && git commit -m "feat: redis driver (SCAN + typed values) and key namespace tree"
```

---

### Task 9: ConnectionRegistry + Tauri commands + AppState wiring

**Files:**
- Create: `src-tauri/src/registry.rs`, `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs` (modules, AppState, setup, invoke_handler)

**Interfaces:**
- Consumes: everything from Tasks 2–8.
- Produces the full M1 command surface (exact names, frontend depends on these):
  - `list_connections() -> Vec<ConnectionConfig>`
  - `save_connection(config: ConnectionConfig, password: Option<String>) -> ()` — stores password in Keychain when provided
  - `delete_connection(id: String) -> ()` — also deletes Keychain entry and disconnects
  - `test_connection(config: ConnectionConfig, password: Option<String>) -> ()` — opens, pings, drops; falls back to stored Keychain password when `password` is None
  - `connect(id: String) -> String` — returns `"sql"` or `"kv"`
  - `disconnect(id: String) -> ()`
  - `list_tables(id: String) -> Vec<TableInfo>`
  - `fetch_rows(id: String, table: TableInfo, opts: FetchOptions) -> RowsPage`
  - `scan_keys(id: String, pattern: String, cursor: u64) -> KeyScanPage`
  - `get_redis_value(id: String, key: String) -> RedisEntry`
  - `build_key_tree(keys: Vec<String>) -> Vec<KeyTreeNode>`

- [ ] **Step 1: Write failing registry unit test in `registry.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::sqlite::SqliteDriver;

    #[tokio::test]
    async fn insert_get_remove() {
        let reg = ConnectionRegistry::default();
        let d = SqliteDriver::connect(":memory:").await.unwrap();
        reg.insert("c1", LiveConnection::Sql(Box::new(d))).await;
        assert!(reg.get("c1").await.is_some());
        assert!(matches!(&*reg.get("c1").await.unwrap(), LiveConnection::Sql(_)));
        reg.remove("c1").await;
        assert!(reg.get("c1").await.is_none());
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd src-tauri && cargo test registry` — Expected: compile FAIL.

- [ ] **Step 3: Implement `registry.rs`**

```rust
use crate::drivers::{KvDriver, SqlDriver};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

pub enum LiveConnection {
    Sql(Box<dyn SqlDriver>),
    Kv(Box<dyn KvDriver>),
}

#[derive(Default)]
pub struct ConnectionRegistry {
    inner: Mutex<HashMap<String, Arc<LiveConnection>>>,
}

impl ConnectionRegistry {
    pub async fn insert(&self, id: &str, conn: LiveConnection) {
        self.inner.lock().await.insert(id.to_string(), Arc::new(conn));
    }
    pub async fn get(&self, id: &str) -> Option<Arc<LiveConnection>> {
        self.inner.lock().await.get(id).cloned()
    }
    pub async fn remove(&self, id: &str) {
        self.inner.lock().await.remove(id);
    }
}
```

Run: `cd src-tauri && cargo test registry` — Expected: 1 passed.

- [ ] **Step 4: Implement `commands.rs`**

```rust
use crate::config::{ConnectionConfig, Engine};
use crate::credentials::CredentialStore;
use crate::drivers::keytree::{self, KeyTreeNode};
use crate::drivers::mysql::MysqlDriver;
use crate::drivers::postgres::PostgresDriver;
use crate::drivers::redis_kv::RedisDriver;
use crate::drivers::sqlite::SqliteDriver;
use crate::drivers::{FetchOptions, KeyScanPage, RedisEntry, RowsPage, TableInfo};
use crate::error::AppError;
use crate::registry::LiveConnection;
use crate::AppState;
use tauri::State;

async fn open_connection(cfg: &ConnectionConfig, password: Option<&str>) -> Result<LiveConnection, AppError> {
    let host = cfg.host.as_deref().unwrap_or("127.0.0.1");
    let user = cfg.username.as_deref().unwrap_or("");
    let db = cfg.database.as_deref().unwrap_or("");
    let pw = password.unwrap_or("");
    Ok(match cfg.engine {
        Engine::Sqlite => LiveConnection::Sql(Box::new(SqliteDriver::connect(db).await?)),
        Engine::Mysql => LiveConnection::Sql(Box::new(
            MysqlDriver::connect(host, cfg.port.unwrap_or(3306), user, pw, db).await?,
        )),
        Engine::Postgres => LiveConnection::Sql(Box::new(
            PostgresDriver::connect(host, cfg.port.unwrap_or(5432), user, pw, db).await?,
        )),
        Engine::Redis => LiveConnection::Kv(Box::new(
            RedisDriver::connect(host, cfg.port.unwrap_or(6379), password, db.parse().unwrap_or(0)).await?,
        )),
    })
}

fn stored_or_given(id: &str, given: Option<String>) -> Result<Option<String>, AppError> {
    match given {
        Some(p) => Ok(Some(p)),
        None => CredentialStore::get(id),
    }
}

#[tauri::command]
pub async fn list_connections(state: State<'_, AppState>) -> Result<Vec<ConnectionConfig>, AppError> {
    state.store.lock().await.list()
}

#[tauri::command]
pub async fn save_connection(
    state: State<'_, AppState>,
    config: ConnectionConfig,
    password: Option<String>,
) -> Result<(), AppError> {
    if let Some(p) = &password {
        CredentialStore::set(&config.id, p)?;
    }
    state.store.lock().await.upsert(config)
}

#[tauri::command]
pub async fn delete_connection(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    state.registry.remove(&id).await;
    CredentialStore::delete(&id)?;
    state.store.lock().await.delete(&id)
}

#[tauri::command]
pub async fn test_connection(
    config: ConnectionConfig,
    password: Option<String>,
) -> Result<(), AppError> {
    let pw = stored_or_given(&config.id, password)?;
    let conn = open_connection(&config, pw.as_deref()).await?;
    match conn {
        LiveConnection::Sql(d) => d.ping().await,
        LiveConnection::Kv(d) => d.ping().await,
    }
}

#[tauri::command]
pub async fn connect(state: State<'_, AppState>, id: String) -> Result<String, AppError> {
    let cfg = state.store.lock().await.list()?
        .into_iter().find(|c| c.id == id)
        .ok_or_else(|| AppError::internal(format!("unknown connection {id}")))?;
    let pw = CredentialStore::get(&id)?;
    let conn = open_connection(&cfg, pw.as_deref()).await?;
    let paradigm = match &conn {
        LiveConnection::Sql(_) => "sql",
        LiveConnection::Kv(_) => "kv",
    };
    state.registry.insert(&id, conn).await;
    Ok(paradigm.to_string())
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    state.registry.remove(&id).await;
    Ok(())
}

async fn sql_conn(state: &State<'_, AppState>, id: &str) -> Result<std::sync::Arc<LiveConnection>, AppError> {
    state.registry.get(id).await
        .ok_or_else(|| AppError::connection("not connected"))
}

#[tauri::command]
pub async fn list_tables(state: State<'_, AppState>, id: String) -> Result<Vec<TableInfo>, AppError> {
    match &*sql_conn(&state, &id).await? {
        LiveConnection::Sql(d) => d.list_tables().await,
        LiveConnection::Kv(_) => Err(AppError::query("not a SQL connection")),
    }
}

#[tauri::command]
pub async fn fetch_rows(
    state: State<'_, AppState>,
    id: String,
    table: TableInfo,
    opts: FetchOptions,
) -> Result<RowsPage, AppError> {
    match &*sql_conn(&state, &id).await? {
        LiveConnection::Sql(d) => d.fetch_rows(&table, &opts).await,
        LiveConnection::Kv(_) => Err(AppError::query("not a SQL connection")),
    }
}

#[tauri::command]
pub async fn scan_keys(
    state: State<'_, AppState>,
    id: String,
    pattern: String,
    cursor: u64,
) -> Result<KeyScanPage, AppError> {
    match &*sql_conn(&state, &id).await? {
        LiveConnection::Kv(d) => d.scan_keys(&pattern, cursor, 200).await,
        LiveConnection::Sql(_) => Err(AppError::query("not a Redis connection")),
    }
}

#[tauri::command]
pub async fn get_redis_value(
    state: State<'_, AppState>,
    id: String,
    key: String,
) -> Result<RedisEntry, AppError> {
    match &*sql_conn(&state, &id).await? {
        LiveConnection::Kv(d) => d.get_value(&key).await,
        LiveConnection::Sql(_) => Err(AppError::query("not a Redis connection")),
    }
}

#[tauri::command]
pub fn build_key_tree(keys: Vec<String>) -> Vec<KeyTreeNode> {
    keytree::build(&keys)
}
```

- [ ] **Step 5: Wire `lib.rs`**

```rust
pub mod commands;
pub mod config;
pub mod credentials;
pub mod drivers;
pub mod error;
pub mod registry;

use config::ConfigStore;
use registry::ConnectionRegistry;
use tauri::Manager;
use tokio::sync::Mutex;

pub struct AppState {
    pub store: Mutex<ConfigStore>,
    pub registry: ConnectionRegistry,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            app.manage(AppState {
                store: Mutex::new(ConfigStore::new(dir.join("connections.json"))),
                registry: ConnectionRegistry::default(),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_connections,
            commands::save_connection,
            commands::delete_connection,
            commands::test_connection,
            commands::connect,
            commands::disconnect,
            commands::list_tables,
            commands::fetch_rows,
            commands::scan_keys,
            commands::get_redis_value,
            commands::build_key_tree,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

(Remove the template's `greet` command and its frontend usage.)

- [ ] **Step 6: Verify full build + all tests**

Run: `cd src-tauri && cargo test` — Expected: all unit tests pass, integration tests skip without env vars. Then `npm run tauri dev` — Expected: window still opens.

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "feat: connection registry, full M1 tauri command surface"
```

---

### Task 10: Frontend API layer + connections store

**Files:**
- Create: `src/lib/api.ts`, `src/stores/connections.ts`, `tests/connections.store.test.ts`

**Interfaces:**
- Consumes: command names/payloads from Task 9 (Tauri auto-converts JS camelCase args to Rust snake_case).
- Produces: `api` object and TS types (`ConnectionConfig`, `Engine`, `EnvColor`, `TableInfo`, `RowsPage`, `FetchOptions`, `Filter`, `Sort`, `KeyScanPage`, `KeyTreeNode`, `RedisEntry`, `AppError`); Pinia store `useConnectionsStore` with `{ connections, load(), save(cfg, password?), remove(id), test(cfg, password?), connect(id) -> 'sql'|'kv', disconnect(id) }`.

- [ ] **Step 1: Implement `src/lib/api.ts`**

```ts
import { invoke } from "@tauri-apps/api/core";

export type Engine = "mysql" | "postgres" | "sqlite" | "redis";
export type EnvColor = "gray" | "blue" | "green" | "amber" | "red";

export interface ConnectionConfig {
  id: string;
  name: string;
  engine: Engine;
  color: EnvColor;
  host?: string | null;
  port?: number | null;
  username?: string | null;
  database?: string | null;
  isProduction: boolean;
}

export interface AppError { kind: string; message: string; detail?: string | null }

export interface TableInfo { schema?: string | null; name: string; kind: "table" | "view" }
export interface ColumnMeta { name: string; dataType: string }
export interface RowsPage { columns: ColumnMeta[]; rows: unknown[][]; totalEstimate?: number | null }
export type FilterOp = "contains" | "equals" | "isNull" | "notNull";
export interface Filter { column: string; op: FilterOp; value: string }
export interface Sort { column: string; desc: boolean }
export interface FetchOptions { limit: number; offset: number; sort?: Sort | null; filters: Filter[] }
export interface KeyScanPage { keys: string[]; cursor: number }
export interface KeyTreeNode { name: string; fullKey?: string | null; children: KeyTreeNode[]; count: number }
export type RedisValue =
  | { type: "string"; value: string }
  | { type: "hash"; fields: [string, string][] }
  | { type: "list"; items: string[] }
  | { type: "set"; members: string[] }
  | { type: "zSet"; members: [string, number][] };
export interface RedisEntry { key: string; ttlSecs: number; value: RedisValue }

export const api = {
  listConnections: () => invoke<ConnectionConfig[]>("list_connections"),
  saveConnection: (config: ConnectionConfig, password?: string) =>
    invoke<void>("save_connection", { config, password: password ?? null }),
  deleteConnection: (id: string) => invoke<void>("delete_connection", { id }),
  testConnection: (config: ConnectionConfig, password?: string) =>
    invoke<void>("test_connection", { config, password: password ?? null }),
  connect: (id: string) => invoke<"sql" | "kv">("connect", { id }),
  disconnect: (id: string) => invoke<void>("disconnect", { id }),
  listTables: (id: string) => invoke<TableInfo[]>("list_tables", { id }),
  fetchRows: (id: string, table: TableInfo, opts: FetchOptions) =>
    invoke<RowsPage>("fetch_rows", { id, table, opts }),
  scanKeys: (id: string, pattern: string, cursor: number) =>
    invoke<KeyScanPage>("scan_keys", { id, pattern, cursor }),
  getRedisValue: (id: string, key: string) => invoke<RedisEntry>("get_redis_value", { id, key }),
  buildKeyTree: (keys: string[]) => invoke<KeyTreeNode[]>("build_key_tree", { keys }),
};
```

- [ ] **Step 2: Write failing store test `tests/connections.store.test.ts`**

```ts
import { describe, it, expect, vi, beforeEach } from "vitest";
import { setActivePinia, createPinia } from "pinia";

vi.mock("../src/lib/api", () => ({
  api: {
    listConnections: vi.fn().mockResolvedValue([
      { id: "a", name: "local mysql", engine: "mysql", color: "blue", isProduction: false },
    ]),
    saveConnection: vi.fn().mockResolvedValue(undefined),
    deleteConnection: vi.fn().mockResolvedValue(undefined),
    connect: vi.fn().mockResolvedValue("sql"),
    disconnect: vi.fn().mockResolvedValue(undefined),
    testConnection: vi.fn().mockResolvedValue(undefined),
  },
}));

import { api } from "../src/lib/api";
import { useConnectionsStore } from "../src/stores/connections";

describe("connections store", () => {
  beforeEach(() => setActivePinia(createPinia()));

  it("loads connections", async () => {
    const store = useConnectionsStore();
    await store.load();
    expect(store.connections).toHaveLength(1);
    expect(store.connections[0].name).toBe("local mysql");
  });

  it("save reloads the list", async () => {
    const store = useConnectionsStore();
    await store.save(
      { id: "b", name: "x", engine: "sqlite", color: "gray", isProduction: false },
      undefined,
    );
    expect(api.saveConnection).toHaveBeenCalled();
    expect(api.listConnections).toHaveBeenCalled();
  });

  it("connect returns paradigm", async () => {
    const store = useConnectionsStore();
    expect(await store.connect("a")).toBe("sql");
  });
});
```

- [ ] **Step 3: Run to verify failure**

Run: `npm run test` — Expected: FAIL (store module missing).

- [ ] **Step 4: Implement `src/stores/connections.ts`**

```ts
import { defineStore } from "pinia";
import { ref } from "vue";
import { api, type ConnectionConfig } from "../lib/api";

export const useConnectionsStore = defineStore("connections", () => {
  const connections = ref<ConnectionConfig[]>([]);

  async function load() {
    connections.value = await api.listConnections();
  }
  async function save(cfg: ConnectionConfig, password?: string) {
    await api.saveConnection(cfg, password);
    await load();
  }
  async function remove(id: string) {
    await api.deleteConnection(id);
    await load();
  }
  async function test(cfg: ConnectionConfig, password?: string) {
    await api.testConnection(cfg, password);
  }
  async function connect(id: string) {
    return api.connect(id);
  }
  async function disconnect(id: string) {
    await api.disconnect(id);
  }

  return { connections, load, save, remove, test, connect, disconnect };
});
```

- [ ] **Step 5: Run tests**

Run: `npm run test` — Expected: all pass (smoke + 3 store tests).

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat: typed api layer + connections store"
```

---

### Task 11: ConnectionLauncher + ConnectionForm

**Files:**
- Create: `src/components/ConnectionLauncher.vue`, `src/components/ConnectionForm.vue`
- Modify: `src/App.vue`, `src/main.ts`

**Interfaces:**
- Consumes: `useConnectionsStore`, types from `api.ts`.
- Produces: `ConnectionLauncher` emits `connected(id: string, paradigm: "sql" | "kv")`. `ConnectionForm` props `{ initial?: ConnectionConfig | null }`, emits `saved` and `cancel`.

- [ ] **Step 1: Wire Pinia in `src/main.ts`**

```ts
import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import "./style.css";

createApp(App).use(createPinia()).mount("#app");
```

- [ ] **Step 2: Implement `ConnectionForm.vue`**

```vue
<script setup lang="ts">
import { reactive, ref } from "vue";
import type { ConnectionConfig, Engine, EnvColor } from "../lib/api";
import { useConnectionsStore } from "../stores/connections";

const props = defineProps<{ initial?: ConnectionConfig | null }>();
const emit = defineEmits<{ saved: []; cancel: [] }>();
const store = useConnectionsStore();

const defaultPorts: Record<Engine, number | null> = { mysql: 3306, postgres: 5432, sqlite: null, redis: 6379 };
const colors: EnvColor[] = ["gray", "blue", "green", "amber", "red"];

const form = reactive<ConnectionConfig>(
  props.initial ? { ...props.initial } : {
    id: crypto.randomUUID(), name: "", engine: "mysql", color: "gray",
    host: "127.0.0.1", port: 3306, username: "", database: "", isProduction: false,
  },
);
const password = ref("");
const testState = ref<"idle" | "testing" | "ok" | "fail">("idle");
const testMessage = ref("");

function onEngineChange() {
  form.port = defaultPorts[form.engine];
}
function onColorPick(c: EnvColor) {
  form.color = c;
  if (c === "red") form.isProduction = true;
}
async function test() {
  testState.value = "testing";
  try {
    await store.test(form, password.value || undefined);
    testState.value = "ok";
    testMessage.value = "Connection OK";
  } catch (e: any) {
    testState.value = "fail";
    testMessage.value = e?.message ?? String(e);
  }
}
async function save() {
  await store.save(form, password.value || undefined);
  emit("saved");
}
const isFile = () => form.engine === "sqlite";
</script>

<template>
  <form class="flex flex-col gap-3 w-96" @submit.prevent="save">
    <input v-model="form.name" placeholder="Connection name" required
      class="bg-zinc-900 border border-zinc-800 rounded px-2 py-1.5 focus:border-zinc-600 outline-none" />

    <select v-model="form.engine" @change="onEngineChange"
      class="bg-zinc-900 border border-zinc-800 rounded px-2 py-1.5">
      <option value="mysql">MySQL / MariaDB</option>
      <option value="postgres">PostgreSQL</option>
      <option value="sqlite">SQLite</option>
      <option value="redis">Redis</option>
    </select>

    <div class="flex gap-2 items-center">
      <span class="text-zinc-500 text-xs">Environment</span>
      <button v-for="c in colors" :key="c" type="button" @click="onColorPick(c)"
        class="w-4 h-4 rounded-full border"
        :class="[
          { gray: 'bg-zinc-500', blue: 'bg-blue-500', green: 'bg-green-500', amber: 'bg-amber-500', red: 'bg-red-500' }[c],
          form.color === c ? 'border-white' : 'border-transparent opacity-50',
        ]" />
      <label class="ml-auto flex items-center gap-1 text-xs text-zinc-400">
        <input type="checkbox" v-model="form.isProduction" /> production
      </label>
    </div>

    <template v-if="!isFile()">
      <div class="flex gap-2">
        <input v-model="form.host" placeholder="Host" class="flex-1 bg-zinc-900 border border-zinc-800 rounded px-2 py-1.5" />
        <input v-model.number="form.port" type="number" placeholder="Port" class="w-24 bg-zinc-900 border border-zinc-800 rounded px-2 py-1.5" />
      </div>
      <input v-if="form.engine !== 'redis'" v-model="form.username" placeholder="Username"
        class="bg-zinc-900 border border-zinc-800 rounded px-2 py-1.5" />
      <input v-model="password" type="password" placeholder="Password (stored in Keychain)"
        class="bg-zinc-900 border border-zinc-800 rounded px-2 py-1.5" />
      <input v-model="form.database" :placeholder="form.engine === 'redis' ? 'DB index (0)' : 'Database'"
        class="bg-zinc-900 border border-zinc-800 rounded px-2 py-1.5" />
    </template>
    <input v-else v-model="form.database" placeholder="/path/to/database.sqlite"
      class="bg-zinc-900 border border-zinc-800 rounded px-2 py-1.5" />

    <div class="flex gap-2 items-center pt-1">
      <button type="button" @click="test" class="px-3 py-1.5 rounded border border-zinc-700 hover:bg-zinc-800">
        {{ testState === "testing" ? "Testing…" : "Test Connection" }}
      </button>
      <span v-if="testState === 'ok'" class="text-green-400 text-xs">{{ testMessage }}</span>
      <span v-if="testState === 'fail'" class="text-red-400 text-xs truncate">{{ testMessage }}</span>
      <div class="ml-auto flex gap-2">
        <button type="button" @click="emit('cancel')" class="px-3 py-1.5 rounded text-zinc-400 hover:bg-zinc-800">Cancel</button>
        <button type="submit" class="px-3 py-1.5 rounded bg-blue-600 hover:bg-blue-500 text-white">Save</button>
      </div>
    </div>
  </form>
</template>
```

- [ ] **Step 3: Implement `ConnectionLauncher.vue`**

```vue
<script setup lang="ts">
import { onMounted, ref } from "vue";
import type { ConnectionConfig } from "../lib/api";
import { useConnectionsStore } from "../stores/connections";
import ConnectionForm from "./ConnectionForm.vue";

const emit = defineEmits<{ connected: [id: string, paradigm: "sql" | "kv"] }>();
const store = useConnectionsStore();
const editing = ref<ConnectionConfig | null>(null);
const showForm = ref(false);
const busyId = ref<string | null>(null);
const errorById = ref<Record<string, string>>({});

onMounted(store.load);

const colorClass: Record<string, string> = {
  gray: "bg-zinc-500", blue: "bg-blue-500", green: "bg-green-500",
  amber: "bg-amber-500", red: "bg-red-500",
};

async function open(c: ConnectionConfig) {
  busyId.value = c.id;
  errorById.value = {};
  try {
    const paradigm = await store.connect(c.id);
    emit("connected", c.id, paradigm);
  } catch (e: any) {
    errorById.value[c.id] = e?.message ?? String(e);
  } finally {
    busyId.value = null;
  }
}
function edit(c: ConnectionConfig) { editing.value = c; showForm.value = true; }
function add() { editing.value = null; showForm.value = true; }
</script>

<template>
  <div class="h-full flex items-center justify-center">
    <ConnectionForm v-if="showForm" :initial="editing"
      @saved="showForm = false" @cancel="showForm = false" />
    <div v-else class="w-[28rem]">
      <div class="flex items-center justify-between mb-4">
        <h1 class="text-sm font-medium text-zinc-400">Connections</h1>
        <button @click="add" class="px-2 py-1 rounded border border-zinc-700 hover:bg-zinc-800 text-xs">+ New</button>
      </div>
      <ul class="flex flex-col gap-1">
        <li v-for="c in store.connections" :key="c.id"
          class="group flex items-center gap-3 px-3 py-2 rounded border border-zinc-800 hover:border-zinc-600 cursor-pointer"
          @dblclick="open(c)">
          <span class="w-2.5 h-2.5 rounded-full" :class="colorClass[c.color]" />
          <div class="flex-1 min-w-0">
            <div class="truncate">{{ c.name }}</div>
            <div class="text-xs text-zinc-500">{{ c.engine }}<span v-if="c.host"> · {{ c.host }}</span></div>
            <div v-if="errorById[c.id]" class="text-xs text-red-400 truncate">{{ errorById[c.id] }}</div>
          </div>
          <button @click.stop="open(c)" class="text-xs px-2 py-1 rounded bg-zinc-800 opacity-0 group-hover:opacity-100">
            {{ busyId === c.id ? "…" : "Connect" }}
          </button>
          <button @click.stop="edit(c)" class="text-xs text-zinc-500 opacity-0 group-hover:opacity-100">Edit</button>
          <button @click.stop="store.remove(c.id)" class="text-xs text-red-500/70 opacity-0 group-hover:opacity-100">Delete</button>
        </li>
        <li v-if="!store.connections.length" class="text-zinc-600 text-center py-8 text-xs">
          No connections yet — create one.
        </li>
      </ul>
    </div>
  </div>
</template>
```

- [ ] **Step 4: Minimal `App.vue` (workspace placeholder until Task 12)**

```vue
<script setup lang="ts">
import { ref } from "vue";
import ConnectionLauncher from "./components/ConnectionLauncher.vue";

const active = ref<{ id: string; paradigm: "sql" | "kv" } | null>(null);
</script>

<template>
  <main class="h-full">
    <ConnectionLauncher v-if="!active" @connected="(id, paradigm) => (active = { id, paradigm })" />
    <div v-else class="p-4 text-zinc-400">connected: {{ active.id }} ({{ active.paradigm }})</div>
  </main>
</template>
```

- [ ] **Step 5: Manual verify**

Run: `npm run tauri dev`. Create a SQLite connection pointing at any existing `.sqlite`/`.db` file (e.g. the qa-intake-tool database), Test Connection → "Connection OK", Save, double-click → placeholder shows `connected: … (sql)`. Also verify a deliberately-wrong MySQL connection shows a red error message on Test.

- [ ] **Step 6: Run tests & commit**

Run: `npm run test` — Expected: pass.

```bash
git add -A && git commit -m "feat: connection launcher + form with env colors, keychain-backed password"
```

---

### Task 12: Workspace shell + SchemaSidebar + workspace store

**Files:**
- Create: `src/stores/workspace.ts`, `src/components/SchemaSidebar.vue`
- Modify: `src/App.vue`

**Interfaces:**
- Consumes: `api.listTables`, `api.disconnect`, connections store.
- Produces: `useWorkspaceStore` with `{ activeId, paradigm, tables, selectedTable, open(id, paradigm), close(), loadTables(), selectTable(t) }`. `SchemaSidebar` renders the table list, emits nothing (writes to store). `App.vue` renders: launcher when closed; otherwise toolbar (connection name + colored dot + Disconnect) over `sidebar | main` split; main area shows `DataGrid` for SQL (Task 13) or `KeyBrowser` for Redis (Task 14) — until those tasks land, a `<div>` placeholder.

- [ ] **Step 1: Implement `src/stores/workspace.ts`**

```ts
import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { api, type TableInfo } from "../lib/api";

export const useWorkspaceStore = defineStore("workspace", () => {
  const activeId = ref<string | null>(null);
  const paradigm = ref<"sql" | "kv" | null>(null);
  const tables = ref<TableInfo[]>([]);
  const selectedTable = ref<TableInfo | null>(null);
  const isOpen = computed(() => activeId.value !== null);

  async function open(id: string, p: "sql" | "kv") {
    activeId.value = id;
    paradigm.value = p;
    selectedTable.value = null;
    tables.value = [];
    if (p === "sql") await loadTables();
  }
  async function loadTables() {
    if (activeId.value) tables.value = await api.listTables(activeId.value);
  }
  function selectTable(t: TableInfo) {
    selectedTable.value = t;
  }
  async function close() {
    if (activeId.value) await api.disconnect(activeId.value);
    activeId.value = null;
    paradigm.value = null;
    tables.value = [];
    selectedTable.value = null;
  }

  return { activeId, paradigm, tables, selectedTable, isOpen, open, loadTables, selectTable, close };
});
```

- [ ] **Step 2: Implement `SchemaSidebar.vue`**

```vue
<script setup lang="ts">
import { computed, ref } from "vue";
import { useWorkspaceStore } from "../stores/workspace";

const ws = useWorkspaceStore();
const filter = ref("");
const filtered = computed(() =>
  ws.tables.filter((t) => t.name.toLowerCase().includes(filter.value.toLowerCase())),
);
const label = (t: { schema?: string | null; name: string }) =>
  t.schema && t.schema !== "public" ? `${t.schema}.${t.name}` : t.name;
</script>

<template>
  <aside class="w-56 shrink-0 border-r border-zinc-800 flex flex-col">
    <input v-model="filter" placeholder="Filter tables…"
      class="m-2 bg-zinc-900 border border-zinc-800 rounded px-2 py-1 text-xs outline-none focus:border-zinc-600" />
    <ul class="flex-1 overflow-y-auto px-1 pb-2">
      <li v-for="t in filtered" :key="label(t)"
        class="px-2 py-1 rounded cursor-pointer truncate text-[12.5px]"
        :class="ws.selectedTable === t ? 'bg-zinc-800 text-white' : 'text-zinc-400 hover:bg-zinc-900'"
        @click="ws.selectTable(t)">
        <span class="text-zinc-600 mr-1">{{ t.kind === "view" ? "◇" : "▤" }}</span>{{ label(t) }}
      </li>
      <li v-if="!filtered.length" class="text-zinc-600 text-xs px-2 py-2">No tables</li>
    </ul>
  </aside>
</template>
```

- [ ] **Step 3: Rewrite `App.vue` as the shell**

```vue
<script setup lang="ts">
import { computed } from "vue";
import ConnectionLauncher from "./components/ConnectionLauncher.vue";
import SchemaSidebar from "./components/SchemaSidebar.vue";
import { useConnectionsStore } from "./stores/connections";
import { useWorkspaceStore } from "./stores/workspace";

const ws = useWorkspaceStore();
const conns = useConnectionsStore();
const active = computed(() => conns.connections.find((c) => c.id === ws.activeId));
const colorClass: Record<string, string> = {
  gray: "bg-zinc-500", blue: "bg-blue-500", green: "bg-green-500",
  amber: "bg-amber-500", red: "bg-red-500",
};
</script>

<template>
  <main class="h-full flex flex-col">
    <ConnectionLauncher v-if="!ws.isOpen" @connected="(id, p) => ws.open(id, p)" />
    <template v-else>
      <header class="h-9 shrink-0 flex items-center gap-2 px-3 border-b border-zinc-800">
        <span class="w-2 h-2 rounded-full" :class="colorClass[active?.color ?? 'gray']" />
        <span class="text-xs font-medium">{{ active?.name }}</span>
        <span v-if="active?.isProduction" class="text-[10px] uppercase tracking-wide text-red-400 border border-red-900 rounded px-1">prod</span>
        <button @click="ws.close()" class="ml-auto text-xs text-zinc-500 hover:text-zinc-300">Disconnect</button>
      </header>
      <div class="flex-1 flex min-h-0">
        <SchemaSidebar v-if="ws.paradigm === 'sql'" />
        <section class="flex-1 min-w-0 flex items-center justify-center text-zinc-600 text-xs">
          <!-- DataGrid (Task 13) / KeyBrowser+RedisValuePane (Task 14) mount here -->
          {{ ws.paradigm === "sql" ? (ws.selectedTable ? ws.selectedTable.name : "Select a table") : "Redis browser coming in Task 14" }}
        </section>
      </div>
    </template>
  </main>
</template>
```

- [ ] **Step 4: Manual verify**

Run: `npm run tauri dev`. Connect to the SQLite file → sidebar lists its tables, filter box narrows them, clicking one highlights it and shows its name in the main area. Disconnect returns to launcher.

- [ ] **Step 5: Run tests & commit**

Run: `npm run test` — Expected: pass.

```bash
git add -A && git commit -m "feat: workspace shell, schema sidebar, workspace store"
```

---

### Task 13: DataGrid — virtualized, paginated, sortable, filterable

**Files:**
- Create: `src/lib/virtual.ts`, `tests/virtual.test.ts`, `src/components/DataGrid.vue`
- Modify: `src/App.vue` (mount DataGrid for SQL paradigm)

**Interfaces:**
- Consumes: `api.fetchRows`, workspace store (`activeId`, `selectedTable`).
- Produces: `visibleRange(scrollTop, viewportHeight, rowHeight, total, overscan?) -> { start, end }`; `DataGrid.vue` (no props — reads workspace store). Page size 500, infinite append on scroll near bottom.

- [ ] **Step 1: Write failing test `tests/virtual.test.ts`**

```ts
import { describe, it, expect } from "vitest";
import { visibleRange } from "../src/lib/virtual";

describe("visibleRange", () => {
  it("computes window with overscan", () => {
    // 28px rows, viewport 560px => 20 visible; scrolled to row 100
    const r = visibleRange(2800, 560, 28, 10000, 10);
    expect(r.start).toBe(90);  // 100 - overscan
    expect(r.end).toBe(130);   // 120 + overscan
  });
  it("clamps at edges", () => {
    expect(visibleRange(0, 560, 28, 10000, 10)).toEqual({ start: 0, end: 30 });
    expect(visibleRange(999999, 560, 28, 50, 10)).toEqual({ start: 40, end: 50 });
  });
  it("handles fewer rows than viewport", () => {
    expect(visibleRange(0, 560, 28, 5, 10)).toEqual({ start: 0, end: 5 });
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `npm run test` — Expected: FAIL (module missing).

- [ ] **Step 3: Implement `src/lib/virtual.ts`**

```ts
export function visibleRange(
  scrollTop: number,
  viewportHeight: number,
  rowHeight: number,
  total: number,
  overscan = 10,
): { start: number; end: number } {
  const first = Math.floor(scrollTop / rowHeight);
  const visible = Math.ceil(viewportHeight / rowHeight);
  const start = Math.max(0, Math.min(first, Math.max(0, total - visible)) - overscan);
  const end = Math.min(total, first + visible + overscan);
  return { start, end };
}
```

Run: `npm run test` — Expected: pass.

- [ ] **Step 4: Implement `DataGrid.vue`**

```vue
<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { api, type Filter, type FilterOp, type RowsPage, type Sort } from "../lib/api";
import { visibleRange } from "../lib/virtual";
import { useWorkspaceStore } from "../stores/workspace";

const ws = useWorkspaceStore();
const PAGE = 500;
const ROW_H = 28;

const page = ref<RowsPage | null>(null);
const rows = ref<unknown[][]>([]);
const exhausted = ref(false);
const loading = ref(false);
const error = ref<string | null>(null);
const sort = ref<Sort | null>(null);
const filterDrafts = reactive<Record<string, string>>({});
const filters = ref<Filter[]>([]);
const scrollTop = ref(0);
const viewportH = ref(600);
const scroller = ref<HTMLElement | null>(null);

const range = computed(() => visibleRange(scrollTop.value, viewportH.value, ROW_H, rows.value.length));
const slice = computed(() => rows.value.slice(range.value.start, range.value.end));

async function load(reset: boolean) {
  if (!ws.activeId || !ws.selectedTable || loading.value) return;
  if (reset) { rows.value = []; exhausted.value = false; }
  if (exhausted.value) return;
  loading.value = true;
  error.value = null;
  try {
    const p = await api.fetchRows(ws.activeId, ws.selectedTable, {
      limit: PAGE, offset: rows.value.length, sort: sort.value, filters: filters.value,
    });
    page.value = p;
    rows.value = rows.value.concat(p.rows);
    if (p.rows.length < PAGE) exhausted.value = true;
  } catch (e: any) {
    error.value = e?.message ?? String(e);
  } finally {
    loading.value = false;
  }
}

watch(() => ws.selectedTable, () => {
  sort.value = null;
  filters.value = [];
  Object.keys(filterDrafts).forEach((k) => delete filterDrafts[k]);
  if (scroller.value) scroller.value.scrollTop = 0;
  load(true);
}, { immediate: true });

function toggleSort(col: string) {
  sort.value = sort.value?.column === col
    ? sort.value.desc ? null : { column: col, desc: true }
    : { column: col, desc: false };
  load(true);
}

function applyFilter(col: string) {
  const v = (filterDrafts[col] ?? "").trim();
  const op: FilterOp = "contains";
  filters.value = filters.value.filter((f) => f.column !== col);
  if (v) filters.value.push({ column: col, op, value: v });
  load(true);
}

function onScroll(e: Event) {
  const el = e.target as HTMLElement;
  scrollTop.value = el.scrollTop;
  viewportH.value = el.clientHeight;
  if (el.scrollTop + el.clientHeight > el.scrollHeight - ROW_H * 40) load(false);
}

function cellText(v: unknown): string {
  if (v === null || v === undefined) return "";
  return typeof v === "object" ? JSON.stringify(v) : String(v);
}
</script>

<template>
  <div class="flex-1 min-w-0 flex flex-col">
    <div v-if="error" class="m-2 px-3 py-2 text-xs text-red-400 border border-red-900 rounded">{{ error }}</div>

    <div ref="scroller" class="flex-1 overflow-auto font-mono text-[12px]" @scroll="onScroll">
      <table class="border-collapse min-w-full">
        <thead class="sticky top-0 bg-zinc-950 z-10">
          <tr>
            <th v-for="c in page?.columns ?? []" :key="c.name"
              class="text-left px-2 py-1 border-b border-zinc-800 font-medium text-zinc-400 cursor-pointer select-none whitespace-nowrap"
              @click="toggleSort(c.name)">
              {{ c.name }}
              <span v-if="sort?.column === c.name">{{ sort.desc ? "↓" : "↑" }}</span>
            </th>
          </tr>
          <tr>
            <th v-for="c in page?.columns ?? []" :key="c.name" class="px-1 pb-1 border-b border-zinc-800">
              <input v-model="filterDrafts[c.name]" @keydown.enter="applyFilter(c.name)"
                placeholder="filter…"
                class="w-full min-w-24 bg-zinc-900 border border-zinc-800 rounded px-1 py-0.5 text-[11px] font-sans outline-none focus:border-zinc-600" />
            </th>
          </tr>
        </thead>
        <tbody>
          <tr :style="{ height: `${range.start * ROW_H}px` }" v-if="range.start > 0" />
          <tr v-for="(r, i) in slice" :key="range.start + i" class="hover:bg-zinc-900" :style="{ height: `${ROW_H}px` }">
            <td v-for="(v, j) in r" :key="j" class="px-2 border-b border-zinc-900 whitespace-nowrap max-w-96 overflow-hidden text-ellipsis">
              <span v-if="v === null" class="text-zinc-600 italic">NULL</span>
              <span v-else>{{ cellText(v) }}</span>
            </td>
          </tr>
          <tr :style="{ height: `${(rows.length - range.end) * ROW_H}px` }" v-if="range.end < rows.length" />
        </tbody>
      </table>
      <div v-if="loading" class="p-2 text-xs text-zinc-500">Loading…</div>
    </div>

    <footer class="h-7 shrink-0 flex items-center gap-3 px-3 border-t border-zinc-800 text-[11px] text-zinc-500">
      <span>{{ rows.length }} loaded<template v-if="page?.totalEstimate != null"> / ~{{ page.totalEstimate }}</template></span>
      <span v-if="filters.length">{{ filters.length }} filter(s)</span>
    </footer>
  </div>
</template>
```

- [ ] **Step 5: Mount in `App.vue`**

Replace the placeholder `<section>` with:

```vue
<DataGrid v-if="ws.paradigm === 'sql' && ws.selectedTable" />
<section v-else-if="ws.paradigm === 'sql'" class="flex-1 flex items-center justify-center text-zinc-600 text-xs">Select a table</section>
<section v-else class="flex-1 flex items-center justify-center text-zinc-600 text-xs">Redis browser coming in Task 14</section>
```

and add `import DataGrid from "./components/DataGrid.vue";`.

- [ ] **Step 6: Manual verify (acceptance-relevant)**

Seed a 100k-row SQLite table:

```bash
sqlite3 /tmp/big.db "CREATE TABLE big(id INTEGER PRIMARY KEY, val TEXT); WITH RECURSIVE g(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM g WHERE x < 100000) INSERT INTO big SELECT x, 'row ' || x FROM g;"
```

`npm run tauri dev` → connect → open `big` → scrolling is smooth, more pages append as you approach the bottom, status bar shows `N loaded / ~100000`. Click `id` header → sorts asc, again → desc, again → unsorted. Type `row 99` in `val` filter + Enter → narrowed results. NULL renders as italic badge.

- [ ] **Step 7: Run tests & commit**

Run: `npm run test` — Expected: all pass.

```bash
git add -A && git commit -m "feat: virtualized data grid with pagination, sort, per-column filters"
```

---

### Task 14: KeyBrowser + RedisValuePane

**Files:**
- Create: `src/components/KeyBrowser.vue`, `src/components/RedisValuePane.vue`
- Modify: `src/App.vue` (mount for kv paradigm)

**Interfaces:**
- Consumes: `api.scanKeys`, `api.buildKeyTree`, `api.getRedisValue`, workspace store.
- Produces: `KeyBrowser` emits `select(key: string)`; `RedisValuePane` props `{ entry: RedisEntry | null }`.

- [ ] **Step 1: Implement `KeyBrowser.vue`**

```vue
<script setup lang="ts">
import { onMounted, ref } from "vue";
import { api, type KeyTreeNode } from "../lib/api";
import { useWorkspaceStore } from "../stores/workspace";

const emit = defineEmits<{ select: [key: string] }>();
const ws = useWorkspaceStore();
const pattern = ref("*");
const tree = ref<KeyTreeNode[]>([]);
const keyCount = ref(0);
const loading = ref(false);
const done = ref(false);
const expanded = ref<Set<string>>(new Set());
let cursor = 0;
let allKeys: string[] = [];

async function scan(reset: boolean) {
  if (!ws.activeId || loading.value) return;
  if (reset) { cursor = 0; allKeys = []; done.value = false; }
  loading.value = true;
  try {
    // pull a few SCAN pages per click, stop at cursor 0
    for (let i = 0; i < 5; i++) {
      const page = await api.scanKeys(ws.activeId, pattern.value, cursor);
      allKeys = allKeys.concat(page.keys);
      cursor = page.cursor;
      if (cursor === 0) { done.value = true; break; }
    }
    keyCount.value = allKeys.length;
    tree.value = await api.buildKeyTree(allKeys);
  } finally {
    loading.value = false;
  }
}
function toggle(path: string) {
  expanded.value.has(path) ? expanded.value.delete(path) : expanded.value.add(path);
  expanded.value = new Set(expanded.value);
}
onMounted(() => scan(true));
</script>

<template>
  <aside class="w-72 shrink-0 border-r border-zinc-800 flex flex-col">
    <div class="p-2 flex gap-1">
      <input v-model="pattern" @keydown.enter="scan(true)" placeholder="Pattern (e.g. cache:*)"
        class="flex-1 bg-zinc-900 border border-zinc-800 rounded px-2 py-1 text-xs outline-none focus:border-zinc-600" />
      <button @click="scan(true)" class="text-xs px-2 rounded border border-zinc-700 hover:bg-zinc-800">Scan</button>
    </div>
    <div class="flex-1 overflow-y-auto px-1 pb-2 text-[12.5px]">
      <template v-for="node in tree" :key="node.name">
        <KeyTreeItem :node="node" :depth="0" :expanded="expanded" @toggle="toggle" @select="(k) => emit('select', k)" />
      </template>
    </div>
    <footer class="px-2 py-1 border-t border-zinc-800 text-[11px] text-zinc-500 flex gap-2">
      <span>{{ keyCount }} keys{{ done ? "" : " (partial)" }}</span>
      <button v-if="!done" @click="scan(false)" class="text-blue-400">load more</button>
    </footer>
  </aside>
</template>

<script lang="ts">
// Recursive tree item as a local secondary component
import { defineComponent, h } from "vue";
import type { PropType } from "vue";
import type { KeyTreeNode as KTN } from "../lib/api";

const KeyTreeItem = defineComponent({
  name: "KeyTreeItem",
  props: {
    node: { type: Object as PropType<KTN>, required: true },
    depth: { type: Number, required: true },
    expanded: { type: Object as PropType<Set<string>>, required: true },
  },
  emits: ["toggle", "select"],
  setup(props, { emit }) {
    const path = () => `${props.depth}:${props.node.name}:${props.node.fullKey ?? ""}`;
    return () => {
      const kids = props.node.children;
      const isOpen = props.expanded.has(path());
      const row = h("div", {
        class: "flex items-center gap-1 px-2 py-0.5 rounded cursor-pointer text-zinc-400 hover:bg-zinc-900",
        style: { paddingLeft: `${8 + props.depth * 12}px` },
        onClick: () => {
          if (kids.length) emit("toggle", path());
          if (props.node.fullKey) emit("select", props.node.fullKey);
        },
      }, [
        kids.length ? h("span", { class: "text-zinc-600 w-3" }, isOpen ? "▾" : "▸") : h("span", { class: "w-3" }),
        h("span", { class: props.node.fullKey ? "" : "text-zinc-500" }, props.node.name),
        kids.length ? h("span", { class: "text-zinc-600 text-[10px]" }, String(props.node.count)) : null,
      ]);
      const children = isOpen
        ? kids.map((c) => h(KeyTreeItem, {
            node: c, depth: props.depth + 1, expanded: props.expanded,
            onToggle: (p: string) => emit("toggle", p),
            onSelect: (k: string) => emit("select", k),
          }))
        : [];
      return h("div", null, [row, ...children]);
    };
  },
});
export default { components: { KeyTreeItem } };
</script>
```

- [ ] **Step 2: Implement `RedisValuePane.vue`**

```vue
<script setup lang="ts">
import type { RedisEntry } from "../lib/api";

defineProps<{ entry: RedisEntry | null }>();

function ttlText(ttl: number): string {
  if (ttl === -1) return "no TTL";
  if (ttl === -2) return "missing";
  return `TTL ${ttl}s`;
}
function looksJson(s: string): boolean {
  const t = s.trim();
  return (t.startsWith("{") && t.endsWith("}")) || (t.startsWith("[") && t.endsWith("]"));
}
function pretty(s: string): string {
  try { return JSON.stringify(JSON.parse(s), null, 2); } catch { return s; }
}
</script>

<template>
  <section class="flex-1 min-w-0 flex flex-col">
    <div v-if="!entry" class="flex-1 flex items-center justify-center text-zinc-600 text-xs">Select a key</div>
    <template v-else>
      <header class="h-8 shrink-0 flex items-center gap-2 px-3 border-b border-zinc-800 text-xs">
        <span class="font-mono truncate">{{ entry.key }}</span>
        <span class="text-zinc-500 uppercase text-[10px] border border-zinc-800 rounded px-1">{{ entry.value.type }}</span>
        <span class="ml-auto text-zinc-500">{{ ttlText(entry.ttlSecs) }}</span>
      </header>
      <div class="flex-1 overflow-auto p-3 font-mono text-[12px]">
        <pre v-if="entry.value.type === 'string'" class="whitespace-pre-wrap">{{
          looksJson(entry.value.value) ? pretty(entry.value.value) : entry.value.value
        }}</pre>
        <table v-else-if="entry.value.type === 'hash'" class="border-collapse">
          <tr v-for="[f, v] in entry.value.fields" :key="f">
            <td class="pr-4 py-0.5 text-zinc-400 border-b border-zinc-900 align-top">{{ f }}</td>
            <td class="py-0.5 border-b border-zinc-900 whitespace-pre-wrap">{{ v }}</td>
          </tr>
        </table>
        <ol v-else-if="entry.value.type === 'list'" class="list-decimal list-inside">
          <li v-for="(item, i) in entry.value.items" :key="i" class="py-0.5 border-b border-zinc-900">{{ item }}</li>
        </ol>
        <ul v-else-if="entry.value.type === 'set'">
          <li v-for="m in entry.value.members" :key="m" class="py-0.5 border-b border-zinc-900">{{ m }}</li>
        </ul>
        <table v-else-if="entry.value.type === 'zSet'" class="border-collapse">
          <tr v-for="[m, score] in entry.value.members" :key="m">
            <td class="pr-4 py-0.5 text-zinc-400 border-b border-zinc-900">{{ score }}</td>
            <td class="py-0.5 border-b border-zinc-900">{{ m }}</td>
          </tr>
        </table>
      </div>
    </template>
  </section>
</template>
```

- [ ] **Step 3: Mount in `App.vue`**

Replace the kv placeholder with:

```vue
<template v-else>
  <KeyBrowser @select="loadKey" />
  <RedisValuePane :entry="redisEntry" />
</template>
```

and in the script block:

```ts
import KeyBrowser from "./components/KeyBrowser.vue";
import RedisValuePane from "./components/RedisValuePane.vue";
import { api, type RedisEntry } from "./lib/api";
import { ref } from "vue";

const redisEntry = ref<RedisEntry | null>(null);
async function loadKey(key: string) {
  if (ws.activeId) redisEntry.value = await api.getRedisValue(ws.activeId, key);
}
```

- [ ] **Step 4: Manual verify**

With the docker Redis up, seed some realistic keys:

```bash
redis-cli -p 63791 set cache:users:1 '{"name":"Alice"}'
redis-cli -p 63791 hset session:abc user 1 role admin
redis-cli -p 63791 rpush queue:jobs a b c
redis-cli -p 63791 expire cache:users:1 3600
```

`npm run tauri dev` → create Redis connection (host 127.0.0.1, port 63791) → connect. Key tree shows `cache ▸ users ▸ 1`, `session ▸ abc`, `queue ▸ jobs`. Clicking `cache:users:1` shows pretty-printed JSON with `TTL 3600s`; the hash shows a field table; pattern `cache:*` + Scan narrows the tree.

- [ ] **Step 5: Run tests & commit**

Run: `npm run test` — Expected: pass.

```bash
git add -A && git commit -m "feat: redis key browser with namespace tree + typed value pane"
```

---

### Task 15: M1 acceptance pass + README

**Files:**
- Create: `README.md`

**Interfaces:**
- Consumes: the whole app.

- [ ] **Step 1: Full automated test sweep**

```bash
docker compose -f docker-compose.test.yml up -d && sleep 20
cd src-tauri
cargo test
DBCLIENT_TEST_MYSQL_URL="mysql://root:test@127.0.0.1:33061/dbclient_test" \
DBCLIENT_TEST_PG_URL="postgres://postgres:test@127.0.0.1:54321/dbclient_test" \
DBCLIENT_TEST_REDIS_URL="redis://127.0.0.1:63791/0" cargo test
cd .. && npm run test
```

Expected: everything passes.

- [ ] **Step 2: Manual acceptance checklist (from the spec, section 8/M1)**

Run `npm run tauri dev` and verify each:

- [ ] Save a connection to the qa-intake-tool SQLite file; browse its tables.
- [ ] Save a connection to a crm-saas MySQL database (or the docker MySQL); browse tables.
- [ ] Save a connection to the docker Postgres; browse tables.
- [ ] Save a Redis connection; browse keys by pattern; open string/hash/list values.
- [ ] Env colors show on cards and in the toolbar; a red connection shows the `prod` badge.
- [ ] 100k-row table (`/tmp/big.db` from Task 13) scrolls smoothly; filter and sort work.
- [ ] Wrong password / unreachable host shows a readable error, not a crash.
- [ ] Quit and relaunch: connections persist; passwords were NOT written to `connections.json` (inspect `~/Library/Application Support/com.arcris.dbclient/connections.json`).

- [ ] **Step 3: Write `README.md`**

```markdown
# dbclient

Native macOS database client (TablePlus-style) — Tauri 2 + Rust + Vue 3.
Engines: MySQL/MariaDB, PostgreSQL, SQLite, Redis.

## Develop

    npm install
    npm run tauri dev

## Test

    npm run test                                  # frontend
    cd src-tauri && cargo test                    # rust unit + sqlite integration

Integration tests against real engines:

    docker compose -f docker-compose.test.yml up -d
    cd src-tauri
    DBCLIENT_TEST_MYSQL_URL="mysql://root:test@127.0.0.1:33061/dbclient_test" \
    DBCLIENT_TEST_PG_URL="postgres://postgres:test@127.0.0.1:54321/dbclient_test" \
    DBCLIENT_TEST_REDIS_URL="redis://127.0.0.1:63791/0" cargo test

Design docs: `docs/superpowers/specs/`, plans: `docs/superpowers/plans/`.
```

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "docs: README + M1 acceptance pass"
```
