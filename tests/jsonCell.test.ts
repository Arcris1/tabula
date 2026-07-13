import { describe, it, expect } from "vitest";
import { asJson } from "../src/lib/jsonCell";

describe("asJson", () => {
  it("returns parsed objects/arrays", () => {
    expect(asJson({ a: 1 })).toEqual({ a: 1 });
    expect(asJson('{"a":1}')).toEqual({ a: 1 });
    expect(asJson("[1,2,3]")).toEqual([1, 2, 3]);
  });
  it("returns null for non-json", () => {
    expect(asJson("hello")).toBeNull();
    expect(asJson(42)).toBeNull();
    expect(asJson(null)).toBeNull();
    expect(asJson("{not json")).toBeNull();
  });
});
