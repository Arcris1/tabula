/** A foreign key reference: the table and column this column points at. */
export interface FkRef {
  table: string;
  column: string;
}

/**
 * Parse the backend's foreign-key descriptor `"ref_table(ref_col)"` into
 * its parts. Returns null for anything that doesn't match that shape.
 */
export function parseFk(raw: string | null | undefined): FkRef | null {
  if (!raw) return null;
  const m = /^(.+)\(([^()]+)\)$/.exec(raw.trim());
  if (!m) return null;
  const table = m[1].trim();
  const column = m[2].trim();
  if (!table || !column) return null;
  return { table, column };
}
