import { describe, it, expect } from "vitest";
import { parseCellInput } from "../src/lib/cellInput";

describe("parseCellInput", () => {
  it("NULL sentinel only when the cell was already null", () => {
    expect(parseCellInput("NULL", null)).toBeNull();          // seeded edit box round-trips
    expect(parseCellInput("NULL", "x")).toBe("NULL");          // literal string, not a wipe
    expect(parseCellInput("NULL", "NULL")).toBe("NULL");       // the Marites corruption case
    expect(parseCellInput("NULL", 7)).toBe("NULL");
  });
  it("numeric stays numeric when old was number", () => expect(parseCellInput("42", 7)).toBe(42));
  it("numeric text stays string when old was string", () => expect(parseCellInput("42", "7")).toBe("42"));
  it("non-numeric is string", () => expect(parseCellInput("abc", 7)).toBe("abc"));
});
