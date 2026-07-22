import { describe, it, expect } from "vitest";
import { statements, statementAt } from "../src/lib/sqlStatement";

describe("statements", () => {
  it("splits on semicolons", () => {
    const s = statements("SELECT 1; SELECT 2;");
    expect(s.map((x) => x.text)).toEqual(["SELECT 1", "SELECT 2"]);
  });
  it("ignores semicolons in strings and comments", () => {
    const text = "SELECT 'a;b'; -- c;d\nSELECT /* e;f */ 2";
    const s = statements(text);
    expect(s).toHaveLength(2);
    expect(s[0].text).toBe("SELECT 'a;b'");
    expect(s[1].text).toContain("SELECT /* e;f */ 2");
  });
  it("statementAt picks the statement under the cursor", () => {
    const text = "SELECT 1;\nSELECT 2;";
    expect(statementAt(text, 2)!.text).toBe("SELECT 1");
    expect(statementAt(text, 12)!.text).toBe("SELECT 2");
    expect(statementAt(text, text.length)!.text).toBe("SELECT 2"); // after last
  });
  it("returns null for empty text", () => {
    expect(statementAt("", 0)).toBeNull();
  });

  // regression: db-app-explorer B3
  it("does not split inside postgres dollar-quoted bodies", () => {
    const fn = "CREATE FUNCTION f() RETURNS void AS $$ BEGIN DELETE FROM t; END $$ LANGUAGE plpgsql";
    const s = statements(fn + "; SELECT 1");
    expect(s).toHaveLength(2);
    expect(s[0].text).toBe(fn);
    expect(s[1].text).toBe("SELECT 1");
  });

  it("handles tagged dollar quotes", () => {
    const s = statements("SELECT $body$ a; b $body$; SELECT 2");
    expect(s).toHaveLength(2);
  });

  it("mysql dialect: backslash-escaped quotes do not terminate strings", () => {
    const s = statements("SELECT 'it\\'s; fine'; SELECT 2", { backslashEscapes: true });
    expect(s).toHaveLength(2);
    expect(s[0].text).toBe("SELECT 'it\\'s; fine'");
  });

  // regression: db-app-explorer round 3 B3
  it("mysql dialect: semicolon inside a # comment does not split", () => {
    const withHash = statements("SELECT 1 # note; not a boundary\n+ 1; SELECT 2", { hashComments: true });
    expect(withHash).toHaveLength(2);
    expect(withHash[0].text).toBe("SELECT 1 # note; not a boundary\n+ 1");
    expect(withHash[1].text).toBe("SELECT 2");
    // without the mysql dialect flag, # is not a comment and the ; splits
    expect(statements("SELECT 1 # note; SELECT 2")).toHaveLength(2);
  });

  // regression: db-app-explorer round 2 B7
  it("standard dialect: trailing backslash in a string does not merge statements", () => {
    const s = statements("SELECT 'C:\\' AS p;\nSELECT 1;");
    expect(s).toHaveLength(2);
    expect(s[0].text).toBe("SELECT 'C:\\' AS p");
    expect(s[1].text).toBe("SELECT 1");
  });
});

describe("T-SQL GO batches (mssql)", () => {
  const tsql = { goSeparator: true, semicolons: false };

  it("splits on GO lines and never sends GO itself", () => {
    const s = statements("CREATE DATABASE d;\nGO\nUSE d;\nGO\nSELECT 1;", tsql);
    expect(s.map((x) => x.text)).toEqual(["CREATE DATABASE d;", "USE d;", "SELECT 1;"]);
  });

  it("keeps semicolons inside a batch (proc bodies ship whole)", () => {
    const proc = "CREATE PROCEDURE p AS\nBEGIN\n  SELECT 1;\n  SELECT 2;\nEND\nGO\nEXEC p;";
    const s = statements(proc, tsql);
    expect(s).toHaveLength(2);
    expect(s[0].text).toContain("SELECT 1;");
    expect(s[1].text).toBe("EXEC p;");
  });

  it("is case-insensitive, allows GO n, and ignores mid-line go", () => {
    const s = statements("SELECT 'go' AS x\ngo 5\nSELECT let_go FROM t", tsql);
    expect(s.map((x) => x.text)).toEqual(["SELECT 'go' AS x", "SELECT let_go FROM t"]);
  });

  it("GO at end of script and without trailing newline", () => {
    expect(statements("SELECT 1\nGO", tsql).map((x) => x.text)).toEqual(["SELECT 1"]);
  });
});

describe("regression: user's exact proc script", () => {
  it("splits into 4 batches with CREATE PROCEDURE first in its batch", () => {
    const doc = `USE ProcLecturesDB;
GO
SELECT * FROM dbo.Customers;
GO
CREATE PROCEDURE dbo.GetActiveCustomers
AS
BEGIN
    SET NOCOUNT ON;
    SELECT CustomerID, FirstName, LastName, Email, City
    FROM Customers
    WHERE IsActive = 1;
END;
GO
EXEC GetActiveCustomers;`;
    const s = statements(doc, { goSeparator: true, semicolons: false });
    expect(s.map((x) => x.text.split("\n")[0])).toEqual([
      "USE ProcLecturesDB;",
      "SELECT * FROM dbo.Customers;",
      "CREATE PROCEDURE dbo.GetActiveCustomers",
      "EXEC GetActiveCustomers;",
    ]);
  });
});
