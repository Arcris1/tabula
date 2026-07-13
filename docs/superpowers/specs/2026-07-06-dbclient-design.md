# dbclient — Design Spec

**Date:** 2026-07-06
**Status:** Approved pending user review
**Working name:** `dbclient` (rename open)

## 1. Purpose

A native macOS desktop database client in the spirit of TablePlus, built as a
daily-use tool for the author's own databases (crm-saas MySQL, qa-intake-tool
SQLite, Redis caches/queues, plus PostgreSQL). Reliability and speed take
priority over architectural novelty.

## 2. Scope

### v1 supports

- **Engines:** MySQL/MariaDB, PostgreSQL, SQLite, Redis — all four in v1.
- **Features (built across milestones M1–M4):**
  - Connection manager with saved connections, environment colors, test button
  - Schema sidebar (SQL) / key browser (Redis)
  - Virtualized, paginated data grid with sorting and per-column filters
  - SQL editor: syntax highlighting, schema-aware autocomplete, multiple tabs,
    query history, run-statement-under-cursor, cancellable streaming results
  - Inline row editing via changeset + preview-SQL + transactional commit
  - Redis type-aware value viewing and editing with TTL
  - SSH tunneling
  - Schema editing (create/alter tables, columns, indexes) with DDL preview

### Out of scope for v1

Import/export (CSV etc.), ER diagrams, user/permission management, backup
tooling, multi-query result splitting, Windows/Linux builds (macOS only).

## 3. Architecture

**Stack:** Tauri v2 (Rust backend) + Vue 3 + TypeScript + Tailwind CSS.
Pinia for frontend state. CodeMirror 6 for the SQL editor.

**Process model:** One Tauri app. Rust owns all database connections, pooling,
SSH tunnels, and credential storage. Vue is pure UI — it never touches a
database; it calls Rust via `invoke` commands and receives JSON. Long-running
queries stream back via Tauri events so the UI stays responsive and queries
are cancellable.

```
                ┌─ Vue 3 UI (webview) ─┐
                │  invoke / events      │
                └──────────┬────────────┘
                           │
                ┌─ Rust core ───────────┐
                │  ConnectionManager    │
                │   ├─ SqlDriver trait ──── sqlx ── MySQL / Postgres / SQLite
                │   └─ KvDriver trait ───── redis crate ── Redis
                │  TunnelManager (russh) │
                │  CredentialStore       │
                └────────────────────────┘
```

### Driver abstraction — two trait families by paradigm

- **`SqlDriver`** — `list_schemas`, `list_tables`, `table_ddl`,
  `query(sql, params) -> row stream`, `apply_changes(changeset)`.
  One impl per engine over `sqlx`; engine quirks (identifier quoting, LIMIT
  syntax, type mapping) isolated inside each impl.
- **`KvDriver`** — `scan_keys(pattern, cursor)`, `get_value(key) -> typed
  value` (string/hash/list/set/zset + TTL), `set_value`, `delete_key`.
  Redis is the only v1 impl; the trait keeps the door open (e.g. Valkey).

### Key crates

`sqlx` (MySQL, Postgres, SQLite), `redis`, `russh` (SSH tunnels),
`keyring` (macOS Keychain).

### Credential storage

Connection configs (host, port, user, colors, tunnel settings) in a local
app-data file. Passwords and SSH key passphrases in the macOS Keychain via
`keyring` — never plaintext on disk.

## 4. UI Components

Three-zone window layout (TablePlus-style):

```
┌────────────────────────────────────────────────────┐
│ Toolbar: connection name/color · tab bar · actions │
├──────────┬─────────────────────────────────────────┤
│ Sidebar  │  Workspace (tabbed)                     │
│ schemas/ │  ├ Data tab: virtualized grid           │
│ tables   │  ├ Query tab: CodeMirror + results grid │
│ (Redis:  │  ├ Structure tab: columns/indexes       │
│ key tree)│  (Redis: key value viewer/editor)       │
├──────────┴─────────────────────────────────────────┤
│ Status bar: row count · query time · pending edits │
└────────────────────────────────────────────────────┘
```

- **ConnectionLauncher** — saved connection cards with env colors (red =
  production), create/edit form with Test Connection, SSH tunnel section.
- **SchemaSidebar** — schemas → tables/views tree with search. For Redis,
  swaps to **KeyBrowser**: pattern search, namespace tree from `:` separators,
  cursor-based lazy loading via `SCAN` (never `KEYS *`).
- **DataGrid** — virtualized scrolling (renders visible rows only; 100k-row
  tables stay smooth), server-side pagination (~500-row LIMIT/OFFSET batches),
  column sorting, per-column filters, typed cell rendering (NULL badge, JSON
  pretty-print, dates). M3 adds edit mode: dirty cells amber, deleted rows
  struck through, pending-changes bar with Preview SQL / Commit / Discard.
