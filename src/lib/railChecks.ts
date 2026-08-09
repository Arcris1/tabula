export interface RailSettings {
  warnNoWhere: boolean;
  warnDropTruncate: boolean;
  warnProductionWrites: boolean;
}

export interface RailWarning {
  title: string;
  message: string;
  /** True when the batch is irreversible (DROP/TRUNCATE/ALTER…DROP) or targets
   *  production — the caller should demand a typed confirmation, not one click. */
  severe: boolean;
}

export type SqlDialect = "mysql" | "other";

const NO_WHERE_RE = /^\s*(update|delete)\b/i;
const DROP_TRUNC_RE = /^\s*(drop|truncate|rename)\b/i;
// ALTER … DROP … removes a column/constraint — as irreversible as DROP/TRUNCATE
// but hidden behind a leading ALTER, so the anchored DROP_TRUNC_RE misses it.
const ALTER_DROP_RE = /^\s*alter\b[\s\S]*\bdrop\b/i;
const WRITE_RE = /^\s*(insert|update|delete|alter|drop|create|truncate|replace|grant|rename|call|exec|execute|load|merge)\b/i;
// A routine/trigger definition is classified as a single unit — its body's inner
// statements must NOT be split out and individually flagged (false positives).
const ROUTINE_DEF_RE = /^\s*(create|alter)\s+(or\s+replace\s+)?(proc|procedure|function|trigger)\b/i;

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

/** A same-LENGTH shadow of the SQL with string contents and comment bodies
 *  replaced by spaces (delimiters kept). Lets us scan for structural characters
 *  (`;`, `(`, `)`) at real code positions while indices still map 1:1 to the
 *  original text, so we can slice the original on those positions. */
function codeShadow(sql: string, opts: CommentOptions = {}): string {
  let out = "";
  let mode: "code" | "sq" | "dq" | "bq" | "line" | "block" = "code";
  for (let i = 0; i < sql.length; i++) {
    const ch = sql[i], next = sql[i + 1];
    if (mode === "code") {
      if (ch === "-" && next === "-") { mode = "line"; out += "  "; i++; continue; }
      if (opts.hashComments && ch === "#") { mode = "line"; out += " "; continue; }
      if (ch === "/" && next === "*") { mode = "block"; out += "  "; i++; continue; }
      if (ch === "'") { mode = "sq"; out += ch; continue; }
      if (ch === '"') { mode = "dq"; out += ch; continue; }
      if (ch === "`") { mode = "bq"; out += ch; continue; }
      out += ch;
    } else if (mode === "line") {
      out += ch === "\n" ? "\n" : " ";
      if (ch === "\n") mode = "code";
    } else if (mode === "block") {
      if (ch === "*" && next === "/") { mode = "code"; out += "  "; i++; } else out += ch === "\n" ? "\n" : " ";
    } else {
      // inside a string literal
      if ((mode === "sq" && ch === "'") || (mode === "dq" && ch === '"') || (mode === "bq" && ch === "`")) {
        mode = "code"; out += ch;
      } else out += " ";
    }
  }
  return out;
}

/** Splits a batch into individual statements on top-level (paren-depth-0)
 *  semicolons, ignoring semicolons inside strings, comments or parentheses.
 *  Returns the ORIGINAL (non-blanked) slices, empties dropped. */
export function splitTopLevel(sql: string, opts: CommentOptions = {}): string[] {
  const shadow = codeShadow(sql, opts);
  const parts: string[] = [];
  let start = 0, depth = 0;
  for (let i = 0; i < shadow.length; i++) {
    const ch = shadow[i];
    if (ch === "(") depth++;
    else if (ch === ")") depth = Math.max(0, depth - 1);
    else if (ch === ";" && depth === 0) {
      const piece = sql.slice(start, i).trim();
      if (piece) parts.push(piece);
      start = i + 1;
    }
  }
  const tail = sql.slice(start).trim();
  if (tail) parts.push(tail);
  return parts;
}

export interface CommentOptions {
  /** MySQL also treats # as a line comment (postgres uses # as an operator). */
  hashComments?: boolean;
}

/** Removes -- , C-style and (mysql) # comments (string-aware) so guards can't be fooled by them.
 *  Exception: MySQL executable comments `/*! … *​/` (optionally `/*!50000 …`) are
 *  RUN by the server, so their contents are kept as code — otherwise a
 *  `/*! DELETE FROM users *​/` would slip past the no-WHERE rail. */
export function stripComments(sql: string, opts: CommentOptions = {}): string {
  let out = "";
  let mode: "code" | "sq" | "dq" | "bq" | "line" | "block" = "code";
  let execDepth = 0;
  for (let i = 0; i < sql.length; i++) {
    const ch = sql[i], next = sql[i + 1];
    if (mode === "code") {
      if (execDepth > 0 && ch === "*" && next === "/") { execDepth--; i++; continue; } // close of /*! … */
      if (ch === "-" && next === "-") { mode = "line"; i++; continue; }
      if (opts.hashComments && ch === "#") { mode = "line"; continue; }
      if (ch === "/" && next === "*") {
        if (opts.hashComments && sql[i + 2] === "!") {
          i += 2; // skip "/*!"
          while (i + 1 < sql.length && /\d/.test(sql[i + 1]!)) i++; // optional version prefix
          execDepth++;
          continue;
        }
        mode = "block"; i++; continue;
      }
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
  let severe = false;
  const hashComments = dialect === "mysql";
  // Expand each batch into individual statements: MSSQL ships whole `;`-batches
  // as one string, so classifying only the first token would miss a wrapped
  // `SET NOCOUNT ON; DELETE …`. A routine/trigger definition stays one unit so
  // its body's inner statements aren't split out and falsely flagged.
  const units = stmts.flatMap((s) =>
    ROUTINE_DEF_RE.test(stripComments(s, { hashComments })) ? [s] : splitTopLevel(s, { hashComments }),
  );
  // classify on a comment-stripped, CTE-unwrapped view; report the ORIGINAL text
  const views = units.map((s) => [s, guardView(s, dialect)] as const);

  if (settings.warnNoWhere) {
    const offenders = views.filter(([, v]) => NO_WHERE_RE.test(v) && !hasTopLevelWhere(v)).map(([s]) => s);
    if (offenders.length) sections.push({ title: "No WHERE clause — affects EVERY row", offenders });
  }
  if (settings.warnDropTruncate) {
    const offenders = views.filter(([, v]) => DROP_TRUNC_RE.test(v) || ALTER_DROP_RE.test(v)).map(([s]) => s);
    if (offenders.length) { sections.push({ title: "DROP / TRUNCATE / RENAME — hard to undo", offenders }); severe = true; }
  }
  if (settings.warnProductionWrites && isProduction) {
    const offenders = views.filter(([, v]) => WRITE_RE.test(v)).map(([s]) => s);
    if (offenders.length) { sections.push({ title: "Write on PRODUCTION", offenders }); severe = true; }
  }

  if (!sections.length) return null;
  const title = sections.length === 1 ? sections[0].title.split(" — ")[0] : "Review dangerous statements";
  const message = sections
    .map((s) => `${s.title}:\n${s.offenders.map((o) => `  ${o}`).join(";\n")}`)
    .join("\n\n");
  return { title, message, severe };
}
