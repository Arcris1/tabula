# Shortcuts + Multi-Window Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cross-platform (⌘/Ctrl) keyboard shortcuts for every existing action plus independent multi-window support with per-window backend isolation.

**Architecture:** Pure `keymap.ts` + Pinia shortcuts registry + one App-level keydown listener; Rust registries re-keyed by `{window_label}:{connection_id}` with sweep helpers, `open_new_window` command, Destroyed-event cleanup.

**Tech Stack:** Existing stack; no new dependencies.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-07-07-shortcuts-multiwindow-design.md` — binding table is normative.
- mod = meta on mac, ctrl elsewhere; combos with mod fire inside inputs; Esc handled per-component.
- No behavior changes to the actions themselves (commit keeps production guard, etc.).
- Commit after every task; all existing suites stay green.

---

### Task 1: Rust — scoped registries + window-aware commands + open_new_window

**Files:** Modify `src-tauri/src/registry.rs`, `tunnel.rs`, `queries.rs` (no change needed — key is caller-provided), `commands.rs`, `lib.rs`.

**Interfaces:**
- `ConnectionRegistry` + `TunnelRegistry` gain `remove_by_connection(&self, id: &str)` (removes keys ending `:{id}`) and `remove_by_window(&self, label: &str)` (removes keys starting `{label}:`).
- `commands.rs` helper: `fn scoped(window: &tauri::Window, id: &str) -> String { format!("{}:{}", window.label(), id) }`.
- Every command that touches `state.registry`/`state.tunnels` by id gains `window: tauri::Window` and uses `scoped(...)`: `connect`, `disconnect`, `list_tables`, `fetch_rows`, `scan_keys`, `get_redis_value`, `schema_columns`, `run_query` (already has window; RunningQuery.connection_id stores the scoped key), `table_structure`, `preview_changeset`, `apply_changeset`, `preview_ddl`, `apply_ddl`, `set_redis_value`, `delete_redis_key`, `set_redis_ttl`.
- `delete_connection` → `remove_by_connection` on both registries.
- `connect` sets window title `dbclient — {name}`; `disconnect` resets to `dbclient`.
- New command `open_new_window(app: tauri::AppHandle)` → WebviewWindowBuilder, label `win-{uuid}`, inner_size 1200×800, title `dbclient`.
- `lib.rs`: `.on_window_event` → on `WindowEvent::Destroyed`, spawn task calling `remove_by_window(label)` on both registries.

**Steps:**
- [ ] Registry unit tests (two labels same conn → remove_by_window keeps the other; remove_by_connection clears both) → FAIL → implement helpers → PASS.
- [ ] Rewire commands + lib.rs; `cargo test` fully green (integration tests unaffected — they use drivers directly).
- [ ] Commit `feat: per-window connection scoping + open_new_window`.

### Task 2: keymap.ts + tests

**Interfaces:**
```ts
export type Platform = "mac" | "other";
export function detectPlatform(): Platform;                     // navigator.platform/userAgent
export function comboOf(e: KeyboardEvent): string | null;       // "meta+shift+d", "ctrl+tab", "f5"; null for bare modifiers
export function bindings(p: Platform): Record<string, string>;  // combo -> action name (mod resolved per platform)
export function displayCombo(combo: string, p: Platform): string; // "meta+shift+d" -> "⌘⇧D" / "Ctrl+Shift+D"
export const SHORTCUT_TABLE: { action: string; combo: string; label: string; context: string }[]; // for help modal (combo uses "mod")
```
Action names: `launcher:new`, `tabs:new`, `tabs:close`, `tabs:data`, `tabs:structure`, `tabs:index:<n>` (n=3..9 → query tab n-3), `tabs:next`, `tabs:prev`, `view:refresh`, `grid:commit`, `grid:discard`, `grid:insertRow`, `grid:focusFilter`, `sidebar:focusFilter`, `history:toggle`, `query:cancel`, `workspace:disconnect`, `window:new`, `help:toggle`.

- [ ] Tests: comboOf normalization (meta on mac, ctrl+tab, f5, ignores bare shift), bindings resolve mod per platform (mac: `meta+s`→grid:commit; other: `ctrl+s`), displayCombo glyphs both platforms → implement → PASS → commit `feat: cross-platform keymap`.

### Task 3: shortcuts store + tests

```ts
useShortcutsStore(): { register(action, fn): () => void; dispatch(action): boolean }
// stack per action; dispatch invokes the LAST registered handler; returns false when none.
```
- [ ] Tests (register/dispatch/unregister restores previous, unknown action → false) → implement → commit `feat: shortcuts registry store`.

### Task 4: App wiring + component registrations + help modal + api

- api.ts: `openNewWindow: () => invoke<void>("open_new_window")`.
- `ShortcutHelpModal.vue`: renders SHORTCUT_TABLE with displayCombo(platform); Esc/Close closes.
- App.vue: keydown listener → comboOf → bindings[combo] → context resolution:
  - `view:refresh` → `grid:refresh` | `structure:refresh` | `query:rerun:${mainTab}`
  - `query:cancel` → `query:cancel:${mainTab}`; `grid:*`/`tabs:*` only for SQL paradigm; `launcher:new` only when `!ws.isOpen`.
  - implements tabs actions, history toggle, disconnect (closeWorkspace), window:new, help:toggle locally.
  - listener skips plain-key combos when target is input/textarea/contenteditable (mod-combos pass).
- Registrations: ConnectionLauncher (`launcher:new`), SchemaSidebar (`sidebar:focusFilter` with input ref), DataGrid (`grid:refresh|commit|discard|insertRow|focusFilter`), StructureView (`structure:refresh`), QueryEditor (`query:cancel:${tab.id}`, `query:rerun:${tab.id}` re-running `tab.run?.sql ?? current statement`).
- CodeMirror already owns mod+Enter; ensure editor doesn't swallow the global combos it doesn't use (listener is on window with capture=false — CM stops propagation only for bound keys; acceptable).
- [ ] `npm run build` + all tests green → commit `feat: global shortcuts + help modal + new window button`.

### Task 5: Sweep + acceptance

- [ ] Full sweep: cargo (all env vars) + vitest + build.
- [ ] Manual: mod+T/W/1..9/Tab cycling; mod+S commits with prod guard; mod+Shift+N opens an independent second window; same connection open in both windows, disconnect in one → other keeps browsing; window title shows connection name; mod+/ cheat sheet on both.
- [ ] Commit `docs: shortcuts + multiwindow acceptance`.
