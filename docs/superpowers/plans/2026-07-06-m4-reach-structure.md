# M4 — Reach & Structure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Connect to non-exposed databases through SSH tunnels, and edit schema (create table, add/drop column, add/drop index) from a Structure tab with generated-DDL preview.

**Architecture:** A `tunnel.rs` module opens SSH connections via russh (password or key-file auth), listens on an ephemeral local port, and forwards each accepted TCP connection through a `direct-tcpip` channel to the database host; a `TunnelRegistry` ties tunnel lifetime to connection id (teardown: pool first, tunnel second). Schema editing mirrors the changeset pattern: a pure `ddl.rs` renders `DdlOp` values to engine-flavored SQL strings, the same string is previewed and executed. A `table_structure` driver method feeds the Structure tab.

**Tech Stack:** Existing stack + `russh`/`russh-keys` 0.45. Test sshd via a dockerfile_inline alpine service in docker-compose.test.yml.

## Global Constraints

- Spec §3/§5: SSH tunnel established BEFORE the pool; teardown order: pools, then tunnels. SSH passwords/key passphrases in Keychain (never plaintext) — stored under `<connection-id>.ssh`.
- Spec §8/M4 acceptance: "Connect through an SSH tunnel to a non-exposed database; create a table, add a column, add an index from the Structure tab with generated-DDL preview."
- Production connections: DDL is a write → ConfirmModal rail (spec §6).
- Known v1 simplification (document in README): host keys are accepted on first use without verification.
- Same global rules as M1–M3 (camelCase serde, AppError, env-gated integration tests, commit per task).

## File Structure

```
src-tauri/src/
├── tunnel.rs            # SshTunnel::open + TunnelRegistry (NEW)
├── ddl.rs               # DdlOp + render_ddl(op, flavor) (NEW, pure)
├── config.rs            # + SshConfig on ConnectionConfig (optional field)
├── drivers/mod.rs       # + TableStructure types; SqlDriver: execute_raw (trait), table_structure
├── drivers/{sqlite,mysql,postgres}.rs
└── commands.rs, lib.rs  # tunnel-aware connect/test, ddl + structure commands
src/
├── lib/api.ts           # SshConfig, structure/ddl types + calls
├── components/ConnectionForm.vue  # SSH section
├── components/StructureView.vue   # (NEW) columns/indexes + create/add/drop with DDL preview
└── App.vue              # Structure tab in the strip
src-tauri/tests/tunnel_it.rs       # end-to-end tunnel test (sshd docker)
docker-compose.test.yml            # + sshd service
```

---

### Task 1: SSH tunnel module + docker sshd + end-to-end test

**Files:**
- Create: `src-tauri/src/tunnel.rs`, `src-tauri/tests/tunnel_it.rs`
- Modify: `src-tauri/Cargo.toml` (`russh = "0.45"`, `russh-keys = "0.45"`), `src-tauri/src/lib.rs`, `docker-compose.test.yml`

**Interfaces:**

```rust
// tunnel.rs
pub struct SshAuth<'a> { pub key_path: Option<&'a str>, pub secret: Option<&'a str> }
// key_path Some => publickey auth (secret = optional passphrase); else password auth (secret = password)

pub struct SshTunnel { pub local_port: u16, /* task: JoinHandle aborted on drop */ }
impl SshTunnel {
    pub async fn open(ssh_host: &str, ssh_port: u16, ssh_user: &str, auth: SshAuth<'_>,
                      target_host: &str, target_port: u16) -> Result<SshTunnel, AppError>;
    // errors map to ErrorKind::TunnelFailed
}
impl Drop for SshTunnel { /* abort accept-loop task */ }

#[derive(Default)]
pub struct TunnelRegistry { /* Mutex<HashMap<String, SshTunnel>> */ }
impl TunnelRegistry {
    pub async fn insert(&self, id: &str, t: SshTunnel);
    pub async fn remove(&self, id: &str);   // drop closes the tunnel
}
```

