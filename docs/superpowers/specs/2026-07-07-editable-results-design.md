# Editable Query Results — Design Spec

**Date:** 2026-07-07 · **Status:** Approved in session

A query result set becomes editable when ALL hold:
1. statement is a simple single-table SELECT (no JOIN/comma-join/GROUP BY/
   DISTINCT/HAVING/UNION/subquery-FROM/aggregates; detected on comment-stripped,
   string-blanked text; ambiguity => read-only, never guess),
2. the table has a primary key,
3. every pk column is present in the result columns.

Then the results grid gets the Data-tab behaviors: dblclick cell edit
(parseCellInput), × row delete, + Row drafts, pending bar with Preview SQL /
Commit / Discard through the EXISTING preview_changeset/apply_changeset
(optimistic guards, transaction, production rail via settings). Successful
commit re-runs that statement. When read-only, the footer says WHY
("not a simple single-table SELECT" / "<t> has no primary key" /
"include <pk> in the SELECT to edit").

Implementation: `editableTarget(sql, opts)` strict parser in sqlContext
(returns schema/table with quoted-ness; unquoted parts lowercased for
postgres); changeset logic extracted to `createChangeset()` composable
(lib/changesetCore.ts) — Pinia store delegates to it (Data tab unchanged),
each QueryEditor owns an instance, so grid and result edits never collide.

Tests: editableTarget accept/reject matrix; existing changeset tests keep
passing against the composable via the store.
