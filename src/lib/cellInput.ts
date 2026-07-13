export function parseCellInput(input: string, old: unknown): unknown {
  if (input === "NULL") return null;
  if (typeof old === "number" && input.trim() !== "" && Number.isFinite(Number(input))) return Number(input);
  return input;
}