Implementation shape: russh `client::connect` with a Handler whose `check_server_key` returns `Ok(true)` (documented TOFU simplification); authenticate via `authenticate_password` or `russh_keys::load_secret_key` + `authenticate_publickey`; bind `TcpListener` on `127.0.0.1:0`; accept loop task: for each socket, `channel_open_direct_tcpip(target_host, target_port, "127.0.0.1", 0)` then `tokio::io::copy_bidirectional(&mut socket, &mut channel.into_stream())` in a spawned task. Errors before listen → `AppError { kind: TunnelFailed, .. }` (add `AppError::tunnel(m)` constructor).

docker-compose addition:

```yaml
  sshd:
    build:
      context: .
      dockerfile_inline: |
        FROM alpine:3.20
        RUN apk add --no-cache openssh \
         && ssh-keygen -A \
         && adduser -D tunnel \
         && echo "tunnel:tunnelpass" | chpasswd \
         && sed -i 's/^#\?PasswordAuthentication.*/PasswordAuthentication yes/' /etc/ssh/sshd_config \
         && sed -i 's/^#\?AllowTcpForwarding.*/AllowTcpForwarding yes/' /etc/ssh/sshd_config
        CMD ["/usr/sbin/sshd", "-D", "-e"]
    ports: ["2223:22"]
```

- [ ] **Step 1: Write `tests/tunnel_it.rs`** (env-gated; tunnels to the compose-internal `mysql:3306` through the sshd container, then runs a real driver ping through the tunnel):

```rust
use dbclient_lib::drivers::mysql::MysqlDriver;
use dbclient_lib::drivers::SqlDriver;
use dbclient_lib::tunnel::{SshAuth, SshTunnel};

#[tokio::test]
async fn tunnel_to_mysql_via_sshd() {
    if std::env::var("DBCLIENT_TEST_SSH").is_err() {
        eprintln!("skipping: DBCLIENT_TEST_SSH not set");
        return;
    }
    let t = SshTunnel::open(
        "127.0.0.1", 2223, "tunnel",
        SshAuth { key_path: None, secret: Some("tunnelpass") },
        "mysql", 3306, // service name on the compose network, NOT exposed to the test directly
    ).await.unwrap();
    assert!(t.local_port > 0);

    let d = MysqlDriver::connect("127.0.0.1", t.local_port, "root", "test", "dbclient_test").await.unwrap();
    d.ping().await.unwrap();

    // bad auth -> TunnelFailed
    let err = SshTunnel::open(
        "127.0.0.1", 2223, "tunnel",
        SshAuth { key_path: None, secret: Some("wrong") },
        "mysql", 3306,
    ).await.unwrap_err();
    assert_eq!(err.kind, dbclient_lib::error::ErrorKind::TunnelFailed);
}
```

- [ ] **Step 2:** Add deps + compose service, `docker compose -f docker-compose.test.yml up -d --build sshd`. Run test — compile FAIL.

- [ ] **Step 3: Implement `tunnel.rs`:**

```rust
use crate::error::{AppError, ErrorKind};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

fn tunnel_err(m: impl std::fmt::Display) -> AppError {
    AppError { kind: ErrorKind::TunnelFailed, message: m.to_string(), detail: None }
}

struct AcceptAll;
impl russh::client::Handler for AcceptAll {
    type Error = russh::Error;
    async fn check_server_key(&mut self, _key: &russh_keys::key::PublicKey) -> Result<bool, Self::Error> {
        Ok(true) // v1: trust-on-first-use without persistence (documented)
    }
}

pub struct SshAuth<'a> { pub key_path: Option<&'a str>, pub secret: Option<&'a str> }

pub struct SshTunnel {
    pub local_port: u16,
    task: tokio::task::JoinHandle<()>,
}

impl SshTunnel {
    pub async fn open(ssh_host: &str, ssh_port: u16, ssh_user: &str, auth: SshAuth<'_>,
                      target_host: &str, target_port: u16) -> Result<SshTunnel, AppError> {
        let config = Arc::new(russh::client::Config::default());
        let mut session = russh::client::connect(config, (ssh_host, ssh_port), AcceptAll)
            .await.map_err(tunnel_err)?;
        let authed = match auth.key_path {
            Some(path) => {
                let key = russh_keys::load_secret_key(path, auth.secret).map_err(tunnel_err)?;
                session.authenticate_publickey(ssh_user, Arc::new(key)).await.map_err(tunnel_err)?
            }
            None => session.authenticate_password(ssh_user, auth.secret.unwrap_or(""))
                .await.map_err(tunnel_err)?,
        };
        if !authed { return Err(tunnel_err("SSH authentication failed")); }

        let listener = TcpListener::bind(("127.0.0.1", 0)).await.map_err(tunnel_err)?;
        let local_port = listener.local_addr().map_err(tunnel_err)?.port();
        let target_host = target_host.to_string();

        let task = tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else { break };
                let Ok(channel) = session
                    .channel_open_direct_tcpip(&target_host, target_port as u32, "127.0.0.1", 0)
                    .await else { break };
                tokio::spawn(async move {
                    let mut stream = channel.into_stream();
                    let _ = tokio::io::copy_bidirectional(&mut socket, &mut stream).await;
                });
            }
        });
        Ok(SshTunnel { local_port, task })
    }
}

impl Drop for SshTunnel {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Default)]
pub struct TunnelRegistry {
    inner: Mutex<HashMap<String, SshTunnel>>,
}

impl TunnelRegistry {
    pub async fn insert(&self, id: &str, t: SshTunnel) {
        self.inner.lock().await.insert(id.to_string(), t);
    }
    pub async fn remove(&self, id: &str) {
        self.inner.lock().await.remove(id);
    }
}
```

