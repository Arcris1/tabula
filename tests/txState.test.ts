import { describe, it, expect } from "vitest";
import { txEffect } from "../src/lib/txState";

describe("txEffect", () => {
  it("opens on BEGIN and START TRANSACTION", () => {
    expect(txEffect("BEGIN")).toBe("begin");
    expect(txEffect("  begin;")).toBe("begin");
    expect(txEffect("START TRANSACTION")).toBe("begin");
  });
  it("closes on COMMIT, END, ABORT and ROLLBACK", () => {
    expect(txEffect("COMMIT")).toBe("end");
    expect(txEffect("END")).toBe("end"); // postgres COMMIT synonym
    expect(txEffect("ABORT")).toBe("end");
    expect(txEffect("rollback")).toBe("end");
  });
  it("ROLLBACK TO SAVEPOINT keeps the transaction open", () => {
    expect(txEffect("ROLLBACK TO SAVEPOINT sp1")).toBeNull();
    expect(txEffect("ROLLBACK TO sp1")).toBeNull();
  });
  it("AND CHAIN keeps a transaction open", () => {
    expect(txEffect("COMMIT AND CHAIN")).toBeNull();
    expect(txEffect("ROLLBACK AND CHAIN")).toBeNull();
  });
  it("comments cannot hide transaction control", () => {
    expect(txEffect("/* deploy */ BEGIN")).toBe("begin");
    expect(txEffect("-- audit\nCOMMIT")).toBe("end");
  });
  it("mysql # comments cannot hide transaction control", () => {
    expect(txEffect("# comment\nCOMMIT", "mysql")).toBe("end");
    expect(txEffect("# comment\nBEGIN", "mysql")).toBe("begin");
  });
  it("mysql DDL implicitly commits", () => {
    expect(txEffect("CREATE TABLE t (a int)", "mysql")).toBe("end");
    expect(txEffect("ALTER TABLE t ADD b int", "mysql")).toBe("end");
    expect(txEffect("CREATE TABLE t (a int)", "other")).toBeNull();
  });
  it("ordinary statements have no effect", () => {
    expect(txEffect("SELECT 1")).toBeNull();
    expect(txEffect("DELETE FROM t WHERE id = 1")).toBeNull();
    expect(txEffect("START something_else")).toBeNull();
  });
});

describe("T-SQL BEGIN blocks vs transactions", () => {
  it("BEGIN...END blocks do not open a transaction", () => {
    expect(txEffect("BEGIN PRINT 'hi'; END")).toBeNull();
    expect(txEffect("BEGIN TRY SELECT 1 END TRY BEGIN CATCH END CATCH")).toBeNull();
    expect(txEffect("BEGIN\n  SELECT 1;\nEND")).toBeNull();
  });
  it("real transaction openers still count", () => {
    expect(txEffect("BEGIN")).toBe("begin");
    expect(txEffect("begin;")).toBe("begin");
    expect(txEffect("BEGIN TRAN")).toBe("begin");
    expect(txEffect("BEGIN TRANSACTION my_tx")).toBe("begin");
    expect(txEffect("BEGIN WORK")).toBe("begin");
    expect(txEffect("BEGIN ISOLATION LEVEL SERIALIZABLE")).toBe("begin");
  });
});

describe("T-SQL savepoint rollback keeps the transaction open (Sofia Bug 3)", () => {
  it("ROLLBACK TRAN <savepoint> does NOT end the transaction", () => {
    expect(txEffect("ROLLBACK TRAN sp1", "other")).toBeNull();
    expect(txEffect("ROLLBACK TRANSACTION my_savepoint", "other")).toBeNull();
    expect(txEffect("ROLLBACK TRANSACTION my_savepoint;", "other")).toBeNull();
  });
  it("bare ROLLBACK / ROLLBACK TRAN still ends it", () => {
    expect(txEffect("ROLLBACK", "other")).toBe("end");
    expect(txEffect("ROLLBACK TRAN", "other")).toBe("end");
    expect(txEffect("ROLLBACK TRANSACTION", "other")).toBe("end");
    expect(txEffect("ROLLBACK WORK", "other")).toBe("end");
  });
  it("ROLLBACK TO SAVEPOINT and AND CHAIN still keep it open", () => {
    expect(txEffect("ROLLBACK TO SAVEPOINT x", "other")).toBeNull();
    expect(txEffect("ROLLBACK AND CHAIN", "other")).toBeNull();
  });
});
