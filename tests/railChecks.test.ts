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

describe("hardened guards (persona-panel findings)", () => {
  const rails = { warnNoWhere: true, warnDropTruncate: true, warnProductionWrites: true };

  it("'where' inside a string literal no longer silences the warning", () => {
    const w = checkStatements(["UPDATE leads SET note = 'call where applicable'"], rails, false);
    expect(w?.title).toContain("No WHERE");
  });

  it("a WHERE only inside a subquery no longer silences the warning", () => {
    const w = checkStatements(["UPDATE t SET x = (SELECT max(y) FROM u WHERE z = 1)"], rails, false);
    expect(w?.title).toContain("No WHERE");
  });

  it("a real top-level WHERE passes", () => {
    const w = checkStatements(["UPDATE t SET x = 1 WHERE id = 5"], rails, false);
    expect(w).toBeNull();
  });

  it("EXEC on a production connection is flagged as a write", () => {
    const w = checkStatements(["EXEC dbo.purge_all"], rails, true);
    expect(w?.title).toContain("PRODUCTION");
    expect(checkStatements(["EXEC dbo.purge_all"], rails, false)).toBeNull();
  });
});

describe("batch classification (persona S1 / #4 bypasses)", () => {
  const rails = { warnNoWhere: true, warnDropTruncate: true, warnProductionWrites: true };

  it("a MSSQL batch led by SET NOCOUNT no longer hides a wildcard DELETE", () => {
    const w = checkStatements(["SET NOCOUNT ON; DELETE FROM dbo.customers;"], rails, false);
    expect(w?.message).toContain("No WHERE");
  });

  it("a batch led by SELECT no longer hides a DROP TABLE", () => {
    const w = checkStatements(["SELECT 1; DROP TABLE dbo.customers;"], rails, false);
    expect(w?.message).toContain("DROP");
  });

  it("ALTER TABLE ... DROP COLUMN is flagged as destructive", () => {
    const w = checkStatements(["ALTER TABLE users DROP COLUMN email"], rails, false);
    expect(w?.title).toContain("DROP");
  });

  it("every write in a multi-statement batch is flagged on production", () => {
    const w = checkStatements(["INSERT INTO a VALUES (1); UPDATE b SET x = 1 WHERE id = 2"], rails, true);
    expect(w?.message).toContain("INSERT");
    expect(w?.message).toContain("UPDATE");
  });

  it("a CREATE PROCEDURE body does not false-positive on its inner statements", () => {
    // routine def stays one unit: the inner DELETE (no WHERE) must NOT warn on non-prod
    const proc = "CREATE PROCEDURE dbo.purge AS BEGIN DELETE FROM staging; END";
    expect(checkStatements([proc], rails, false)).toBeNull();
    // ...but on production it's still a write
    expect(checkStatements([proc], rails, true)?.title).toContain("PRODUCTION");
  });

  it("semicolons inside strings/parens do not create phantom statements", () => {
    const w = checkStatements(["UPDATE t SET note = 'a; b; c' WHERE id = 1"], rails, false);
    expect(w).toBeNull(); // real top-level WHERE, one statement
  });
});

describe("severe flag drives the typed-confirm escalation (Fix 3)", () => {
  const rails = { warnNoWhere: true, warnDropTruncate: true, warnProductionWrites: true };
  it("DROP/TRUNCATE is severe", () => {
    expect(checkStatements(["DROP TABLE x"], rails, false)?.severe).toBe(true);
    expect(checkStatements(["TRUNCATE TABLE x"], rails, false)?.severe).toBe(true);
  });
  it("a production write is severe", () => {
    expect(checkStatements(["INSERT INTO x VALUES (1)"], rails, true)?.severe).toBe(true);
  });
  it("a plain no-WHERE on a non-prod box is NOT severe (one-click)", () => {
    expect(checkStatements(["UPDATE x SET y = 1"], rails, false)?.severe).toBe(false);
  });
});
