import { describe, it, expect } from "vitest";
import { rowsToTsv, rowsToInsert, rowsToCsv, rowsToJson } from "../src/lib/copyRows";

const cols = ["id", "name", "meta"];
const rows: unknown[][] = [
  [1, "Alice", { a: 1 }],
  [2, "O'Brien", null],
];

describe("copyRows", () => {
  it("tsv has header + tab-separated rows", () => {
    const t = rowsToTsv(cols, rows).split("\n");
    expect(t[0]).toBe("id\tname\tmeta");
    expect(t[1]).toBe('1\tAlice\t{"a":1}');
    expect(t[2]).toBe("2\tO'Brien\t");
  });

  it("insert escapes quotes and renders NULL", () => {
    const sql = rowsToInsert("people", cols, rows);
    expect(sql).toContain(`INSERT INTO "people" ("id", "name", "meta") VALUES (1, 'Alice', '{"a":1}');`);
    expect(sql).toContain(`(2, 'O''Brien', NULL);`);
  });

  it("csv quotes fields with commas/quotes", () => {
    const csv = rowsToCsv(["a"], [["x,y"], ['he said "hi"']]).split("\n");
    expect(csv[1]).toBe('"x,y"');
    expect(csv[2]).toBe('"he said ""hi"""');
  });

  it("json is array of objects", () => {
    expect(JSON.parse(rowsToJson(cols, rows))).toEqual([
      { id: 1, name: "Alice", meta: { a: 1 } },
      { id: 2, name: "O'Brien", meta: null },
    ]);
  });
});
