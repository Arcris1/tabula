import { describe, it, expect } from "vitest";
import { visibleRange } from "../src/lib/virtual";

describe("visibleRange", () => {
  it("computes window with overscan", () => {
    // 28px rows, viewport 560px => 20 visible; scrolled to row 100
    const r = visibleRange(2800, 560, 28, 10000, 10);
    expect(r.start).toBe(90); // 100 - overscan
    expect(r.end).toBe(130); // 120 + overscan
  });
  it("clamps at edges", () => {
    expect(visibleRange(0, 560, 28, 10000, 10)).toEqual({ start: 0, end: 30 });
    expect(visibleRange(999999, 560, 28, 50, 10)).toEqual({ start: 40, end: 50 });
  });
  it("handles fewer rows than viewport", () => {
    expect(visibleRange(0, 560, 28, 5, 10)).toEqual({ start: 0, end: 5 });
  });
});
