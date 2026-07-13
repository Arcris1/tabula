export interface StatementRange {
  from: number;
  to: number;
  text: string;
}

const DOLLAR_TAG = /^\$[A-Za-z_][A-Za-z0-9_]*\$|^\$\$/;

export interface SplitOptions {
  /** MySQL treats \ as an escape inside strings; Postgres/SQLite do not. */
  backslashEscapes?: boolean;
  /** MySQL also treats # as a line comment. */
  hashComments?: boolean;
}

export function statements(text: string, opts: SplitOptions = {}): StatementRange[] {
  const out: StatementRange[] = [];
  let start = 0;
  let i = 0;
  const n = text.length;
  let mode: "code" | "sq" | "dq" | "bq" | "line" | "block" | "dollar" = "code";
  let dollarTag = "";
  const push = (end: number) => {
    const raw = text.slice(start, end);
    const lead = raw.length - raw.trimStart().length;
    const trail = raw.length - raw.trimEnd().length;
    if (raw.trim().length) out.push({ from: start + lead, to: end - trail, text: raw.trim() });
    start = end + 1;
  };
  while (i < n) {
    const ch = text[i],
      next = text[i + 1];
    if (mode === "code") {
      if (ch === "'") mode = "sq";
      else if (ch === '"') mode = "dq";
      else if (ch === "`") mode = "bq";
      else if (ch === "-" && next === "-") {
        mode = "line";
        i++;
      } else if (opts.hashComments && ch === "#") {
        mode = "line"; // mysql # line comment
      } else if (ch === "/" && next === "*") {
        mode = "block";
        i++;
      } else if (ch === "$") {
        // postgres dollar-quoting: $$ ... $$ or $tag$ ... $tag$
        const m = DOLLAR_TAG.exec(text.slice(i));
        if (m) {
          dollarTag = m[0];
          mode = "dollar";
          i += m[0].length - 1;
        }
      } else if (ch === ";") push(i);
    } else if (mode === "sq" || mode === "dq" || mode === "bq") {
      if (opts.backslashEscapes && ch === "\\") i++; // mysql-style escape never closes the string
      else if ((mode === "sq" && ch === "'") || (mode === "dq" && ch === '"') || (mode === "bq" && ch === "`")) mode = "code";
    } else if (mode === "line" && ch === "\n") mode = "code";
    else if (mode === "block" && ch === "*" && next === "/") {
      mode = "code";
      i++;
    } else if (mode === "dollar" && ch === "$" && text.startsWith(dollarTag, i)) {
      mode = "code";
      i += dollarTag.length - 1;
    }
    i++;
  }
  push(n);
  return out;
}

export function statementAt(text: string, pos: number, opts: SplitOptions = {}): StatementRange | null {
  const all = statements(text, opts);
  if (!all.length) return null;
  for (const s of all) if (pos >= s.from && pos <= s.to + 1) return s;
  const before = all.filter((s) => s.to < pos);
  return before.length ? before[before.length - 1] : all[0];
}
