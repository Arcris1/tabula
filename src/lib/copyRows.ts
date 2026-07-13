function cellText(v: unknown): string {
  if (v === null || v === undefined) return "";
  return typeof v === "object" ? JSON.stringify(v) : String(v);
}

/** Tab-separated with a header row — pastes cleanly into spreadsheets. */
export function rowsToTsv(columns: string[], rows: unknown[][]): string {
  const esc = (s: string) => s.replace(/\t/g, " ").replace(/\r?\n/g, " ");
  const lines = [columns.map(esc).join("\t")];
  for (const r of rows) lines.push(r.map((v) => esc(cellText(v))).join("\t"));
  return lines.join("\n");
}

function sqlLiteral(v: unknown): string {
  if (v === null || v === undefined) return "NULL";
  if (typeof v === "number") return String(v);
  if (typeof v === "boolean") return v ? "TRUE" : "FALSE";
  const s = typeof v === "object" ? JSON.stringify(v) : String(v);
  return `'${s.replace(/'/g, "''")}'`;
}

function quoteIdent(name: string): string {
  return `"${name.replace(/"/g, '""')}"`;
}

/** INSERT statements reconstructing the given rows. */
export function rowsToInsert(table: string, columns: string[], rows: unknown[][]): string {
  const cols = columns.map(quoteIdent).join(", ");
  return rows
    .map((r) => `INSERT INTO ${quoteIdent(table)} (${cols}) VALUES (${r.map(sqlLiteral).join(", ")});`)
    .join("\n");
}

/** CSV with a header row. */
export function rowsToCsv(columns: string[], rows: unknown[][]): string {
  const esc = (s: string) => (/[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s);
  const lines = [columns.map(esc).join(",")];
  for (const r of rows) lines.push(r.map((v) => esc(cellText(v))).join(","));
  return lines.join("\n");
}

/** Array-of-objects JSON. */
export function rowsToJson(columns: string[], rows: unknown[][]): string {
  const objs = rows.map((r) => Object.fromEntries(columns.map((c, i) => [c, r[i]])));
  return JSON.stringify(objs, null, 2);
}
