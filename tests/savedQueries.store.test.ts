import { describe, it, expect, beforeEach } from "vitest";
import { setActivePinia, createPinia } from "pinia";
import { nextTick } from "vue";
import { useSavedQueriesStore } from "../src/stores/savedQueries";

describe("saved queries store", () => {
  beforeEach(() => {
    localStorage.clear();
    setActivePinia(createPinia());
  });

  it("saves, renames, removes and persists", async () => {
    const s = useSavedQueriesStore();
    s.save("stuck leads", "SELECT * FROM leads WHERE status = 'STUCK'");
    s.save("today logins", "SELECT count(*) FROM audit_logs");
    expect(s.queries).toHaveLength(2);
    expect(s.queries[0].name).toBe("today logins"); // newest first
    const id = s.queries[1].id;
    s.rename(id, "stuck-leads-v2");
    await nextTick();

    // reload from storage
    setActivePinia(createPinia());
    const s2 = useSavedQueriesStore();
    expect(s2.queries).toHaveLength(2);
    expect(s2.queries.find((q) => q.id === id)?.name).toBe("stuck-leads-v2");

    s2.remove(id);
    expect(s2.queries).toHaveLength(1);
  });

  it("saving an existing name overwrites it instead of duplicating", () => {
    const s = useSavedQueriesStore();
    s.save("leads", "SELECT * FROM leads");
    s.save("leads", "SELECT * FROM leads WHERE status = 'NEW'");
    expect(s.queries).toHaveLength(1);
    expect(s.queries[0].sql).toBe("SELECT * FROM leads WHERE status = 'NEW'");
  });

  it("isSaved matches exact SQL ignoring surrounding whitespace", () => {
    const s = useSavedQueriesStore();
    s.save("vicidial users", "SELECT * FROM vicidial_users;");
    expect(s.isSaved("SELECT * FROM vicidial_users;")).toBe(true);
    expect(s.isSaved("  SELECT * FROM vicidial_users;\n")).toBe(true);
    expect(s.isSaved("SELECT * FROM leads;")).toBe(false);
    expect(s.isSaved("   ")).toBe(false);
  });

  it("survives corrupted storage", () => {
    localStorage.setItem("dbclient.savedQueries", "{bad");
    setActivePinia(createPinia());
    const s = useSavedQueriesStore();
    expect(s.queries).toEqual([]);
  });
});