Add `pub mod tunnel;` to lib.rs and `pub fn tunnel(m) ` constructor if preferred over the local helper (either is fine as long as kind is TunnelFailed). NOTE: exact russh 0.45 API names may need small adaptations at compile time (e.g. `check_server_key` signature); adjust to the version that resolves, keeping behavior identical.

- [ ] **Step 4:** `DBCLIENT_TEST_SSH=1 cargo test --test tunnel_it` with the docker stack up — PASS. **Commit:** `feat: russh ssh tunnel with local port forward + registry`

---

### Task 2: SshConfig on connections + tunnel-aware connect/test

**Files:**
- Modify: `src-tauri/src/config.rs`, `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`, `src/lib/api.ts`

**Interfaces:**

```rust
// config.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshConfig {
    pub host: String,
    pub port: u16,          // default UI 22
    pub username: String,
    pub key_path: Option<String>, // Some => key auth (ssh secret = passphrase); None => password auth
}
// ConnectionConfig gains:  #[serde(default)] pub ssh: Option<SshConfig>,
```

- `AppState` gains `pub tunnels: TunnelRegistry`.
- `save_connection` gains `ssh_secret: Option<String>` (stored via `CredentialStore::set(&format!("{id}.ssh"), …)` when non-empty); `delete_connection` also deletes `<id>.ssh`.
- `open_connection` becomes `open_connection(cfg, password, ssh_secret) -> Result<(LiveConnection, Option<SshTunnel>), AppError>`: when `cfg.ssh` is Some (and engine ≠ sqlite), open the tunnel to (cfg.host, engine default port or cfg.port) FIRST, then connect the driver to `127.0.0.1:tunnel.local_port`.
- `connect` stores the tunnel in `state.tunnels`; `disconnect`/`delete_connection` remove pool from `registry` FIRST, then `tunnels.remove(id)` (spec teardown order). `test_connection` opens and drops both.
- api.ts: `SshConfig` interface, `ConnectionConfig.ssh?: SshConfig | null`, `saveConnection(config, password?, sshSecret?)`, `testConnection(config, password?, sshSecret?)` (command args `password`, `sshSecret`).

- [ ] **Step 1: Config back-compat test in `config.rs` tests:**

```rust
#[test]
fn old_configs_without_ssh_still_load() {
    let raw = r#"[{"id":"a","name":"x","engine":"sqlite","color":"gray","host":null,"port":null,"username":null,"database":"/tmp/x.db","isProduction":false}]"#;
    let parsed: Vec<ConnectionConfig> = serde_json::from_str(raw).unwrap();
    assert!(parsed[0].ssh.is_none());
}
```

- [ ] **Step 2:** FAIL → add the field/struct → PASS.

- [ ] **Step 3:** Rewire commands per Interfaces (test_connection signature gains `ssh_secret: Option<String>`; connect resolves stored `<id>.ssh` secret via `CredentialStore::get`). `cargo test` green, `npm run build` green after api.ts updates (ConnectionForm signature updated in Task 4).

