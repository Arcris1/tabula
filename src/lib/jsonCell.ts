/** Returns a parsed JSON value if `v` is (or stringifies to) a JSON object/array, else null. */
export function asJson(v: unknown): unknown | null {
  if (v && typeof v === "object") return v; // jsonb arrives already parsed
  if (typeof v === "string") {
    const t = v.trim();
    if ((t.startsWith("{") && t.endsWith("}")) || (t.startsWith("[") && t.endsWith("]"))) {
      try {
        return JSON.parse(t);
      } catch {
        return null;
      }
    }
  }
  return null;
}
