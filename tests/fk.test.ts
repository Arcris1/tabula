import { describe, it, expect } from "vitest";
import { parseFk } from "../src/lib/fk";

describe("parseFk", () => {
  it("parses table(col)", () => {
    expect(parseFk("users(id)")).toEqual({ table: "users", column: "id" });
  });
  it("trims whitespace", () => {
    expect(parseFk("  audit_logs ( user_id ) ")).toEqual({ table: "audit_logs", column: "user_id" });
  });
  it("returns null for null/empty/garbage", () => {
    expect(parseFk(null)).toBeNull();
    expect(parseFk(undefined)).toBeNull();
    expect(parseFk("")).toBeNull();
    expect(parseFk("no_parens")).toBeNull();
    expect(parseFk("()")).toBeNull();
    expect(parseFk("t()")).toBeNull();
  });
});
