/**
 * Interprets text typed into a cell editor.
 *
 * "NULL" is a sentinel ONLY when the cell was already NULL — the edit box is
 * seeded with "NULL" for null cells, so blur/Enter must round-trip to null
 * without queuing a change. For any non-null cell, "NULL" is the literal
 * string: a cell that genuinely contains 'NULL' (dirty imports) must never be
 * silently converted to SQL NULL by a stray double-click + blur. Setting a
 * non-null cell to NULL is an explicit action (the ∅ button in the inspector).
 */
export function parseCellInput(input: string, old: unknown): unknown {
  if (input === "NULL" && old === null) return null;
  if (typeof old === "number" && input.trim() !== "" && Number.isFinite(Number(input))) return Number(input);
  return input;
}
