import { describe, it, expect, beforeEach } from "vitest";
import { setActivePinia, createPinia } from "pinia";
import { useChangesetStore } from "../src/stores/changeset";

const COLS = ["id", "name", "age"];
const row1 = [1, "Alice", 30];

describe("changeset store", () => {
  beforeEach(() => setActivePinia(createPinia()));

  it("no pk means not editable", () => {
    const s = useChangesetStore();
    s.setContext([]);
    expect(s.editable).toBe(false);
  });

  it("merges edits per row and builds a changeset", () => {
    const s = useChangesetStore();
    s.setContext(["id"]);
    s.editCell(row1, COLS, "name", "Alicia");
    s.editCell(row1, COLS, "age", 31);
    s.editCell(row1, COLS, "name", "Aly"); // second edit of same cell replaces
    expect(s.count).toBe(1); // one dirty row
    const cs = s.toChangeset();
    expect(cs.updates).toHaveLength(1);
    expect(cs.updates[0].pk).toEqual({ id: 1 });
    expect(cs.updates[0].changes).toEqual([
      { column: "name", old: "Alice", new: "Aly" },
      { column: "age", old: 30, new: 31 },
    ]);
  });

  it("reverting to the original value clears the change", () => {
    const s = useChangesetStore();
    s.setContext(["id"]);
    s.editCell(row1, COLS, "name", "Alicia");
    s.editCell(row1, COLS, "name", "Alice");
    expect(s.count).toBe(0);
  });

  it("delete toggling and drafts", () => {
    const s = useChangesetStore();
    s.setContext(["id"]);
    s.markDelete(row1, COLS);
    expect(s.isDeleted(row1, COLS)).toBe(true);
    s.markDelete(row1, COLS);
    expect(s.isDeleted(row1, COLS)).toBe(false);
    const d = s.addDraft(COLS);
    d.name = "Zoe";
    const cs = s.toChangeset();
    expect(cs.inserts).toEqual([{ name: "Zoe" }]); // empty fields omitted
  });
});