- **QueryEditor** — CodeMirror 6 with per-engine SQL dialect, schema-aware
  autocomplete (metadata fed from Rust), multiple tabs, ⌘↵ runs selection or
  statement under cursor, per-connection query history.
- **RedisValuePane** — type-aware viewer/editor: string editor with JSON
  detection, hash key-value table, list/set/zset tables with member-level
  operations, TTL display and editing.
- **TabManager** (Pinia) — open tabs per connection. One active connection per
  window in v1; parallel connections via multiple Tauri windows (state is
  per-window, so this stays cheap).

**Design language:** Linear-style — compact density, subtle borders,
keyboard-first, dark mode from day one.

## 5. Data Flow

### Connection lifecycle

`connect(connection_id)` → Rust loads config, fetches password from Keychain,
establishes SSH tunnel first if configured (local forwarded port), then opens
a small sqlx pool (or Redis client) through it. Pool handles live in a
Rust-side `ConnectionRegistry` keyed by connection ID; the frontend only ever
holds the ID. Teardown order: pools, then tunnels.

### Query execution (streaming + cancel)

`run_query(conn_id, sql, tab_id)` returns a `query_id` immediately. Rust runs
the query on a background task and emits Tauri events: `query:columns`
(names + types), `query:rows` (~200-row chunks), `query:done` (row count,
elapsed ms) or `query:error`. UI renders progressively with a Cancel button;
`cancel_query(query_id)` aborts the task and issues a server-side kill on
MySQL/Postgres.

### Data grid reads

Generated SQL through the same path: `SELECT * FROM t LIMIT 500 OFFSET n`
with engine-correct quoting, plus a row count (estimated via EXPLAIN-based
stats on Postgres/MySQL for large tables; exact on SQLite).

### Edits — changeset model (M3)

The grid never writes directly. Edits accumulate in a frontend changeset
(`updates[{table, pk, column, old, new}]`, `inserts`, `deletes`).
**Preview** renders the exact SQL in Rust. **Commit** runs it in a single
transaction with optimistic checks
(`UPDATE ... WHERE pk = ? AND old_col = old_value`); `rows_affected = 0`
means an external change → rollback and report the conflicting row.
Tables without a primary key are read-only with a visible explanation.

## 6. Error Handling

All Rust errors map to one typed shape before crossing the bridge:
`{kind, message, detail}`, kind ∈ `ConnectionFailed | TunnelFailed |
QueryError | ConflictOnCommit | Timeout | Internal`.

- SQL errors surface inline in the query tab with line/position when the
  engine provides it.
- Connection errors show on the launcher card with retry.
- Dropped connections flip a status-bar indicator with one-click reconnect,
  plus auto-reconnect on next action (idle MySQL timeouts).
- No raw panics in the UI: `Internal` errors log to
  `~/Library/Logs/dbclient/` with a "copy details" button.

### Safety rails

- Connections marked production (red) get a confirmation dialog on any
  write/DDL statement.
- `DELETE`/`UPDATE` without a `WHERE` clause always warns, in any environment.

## 7. Testing

1. **Rust unit tests** — SQL generation from changesets (per-engine quoting,
   WHERE building, conflict SQL), config serialization, Redis key-tree
   parsing, error mapping. No databases needed.
2. **Rust integration tests against real engines** — SQLite in-memory; MySQL,
   Postgres, Redis via `docker-compose.test.yml`. One shared trait-level test
   battery (list tables, read typed values, commit changeset, detect
   conflict) runs against every engine, plus engine-specific quirk tests
   (MySQL `tinyint(1)`, Postgres arrays/enums, SQLite dynamic typing).
3. **Frontend (Vitest)** — changeset store, grid virtualization edges, tab
   state. Minimal E2E via Tauri WebDriver for the connect → browse → query
   happy path only.

## 8. Milestones & Acceptance Criteria

Each milestone ends with a tool usable daily.

- **M1 — Connect & browse:** Save a connection to each real database
  (crm-saas MySQL, qa-intake-tool SQLite, a Redis instance); env colors;
  smooth scrolling on a 100k-row table; filter/sort; Redis key browsing by
  pattern without blocking the server. *Success: read-only checks no longer
  need TablePlus.*
- **M2 — Query:** Highlighted editor, working autocomplete against a live
  schema, ⌘↵ runs statement under cursor, streaming results, cancellable
  long queries, history persists across restarts.
- **M3 — Edit:** Edit/insert/delete with preview-SQL before commit; conflict
  detection demonstrably works (external change mid-edit test); no-PK tables
  read-only; Redis values editable per type with TTL.
- **M4 — Reach & structure:** Connect through an SSH tunnel to a non-exposed
  database; create a table, add a column, add an index from the Structure tab
  with generated-DDL preview.

## 9. Project Location

`~/Development/dbclient` — this repository.
