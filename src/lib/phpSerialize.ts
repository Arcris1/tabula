/**
 * Parser for PHP's serialize() format (what Laravel stores in Redis for
 * sessions/cache). Byte-accurate: `s:N:"…"` counts BYTES, not characters, so
 * parsing happens over UTF-8 bytes and slices are decoded back to strings.
 */

const enc = new TextEncoder();
const dec = new TextDecoder();

class Cursor {
  constructor(public b: Uint8Array, public i = 0) {}
  peek(): number {
    if (this.i >= this.b.length) throw new Error("eof");
    return this.b[this.i];
  }
  take(): number {
    const c = this.peek();
    this.i++;
    return c;
  }
  expect(ch: string) {
    if (this.take() !== ch.charCodeAt(0)) throw new Error(`expected ${ch}`);
  }
  /** ascii chars up to (not including) `stop`; consumes the stop char */
  until(stop: string): string {
    const s = stop.charCodeAt(0);
    let out = "";
    for (;;) {
      const c = this.take();
      if (c === s) return out;
      out += String.fromCharCode(c);
    }
  }
  bytes(n: number): string {
    if (this.i + n > this.b.length) throw new Error("eof");
    const s = dec.decode(this.b.subarray(this.i, this.i + n));
    this.i += n;
    return s;
  }
}

function parseString(c: Cursor): string {
  c.expect(":");
  const len = parseInt(c.until(":"), 10);
  if (!Number.isFinite(len) || len < 0) throw new Error("bad length");
  c.expect('"');
  const s = c.bytes(len);
  c.expect('"');
  c.expect(";");
  return s;
}

function entriesToValue(entries: [unknown, unknown][]): unknown {
  const sequential = entries.every(([k], idx) => k === idx);
  if (sequential) return entries.map(([, v]) => v);
  const obj: Record<string, unknown> = {};
  for (const [k, v] of entries) obj[String(k)] = v;
  return obj;
}

function parseValue(c: Cursor): unknown {
  const t = String.fromCharCode(c.take());
  switch (t) {
    case "N":
      c.expect(";");
      return null;
    case "b": {
      c.expect(":");
      const v = c.take() === "1".charCodeAt(0);
      c.expect(";");
      return v;
    }
    case "i": {
      c.expect(":");
      const n = Number(c.until(";"));
      if (!Number.isFinite(n)) throw new Error("bad int");
      return n;
    }
    case "d": {
      c.expect(":");
      const raw = c.until(";");
      if (raw === "INF") return Infinity;
      if (raw === "-INF") return -Infinity;
      if (raw === "NAN") return NaN;
      const n = Number(raw);
      if (!Number.isFinite(n)) throw new Error("bad float");
      return n;
    }
    case "s":
      return parseString(c);
    case "a": {
      c.expect(":");
      const count = parseInt(c.until(":"), 10);
      c.expect("{");
      const entries: [unknown, unknown][] = [];
      for (let k = 0; k < count; k++) {
        entries.push([parseValue(c), parseValue(c)]);
      }
      c.expect("}");
      return entriesToValue(entries);
    }
    case "O": {
      const cls = parseStringNoSemi(c);
      c.expect(":");
      const count = parseInt(c.until(":"), 10);
      c.expect("{");
      const obj: Record<string, unknown> = { __class: cls };
      for (let k = 0; k < count; k++) {
        const key = parseValue(c);
        obj[String(key).replace(/\0.*\0/, "")] = parseValue(c); // strip prop visibility markers
      }
      c.expect("}");
      return obj;
    }
    default:
      throw new Error(`unknown type ${t}`);
  }
}

/** like parseString but for `O:LEN:"Class"` (no trailing `;`) */
function parseStringNoSemi(c: Cursor): string {
  c.expect(":");
  const len = parseInt(c.until(":"), 10);
  c.expect('"');
  const s = c.bytes(len);
  c.expect('"');
  return s;
}

const SNIFF = /^(?:[abisdO]:|N;)/;

/** Strict parse of a complete PHP-serialized value; null if it isn't one. */
export function phpUnserialize(text: string): { ok: boolean; value?: unknown } {
  const t = text.trim();
  if (!SNIFF.test(t)) return { ok: false };
  const c = new Cursor(enc.encode(t));
  try {
    const value = parseValue(c);
    if (c.i !== c.b.length) return { ok: false }; // trailing garbage → not serialized data
    return { ok: true, value };
  } catch {
    return { ok: false };
  }
}

/** After parsing, string leaves may themselves be serialized (Laravel double-
 *  serializes session payloads: s:N:"a:8:{…}"). Resolve those, bounded. */
function resolveNested(v: unknown, depth: number): unknown {
  if (depth <= 0) return v;
  if (typeof v === "string" && SNIFF.test(v.trim())) {
    const inner = phpUnserialize(v);
    if (inner.ok) return resolveNested(inner.value, depth - 1);
    return v;
  }
  if (Array.isArray(v)) return v.map((x) => resolveNested(x, depth - 1));
  if (v && typeof v === "object") {
    const out: Record<string, unknown> = {};
    for (const [k, val] of Object.entries(v)) out[k] = resolveNested(val, depth - 1);
    return out;
  }
  return v;
}

/** Best-effort pretty value for display: parsed (with nested layers resolved),
 *  or null when the text isn't PHP-serialized at all. */
export function tryPhpFormat(text: string): unknown | null {
  const r = phpUnserialize(text);
  if (!r.ok) return null;
  return resolveNested(r.value, 6);
}