- [ ] **Step 4: Commit:** `feat: per-connection ssh tunnel config wired into connect/test`

---

### Task 3: DDL ops + renderer + table_structure on drivers

**Files:**
- Create: `src-tauri/src/ddl.rs`
- Modify: `src-tauri/src/drivers/mod.rs` + the 3 SQL drivers, `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`
- Test: unit tests in `ddl.rs`; structure battery in `tests/common/mod.rs`

**Interfaces:**

```rust
// ddl.rs
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Flavor { Mysql, Postgres, Sqlite }

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnDef { pub name: String, pub data_type: String, pub nullable: bool, pub is_pk: bool }

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum DdlOp {
    CreateTable { name: String, columns: Vec<ColumnDef> },
    AddColumn { table: TableInfo, column: ColumnDef },
    DropColumn { table: TableInfo, column: String },
    AddIndex { table: TableInfo, name: String, columns: Vec<String>, unique: bool },
    DropIndex { table: TableInfo, name: String },
}

pub fn render_ddl(op: &DdlOp, flavor: Flavor) -> String
// quoting: backticks for mysql, double quotes otherwise
// CreateTable: CREATE TABLE t (c TYPE [NOT NULL], ..., PRIMARY KEY (a, b))   (pk clause only if any is_pk)
// AddColumn:   ALTER TABLE t ADD COLUMN c TYPE [NOT NULL]
// DropColumn:  ALTER TABLE t DROP COLUMN c
// AddIndex:    CREATE [UNIQUE ]INDEX name ON t (c1, c2)
// DropIndex:   mysql => DROP INDEX name ON t ; pg/sqlite => DROP INDEX name
```

```rust
// drivers/mod.rs
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructColumn { pub name: String, pub data_type: String, pub nullable: bool, pub default: Option<String>, pub is_pk: bool }
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructIndex { pub name: String, pub columns: Vec<String>, pub unique: bool }
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableStructure { pub columns: Vec<StructColumn>, pub indexes: Vec<StructIndex> }

// SqlDriver gains:
async fn execute_raw(&self, sql: &str) -> Result<(), AppError>;   // move existing inherent methods into the trait
async fn table_structure(&self, table: &TableInfo) -> Result<TableStructure, AppError>;
fn ddl_flavor(&self) -> crate::ddl::Flavor;
```

Structure sources — sqlite: `PRAGMA table_info` + `PRAGMA index_list` / `PRAGMA index_info`; mysql: `information_schema.columns` (CAST names AS CHAR; nullable = `IS_NULLABLE='YES'`; pk = `COLUMN_KEY='PRI'`) + `information_schema.statistics` grouped by index_name (`NON_UNIQUE=0` → unique, skip `PRIMARY`); postgres: `information_schema.columns` + the `pg_index`/`unnest(indkey) WITH ORDINALITY` join grouped in Rust, skipping the primary-key index (`indisprimary`).

Commands:

```rust
table_structure(id, table: TableInfo) -> TableStructure
preview_ddl(id, op: DdlOp) -> String                      // render only
apply_ddl(id, op: DdlOp) -> ()                            // render + execute_raw
```

