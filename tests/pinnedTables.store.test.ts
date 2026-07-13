import { describe, it, expect, beforeEach } from "vitest";
import { setActivePinia, createPinia } from "pinia";
import { nextTick } from "vue";
import { usePinnedTablesStore } from "../src/stores/pinnedTables";

describe("pinned tables store", () => {
  beforeEach(() => {
    localStorage.clear();
    setActivePinia(createPinia());
  });

  it("pins per connection and persists", async () => {
    const s = usePinnedTablesStore();
    s.toggle("conn-1", ".users");
    s.toggle("conn-1", ".audit_logs");
    s.toggle("conn-2", ".other");
    expect(s.list("conn-1")).toEqual([".users", ".audit_logs"]);
    expect(s.isPinned("conn-1", ".users")).toBe(true);
    expect(s.isPinned("conn-2", ".users")).toBe(false);
    await nextTick();

    setActivePinia(createPinia());
    const s2 = usePinnedTablesStore();
    expect(s2.list("conn-1")).toEqual([".users", ".audit_logs"]);
    s2.toggle("conn-1", ".users"); // unpin
    expect(s2.isPinned("conn-1", ".users")).toBe(false);
  });
});
