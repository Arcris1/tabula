import { describe, it, expect } from "vitest";
import { checkStatements, type RailSettings } from "../src/lib/railChecks";

const ALL: RailSettings = { warnNoWhere: true, warnDropTruncate: true, warnProductionWrites: true };
const NONE: RailSettings = { warnNoWhere: false, warnDropTruncate: false, warnProductionWrites: false };

describe("checkStatements", () => {
  it("flags UPDATE/DELETE without WHERE", () => {
    const w = checkStatements(["DELETE FROM people"], ALL, false)!;
    expect(w.title).toBe("No WHERE clause");
    expect(w.message).toContain("DELETE FROM people");
    expect(checkStatements(["DELETE FROM people WHERE id = 1"], ALL, false)).toBeNull();
  });

  it("flags DROP and TRUNCATE on any connection", () => {
    expect(checkStatements(["DROP TABLE people"], ALL, false)!.title).toBe("DROP / TRUNCATE / RENAME");
    expect(checkStatements(["TRUNCATE people"], ALL, false)!.title).toBe("DROP / TRUNCATE / RENAME");
  });

  it("flags production writes only when production", () => {
    expect(checkStatements(["INSERT INTO t VALUES (1)"], ALL, false)).toBeNull();
    expect(checkStatements(["INSERT INTO t VALUES (1)"], ALL, true)!.title).toBe("Write on PRODUCTION");
  });

  it("combines multiple reasons into one warning", () => {
    const w = checkStatements(["DELETE FROM a", "DROP TABLE b"], ALL, true)!;
    expect(w.title).toBe("Review dangerous statements");
    expect(w.message).toContain("No WHERE clause");
    expect(w.message).toContain("DROP / TRUNCATE");
    expect(w.message).toContain("Write on PRODUCTION");
  });

  it("disabled toggles silence their guard", () => {
    expect(checkStatements(["DELETE FROM a", "DROP TABLE b"], NONE, true)).toBeNull();
    expect(checkStatements(["DROP TABLE b"], { ...ALL, warnDropTruncate: false }, false)).toBeNull();
  });

  it("SELECTs never prompt", () => {
    expect(checkStatements(["SELECT * FROM t", "SELECT 1"], ALL, true)).toBeNull();
  });

  // regression: bypasses found by db-app-explorer (B2)
  it("leading comments cannot bypass guards", () => {
    expect(checkStatements(["-- cleanup\nDELETE FROM t"], ALL, false)!.title).toBe("No WHERE clause");
    expect(checkStatements(["/* note */ DELETE FROM t"], ALL, false)!.title).toBe("No WHERE clause");
    expect(checkStatements(["-- x\nDROP TABLE t"], ALL, false)!.title).toBe("DROP / TRUNCATE / RENAME");
  });

  it("CTE-wrapped writes are classified as writes", () => {
    expect(checkStatements(["WITH doomed AS (SELECT 1) DELETE FROM t"], ALL, false)!.title).toBe("No WHERE clause");
    expect(checkStatements(["WITH s AS (SELECT 1), t2 AS (SELECT 2) INSERT INTO t SELECT * FROM s"], ALL, true)!.title)
      .toBe("Write on PRODUCTION");
    // CTE SELECT stays silent
    expect(checkStatements(["WITH x AS (SELECT 1) SELECT * FROM x"], ALL, true)).toBeNull();
  });

  it("a WHERE inside a comment does not suppress the warning", () => {
    expect(checkStatements(["DELETE FROM t /* where */"], ALL, false)!.title).toBe("No WHERE clause");
  });

  // regression: db-app-explorer round 3 B1/B5
  it("mysql # comments cannot hide a missing WHERE", () => {
    expect(checkStatements(["DELETE FROM users # WHERE id = 1"], ALL, false, "mysql")!.title)
      .toBe("No WHERE clause");
    // postgres: # is an operator, not a comment — must NOT strip
    expect(checkStatements(["DELETE FROM t WHERE a # b = 1"], ALL, false, "other")).toBeNull();
  });

  it("destructive verbs beyond the basics are flagged", () => {
    expect(checkStatements(["RENAME TABLE a TO b"], ALL, false)!.title).toContain("RENAME");
    expect(checkStatements(["CALL wipe_all()"], ALL, true)!.title).toBe("Write on PRODUCTION");
    expect(checkStatements(["LOAD DATA INFILE 'x' INTO TABLE t"], ALL, true)!.title).toBe("Write on PRODUCTION");
  });

  it("warning message shows the original statement text", () => {
    const w = checkStatements(["-- oops\nDELETE FROM t"], ALL, false)!;
    expect(w.message).toContain("-- oops");
  });
});