- [ ] **Step 1: `ddl.rs` unit tests:**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::{TableInfo, TableKind};

    fn t() -> TableInfo { TableInfo { schema: None, name: "orders".into(), kind: TableKind::Table } }
    fn col(n: &str, ty: &str, nullable: bool, pk: bool) -> ColumnDef {
        ColumnDef { name: n.into(), data_type: ty.into(), nullable, is_pk: pk }
    }

    #[test]
    fn create_table_with_pk() {
        let op = DdlOp::CreateTable { name: "orders".into(), columns: vec![
            col("id", "INTEGER", false, true),
            col("note", "TEXT", true, false),
        ]};
        assert_eq!(render_ddl(&op, Flavor::Postgres),
            r#"CREATE TABLE "orders" ("id" INTEGER NOT NULL, "note" TEXT, PRIMARY KEY ("id"))"#);
        assert_eq!(render_ddl(&op, Flavor::Mysql),
            "CREATE TABLE `orders` (`id` INTEGER NOT NULL, `note` TEXT, PRIMARY KEY (`id`))");
    }

    #[test]
    fn add_drop_column() {
        let add = DdlOp::AddColumn { table: t(), column: col("qty", "INT", false, false) };
        assert_eq!(render_ddl(&add, Flavor::Mysql), "ALTER TABLE `orders` ADD COLUMN `qty` INT NOT NULL");
        let drop = DdlOp::DropColumn { table: t(), column: "qty".into() };
        assert_eq!(render_ddl(&drop, Flavor::Sqlite), r#"ALTER TABLE "orders" DROP COLUMN "qty""#);
    }

    #[test]
    fn add_drop_index_flavors() {
        let add = DdlOp::AddIndex { table: t(), name: "idx_qty".into(), columns: vec!["qty".into(), "id".into()], unique: true };
        assert_eq!(render_ddl(&add, Flavor::Postgres), r#"CREATE UNIQUE INDEX "idx_qty" ON "orders" ("qty", "id")"#);
        let drop = DdlOp::DropIndex { table: t(), name: "idx_qty".into() };
        assert_eq!(render_ddl(&drop, Flavor::Mysql), "DROP INDEX `idx_qty` ON `orders`");
        assert_eq!(render_ddl(&drop, Flavor::Postgres), r#"DROP INDEX "idx_qty""#);
    }
}
```

- [ ] **Step 2:** FAIL → implement `render_ddl` (schema-qualify via `drivers::qualified_name` with the flavor's quote fn) → PASS.

- [ ] **Step 3: Structure battery in `tests/common/mod.rs`** (append to each engine test after the edit battery):

```rust
use dbclient_lib::ddl::{ColumnDef, DdlOp};

pub async fn assert_structure_battery(d: &dyn SqlDriver) {
    let flavor = d.ddl_flavor();
    // create a fresh table via rendered DDL
    let _ = d.execute_raw(&dbclient_lib::ddl::render_ddl(
        &DdlOp::DropIndex { table: mk_table("m4_t"), name: "idx_m4".into() }, flavor)).await; // ignore error
    let _ = d.execute_raw("DROP TABLE IF EXISTS m4_t").await;
    d.execute_raw(&dbclient_lib::ddl::render_ddl(&DdlOp::CreateTable {
        name: "m4_t".into(),
        columns: vec![
            ColumnDef { name: "id".into(), data_type: "INTEGER".into(), nullable: false, is_pk: true },
            ColumnDef { name: "label".into(), data_type: "VARCHAR(32)".into(), nullable: true, is_pk: false },
        ],
    }, flavor)).await.expect("create table");

    d.execute_raw(&dbclient_lib::ddl::render_ddl(&DdlOp::AddColumn {
        table: mk_table("m4_t"),
        column: ColumnDef { name: "qty".into(), data_type: "INTEGER".into(), nullable: true, is_pk: false },
    }, flavor)).await.expect("add column");

    d.execute_raw(&dbclient_lib::ddl::render_ddl(&DdlOp::AddIndex {
        table: mk_table("m4_t"), name: "idx_m4".into(), columns: vec!["label".into()], unique: false,
    }, flavor)).await.expect("add index");

    let st = d.table_structure(&mk_table("m4_t")).await.expect("structure");
    assert_eq!(st.columns.len(), 3);
    let id = st.columns.iter().find(|c| c.name == "id").unwrap();
    assert!(id.is_pk);
    let qty = st.columns.iter().find(|c| c.name == "qty").unwrap();
    assert!(qty.nullable);
    assert!(st.indexes.iter().any(|i| i.name == "idx_m4" && i.columns == vec!["label"] && !i.unique));
}

fn mk_table(name: &str) -> dbclient_lib::drivers::TableInfo {
    dbclient_lib::drivers::TableInfo { schema: None, name: name.into(), kind: dbclient_lib::drivers::TableKind::Table }
}
```

- [ ] **Step 4:** Implement `execute_raw`/`table_structure`/`ddl_flavor` on the three drivers (move the inherent `execute_raw` bodies into the trait impls), commands `table_structure`/`preview_ddl`/`apply_ddl` + registration. All engine tests PASS.

- [ ] **Step 5: Commit:** `feat: ddl ops with flavored rendering + table structure introspection`

---

### Task 4: Frontend — SSH form section + api additions

**Files:**
- Modify: `src/lib/api.ts`, `src/components/ConnectionForm.vue`

**Interfaces:** api.ts gains:

```ts
export interface SshConfig { host: string; port: number; username: string; keyPath?: string | null }
// ConnectionConfig: ssh?: SshConfig | null
export interface StructColumn { name: string; dataType: string; nullable: boolean; default?: string | null; isPk: boolean }
export interface StructIndex { name: string; columns: string[]; unique: boolean }
export interface TableStructure { columns: StructColumn[]; indexes: StructIndex[] }
export interface ColumnDef { name: string; dataType: string; nullable: boolean; isPk: boolean }
export type DdlOp =
  | { op: "createTable"; name: string; columns: ColumnDef[] }
  | { op: "addColumn"; table: TableInfo; column: ColumnDef }
  | { op: "dropColumn"; table: TableInfo; column: string }
  | { op: "addIndex"; table: TableInfo; name: string; columns: string[]; unique: boolean }
  | { op: "dropIndex"; table: TableInfo; name: string };
tableStructure: (id, table) => invoke<TableStructure>("table_structure", { id, table }),
previewDdl: (id, op) => invoke<string>("preview_ddl", { id, op }),
applyDdl: (id, op) => invoke<void>("apply_ddl", { id, op }),
// saveConnection/testConnection gain sshSecret?: string
```

ConnectionForm: "SSH Tunnel" checkbox (hidden for sqlite) revealing host/port(22)/username/key path (optional)/secret field (labelled "Password" or "Key passphrase" depending on keyPath). On save/test pass `sshSecret`.

- [ ] **Step 1:** Implement both; `npm run build` + `npm run test` green.
- [ ] **Step 2: Commit:** `feat: ssh tunnel connection form + structure/ddl api surface`

---

### Task 5: StructureView + tab wiring + acceptance

**Files:**
- Create: `src/components/StructureView.vue`
- Modify: `src/App.vue` (Structure tab between Data and query tabs; "+ Table" button in SchemaSidebar footer or tab strip)

**Interfaces:** StructureView reads workspace store (activeId, selectedTable). Layout: columns table (name, type, nullable, default, pk badge) with per-row "drop" button; indexes list with "drop"; "+ Column" and "+ Index" inline forms; "New Table" form (name + dynamic column rows). EVERY action goes through: build `DdlOp` → `api.previewDdl` → modal showing the SQL with Execute/Cancel (`SqlPreviewModal` extended with optional execute button: add props `{ statements: string[]; executable?: boolean }`, emit `execute`) → on execute: production rail if `isProduction` (ConfirmModal) → `api.applyDdl` → refresh structure + `ws.loadTables()`.

- [ ] **Step 1:** Extend `SqlPreviewModal.vue`:

```vue
<script setup lang="ts">
defineProps<{ statements: string[]; executable?: boolean }>();
const emit = defineEmits<{ close: []; execute: [] }>();
</script>
<!-- footer buttons: Close (always) + Execute (v-if executable, blue) emitting execute -->
```

- [ ] **Step 2:** Implement `StructureView.vue` per the contract; wire the tab into `App.vue`'s strip (`mainTab === 'structure'` renders `<StructureView v-if="ws.selectedTable" />`, placeholder text otherwise); "+ Table" button lives in the tab strip next to Structure and opens the New Table form inside StructureView (prop `mode: 'table' | 'create'` or internal state toggled via an exposed method — internal `creating` ref toggled by a `startCreate()` exposed method is fine).

- [ ] **Step 3: Full sweep** (all env vars incl. `DBCLIENT_TEST_SSH=1`): `cargo test`, `npm run test`, `npm run build` — all green.

- [ ] **Step 4: Manual acceptance (spec §8/M4):**
- [ ] Create an SSH-tunneled connection (sshd container: host 127.0.0.1, port 2223, user `tunnel`, password `tunnelpass`; DB host `mysql`, port 3306, root/test) → connects and browses although port 3306 of that path is only reachable through the tunnel.
- [ ] Structure tab: create table with DDL preview; add a column; add an index; drop both.
- [ ] DDL on a production-flagged connection prompts before executing.

- [ ] **Step 5: Commit:** `feat: structure tab with ddl preview` + `docs: M4 acceptance pass`; update README (host-key TOFU note).
