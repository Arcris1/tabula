export interface RailSettings {
  warnNoWhere: boolean;
  warnDropTruncate: boolean;
  warnProductionWrites: boolean;
}

export interface RailWarning {
  title: string;
  message: string;
}

export type SqlDialect = "mysql" | "other";

const NO_WHERE_RE = /^\s*(update|delete)\b/i;
const DROP_TRUNC_RE = /^\s*(drop|truncate|rename)\b/i;
const WRITE_RE = /^\s*(insert|update|delete|alter|drop|create|truncate|replace|grant|rename|call|exec|execute|load|merge)\b/i;

/** Blanks the CONTENTS of '…' / "…" / `…` literals (quotes kept) so words
 *  inside strings — `SET note='call where applicable'` — can't fool guards. */
export function blankStrings(sql: string): string {
  let out = "";
  let mode: "code" | "sq" | "dq" | "bq" = "code";
  for (const ch of sql) {
    if (mode === "code") {
      if (ch === "'") mode = "sq";
      else if (ch === '"') mode = "dq";
      else if (ch === "`") mode = "bq";
      out += ch;
    } else if ((mode === "sq" && ch === "'") || (mode === "dq" && ch === '"') || (mode === "bq" && ch === "`")) {
      mode = "code";
      out += ch;
    } else {
      out += " ";
    }
  }
  return out;
}

/** WHERE at paren depth 0 — a WHERE inside a subquery doesn't scope the outer
 *  UPDATE/DELETE, so it must not silence the no-WHERE warning. */
function hasTopLevelWhere(v: string): boolean {
  let depth = 0;
  for (let i = 0; i < v.length; i++) {
    const ch = v[i];
    if (ch === "(") depth++;
    else if (ch === ")") depth = Math.max(0, depth - 1);
    else if (depth === 0 && (ch === "w" || ch === "W")) {
      const prev = v[i - 1];
      if (prev && /[\w$]/.test(prev)) continue; // inside another word
      if (/^where\b/i.test(v.slice(i, i + 6))) return true;
    }
  }
  return false;
}

export interface CommentOptions {
  /** MySQL also treats # as a line comment (postgres uses # as an operator). */
  hashComments?: boolean;
}

/** Removes -- , C-style and (mysql) # comments (string-aware) so guards can't be fooled by them. */
export function stripComments(sql: string, opts: CommentOptions = {}): string {
  let out = "";
  let mode: "code" | "sq" | "dq" | "bq" | "line" | "block" = "code";
  for (let i = 0; i < sql.length; i++) {
    const ch = sql[i], next = sql[i + 1];
    if (mode === "code") {
      if (ch === "-" && next === "-") { mode = "line"; i++; continue; }
      if (opts.hashComments && ch === "#") { mode = "line"; continue; }
      if (ch === "/" && next === "*") { mode = "block"; i++; continue; }
      if (ch === "'") mode = "sq";
      else if (ch === '"') mode = "dq";
      else if (ch === "`") mode = "bq";
      out += ch;
    } else if (mode === "line") {
      if (ch === "\n") { mode = "code"; out += ch; }
    } else if (mode === "block") {
      if (ch === "*" && next === "/") { mode = "code"; out += " "; i++; }
    } else {
      if ((mode === "sq" && ch === "'") || (mode === "dq" && ch === '"') || (mode === "bq" && ch === "`")) mode = "code";
      out += ch;
    }
  }
  return out;
}

/** Skips a leading `WITH name AS (...), ...` prefix so the underlying statement is classified. */
export function stripLeadingCte(sql: string): string {
  if (!/^\s*with\b/i.test(sql)) return sql;
  let i = sql.toLowerCase().indexOf("with") + 4;
  const n = sql.length;
  for (;;) {
    // skip to the opening paren of this CTE body
    let depth = 0, sawParen = false;
    while (i < n) {
      const ch = sql[i];
      if (ch === "(") { depth++; sawParen = true; i++; break; }
      i++;
    }
    if (!sawParen) return sql; // malformed: classify as-is
    while (i < n && depth > 0) {
      if (sql[i] === "(") depth++;
      else if (sql[i] === ")") depth--;
      i++;
    }
    // after a balanced body: a comma continues the CTE list, anything else is the statement
    while (i < n && /\s/.test(sql[i])) i++;
    if (sql[i] === ",") { i++; continue; }
    return sql.slice(i);
  }
}

/** Normalized view of a statement for guard classification: comments removed,
 *  string contents blanked (so paren/keyword tricks inside literals are inert),
 *  leading CTE unwrapped. */
function guardView(sql: string, dialect: SqlDialect): string {
  return stripLeadingCte(blankStrings(stripComments(sql, { hashComments: dialect === "mysql" })));
}

/** Returns ONE combined warning for the whole batch, or null when nothing is flagged. */
export function checkStatements(
  stmts: string[],
  settings: RailSettings,
  isProduction: boolean,
  dialect: SqlDialect = "other",
): RailWarning | null {
  const sections: { title: string; offenders: string[] }[] = [];
  // classify on a comment-stripped, CTE-unwrapped view; report the ORIGINAL text
  const views = stmts.map((s) => [s, guardView(s, dialect)] as const);

  if (settings.warnNoWhere) {
    const offenders = views.filter(([, v]) => NO_WHERE_RE.test(v) && !hasTopLevelWhere(v)).map(([s]) => s);
    if (offenders.length) sections.push({ title: "No WHERE clause — affects EVERY row", offenders });
  }
  if (settings.warnDropTruncate) {
    const offenders = views.filter(([, v]) => DROP_TRUNC_RE.test(v)).map(([s]) => s);
    if (offenders.length) sections.push({ title: "DROP / TRUNCATE / RENAME — hard to undo", offenders });
  }
  if (settings.warnProductionWrites && isProduction) {
    const offenders = views.filter(([, v]) => WRITE_RE.test(v)).map(([s]) => s);
    if (offenders.length) sections.push({ title: "Write on PRODUCTION", offenders });
  }

  if (!sections.length) return null;
  const title = sections.length === 1 ? sections[0].title.split(" — ")[0] : "Review dangerous statements";
  const message = sections
    .map((s) => `${s.title}:\n${s.offenders.map((o) => `  ${o}`).join(";\n")}`)
    .join("\n\n");
  return { title, message };
}
