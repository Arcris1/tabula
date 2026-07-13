import { describe, it, expect } from "vitest";
import { tablesInStatement } from "../src/lib/sqlContext";

describe("tablesInStatement", () => {
  it("finds FROM and JOIN tables", () => {
    expect(tablesInStatement("SELECT * FROM vicidial_users u JOIN vicidial_lists l ON l.id = u.id"))
      .toEqual(["vicidial_users", "vicidial_lists"]);
  });
  it("handles UPDATE and INSERT INTO", () => {
    expect(tablesInStatement("UPDATE people SET age = 1")).toEqual(["people"]);
    expect(tablesInStatement("INSERT INTO people (id) VALUES (1)")).toEqual(["people"]);
  });
  it("strips quoting and schema prefixes", () => {
    expect(tablesInStatement('SELECT * FROM "public"."Users"')).toEqual(["users"]);
    expect(tablesInStatement("SELECT * FROM `my_db`.`orders`")).toEqual(["orders"]);
  });
  it("STRAIGHT_JOIN tables are found", () => {
    expect(tablesInStatement("SELECT * FROM a STRAIGHT_JOIN b ON a.id = b.id"))
      .toEqual(["a", "b"]);
  });
  it("dedupes and returns empty for no tables", () => {
    expect(tablesInStatement("SELECT * FROM t JOIN t ON 1=1")).toEqual(["t"]);
    expect(tablesInStatement("SELECT 1")).toEqual([]);
  });
});
