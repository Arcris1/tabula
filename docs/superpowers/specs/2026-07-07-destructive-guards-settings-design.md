# Destructive-Statement Guards + Settings — Design Spec

**Date:** 2026-07-07 · **Status:** Approved in session

## Guards (each toggleable, default ON)

1. `warnNoWhere` — UPDATE/DELETE without WHERE (existing rail, now toggleable)
2. `warnDropTruncate` — any DROP/TRUNCATE statement, on any connection (NEW)
3. `warnProductionWrites` — any write on a production connection (existing;
   toggle also governs DataGrid commit + Structure-tab prompts)

Anti-annoyance: ONE modal per run batch, sections grouped by reason, single
"Run anyway". Structure tab keeps only the production guard (its preview →
Execute flow is already an explicit confirmation).

## Settings

- Pinia `settings` store persisted to localStorage (`dbclient.settings`),
  shared across windows, defaults all true.
- `SettingsModal.vue`: three toggles with explanations. Opened via gear icon
  (launcher + workspace header) and `mod+,` (added to keymap + cheat sheet).

## Implementation plan (small)

1. `src/lib/railChecks.ts` (pure) + tests:
   `checkStatements(stmts, settings, isProduction) -> {title, message} | null`
   — sections: "No WHERE clause", "DROP / TRUNCATE", "Write on PRODUCTION".
2. `src/stores/settings.ts` + tests (defaults, persistence roundtrip).
3. `SettingsModal.vue`; gear buttons; keymap `mod+,` → `settings:toggle`;
   cheat-sheet row.
4. QueryEditor: replace chained rails with single railChecks modal
   (one ack per batch). DataGrid/StructureView production prompts respect
   `warnProductionWrites`.
5. Full test sweep; merge; release rebuild.
