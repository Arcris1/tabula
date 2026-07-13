import { describe, it, expect } from "vitest";
import { parseCellInput } from "../src/lib/cellInput";

describe("parseCellInput", () => {
  it("NULL literal", () => expect(parseCellInput("NULL", "x")).toBeNull());
  it("numeric stays numeric when old was number", () => expect(parseCellInput("42", 7)).toBe(42));
  it("numeric text stays string when old was string", () => expect(parseCellInput("42", "7")).toBe("42"));
  it("non-numeric is string", () => expect(parseCellInput("abc", 7)).toBe("abc"));
});
