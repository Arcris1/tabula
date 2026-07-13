# Per-Tab Sessions (Real Transaction Support) — Design Spec

**Date:** 2026-07-07 · **Status:** Approved in session

**Bug being fixed:** every statement acquired a fresh pooled connection, so
`BEGIN; DELETE …; ROLLBACK;` ran on three different connections — the DELETE
autocommitted and the ROLLBACK was a no-op while everything reported OK.

## Design

- Each query tab owns ONE persistent `QuerySession` (TablePlus model), keyed
  `{window_label}:{connection_id}:{tab_id}` in a new `SessionRegistry`.
- `run_query` gains `tab_id`: take the tab's session (or acquire on first
  use), stream, put it back when the statement finishes. SQL errors keep the
  session (you must be able to ROLLBACK after a failed statement).
- New `close_session(id, tab_id)` command, called when a query tab closes.
  Disconnect / connection delete / window destroy sweep their sessions.
  Discarded sessions get a best-effort `ROLLBACK` before dropping so no dirty
  transaction is ever returned to the pool.
- Teardown order: sessions → pools → tunnels.
- MySQL/Postgres pool `max_connections` 4 → 10 (each open query tab holds one
  connection).
- Caveat (inherent to real transactions, same as TablePlus): an open
  transaction holds locks until COMMIT/ROLLBACK or tab close.

## Testing

- Rust: `SessionRegistry` unit tests (take/put, prefix/infix sweeps returning
  sessions); per-engine transaction battery — on ONE session:
  BEGIN → DELETE → ROLLBACK ⇒ row survives; BEGIN → DELETE → COMMIT ⇒ row gone.
- Frontend: store passes `tabId` to runQuery; closing a tab calls
  `closeSession`.

## Addendum: TX indicator (approved in session)

- `txEffect(sql)` heuristic: BEGIN/START → open; COMMIT / ROLLBACK (but not
  ROLLBACK TO SAVEPOINT) → closed. Applied only when the statement succeeds.
- QueryTab.txOpen drives an amber "TX" chip on the tab; closing a TX-open tab
  prompts (close = ROLLBACK, which the backend already performs on
  close_session). MySQL implicit-commit DDL is not tracked (badge may stay on
  until an explicit COMMIT/ROLLBACK — conservative direction).
