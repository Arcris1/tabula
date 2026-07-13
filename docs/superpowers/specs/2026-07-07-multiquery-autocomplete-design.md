# Multi-Statement Queries + Statement-Aware Autocomplete — Design Spec

**Date:** 2026-07-07 · **Status:** Approved in session

## A. Multi-statement execution

- Statements are split client-side with the existing `statements()` lexer and
  executed SEQUENTIALLY, one `run_query` (one prepared statement) each.
  Backend unchanged.
- Run semantics: ⌘↵ / Run with a selection → all statements in the selection;
  without a selection → statement under the cursor only (unchanged).
- `QueryTab` holds `runs: QueryRun[]` + `activeRunIndex`. While executing,
  the active index follows the running statement. When `runs.length > 1` a
  segmented bar above the results switches result sets
  (`Result 1 (5 rows) · Result 2 ✕ …`). Non-SELECTs show "OK — N affected".
- First failing statement stops the batch; earlier results stay; the failed
  slot shows the error. Cancel cancels the current statement and drops the
  rest of the queue.
- Safety rails run per statement UP FRONT for the whole batch: one modal lists
  the offending statements (WHERE-less and/or production writes); a single
  "Run anyway" acknowledges the batch.
- History records one entry per executed statement (existing mechanism).

## B. Statement-aware column autocomplete

- `src/lib/sqlContext.ts`: `tablesInStatement(sql): string[]` — table names
  after FROM / JOIN / UPDATE / INSERT INTO; strips backticks/quotes; for
  `schema.table` returns the bare table name; lowercased, deduped. Unit-tested.
- QueryEditor adds a second CodeMirror completion source (registered via
  `lang.language.data.of({ autocomplete })`): on a bare word, offer the
  columns of exactly the tables in the statement under the cursor,
  `type: "property"`, `detail: <table>`, boosted above the built-in
  suggestions. `table.` completion via lang-sql continues to work.

## Testing

Vitest: `sqlContext` extraction cases; queries store reworked tests
(sequential multi-run resolves in order, error stops batch, result switching,
events routed to the right run). Rust untouched; all suites stay green.
