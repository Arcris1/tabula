import { describe, it, expect, vi, beforeEach } from "vitest";
import { setActivePinia, createPinia } from "pinia";
import { useShortcutsStore } from "../src/stores/shortcuts";

describe("shortcuts store", () => {
  beforeEach(() => setActivePinia(createPinia()));

  it("dispatches the last registered handler", () => {
    const s = useShortcutsStore();
    const a = vi.fn();
    const b = vi.fn();
    s.register("grid:commit", a);
    const unb = s.register("grid:commit", b);
    expect(s.dispatch("grid:commit")).toBe(true);
    expect(b).toHaveBeenCalledOnce();
    expect(a).not.toHaveBeenCalled();

    unb();
    expect(s.dispatch("grid:commit")).toBe(true);
    expect(a).toHaveBeenCalledOnce();
  });

  it("unknown action returns false", () => {
    const s = useShortcutsStore();
    expect(s.dispatch("nope")).toBe(false);
  });
});
