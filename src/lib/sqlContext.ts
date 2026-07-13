import { stripComments, type CommentOptions } from "./railChecks";

/** Table names referenced by FROM / JOIN / UPDATE / INSERT INTO in one statement. */
export function tablesInStatement(sql: string): string[] {
  const out: string[] = [];
  const re = /\b(?:from|join|straight_join|update|insert\s+into)\s+((?:[`"[]?[\w$]+[`"\]]?\.)*[`"[]?[\w$]+[`"\]]?)/gi;
  let m: RegExpExecArray | null;
  while ((m = re.exec(sql))) {
    let name = m[1].replace(/[`"[\]]/g, "");
    const dot = name.lastIndexOf(".");
    if (dot >= 0) name = name.slice(dot + 1);
    name = name.toLowerCase();
    if (name && !out.includes(name)) out.push(name);
  }
  return out;
}

export interface EditableTarget {
  schema: string | null;
  table: string;
  schemaQuoted: boolean;
  tableQuoted: boolean;
}

const BLOCKERS = /\b(join|group\s+by|having|union|intersect|except|distinct|into)\b/i;
const AGGREGATES = /\b(count|sum|avg|min|max|array_agg|string_agg|group_concat)\s*\(/i;

function blankStrings(sql: string): string {
  return sql
    .replace(/'(?:[^']|'')*'/g, "''")
    .replace(/"(?:[^"]|"")*"/g, '"?"')
    .replace(/`[^`]*`/g, "`?`");
}

function unquote(part: string): { name: string; quoted: boolean } {
  const quoted = /^[`"[]/.test(part);
  return { name: part.replace(/[`"[\]]/g, ""), quoted };
}

/**
 * The single table a SELECT reads from, when the result is safely editable.
 * Conservative: any JOIN / comma-join / aggregate / subquery / set-op => null.
 */
export function editableTarget(sql: string, opts: CommentOptions = {}): EditableTarget | null {
  const clean = stripComments(sql, opts);
  const blanked = blankStrings(clean);
  if (!/^\s*select\b/i.test(blanked)) return null;
  if (BLOCKERS.test(blanked) || AGGREGATES.test(blanked)) return null;
  if (/\(\s*select\b/i.test(blanked)) return null; // subquery anywhere
  const froms = blanked.match(/\bfrom\b/gi) ?? [];
  if (froms.length !== 1) return null;

  // capture the table token (optionally schema-qualified) right after FROM
  const m = /\bfrom\s+((?:[`"[]?[\w$]+[`"\]]?\.)?[`"[]?[\w$]+[`"\]]?)([\s\S]*)$/i.exec(clean);
  if (!m) return null;
  const rest = blankStrings(m[2]).trimStart();
  if (rest.startsWith(",")) return null; // comma join

  const parts = m[1].split(".");
  if (parts.length > 2) return null;
  const table = unquote(parts[parts.length - 1]);
  const schema = parts.length === 2 ? unquote(parts[0]) : null;
  return {
    schema: schema?.name ?? null,
    table: table.name,
    schemaQuoted: schema?.quoted ?? false,
    tableQuoted: table.quoted,
  };
}
