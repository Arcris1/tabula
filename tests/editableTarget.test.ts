import { describe, it, expect } from "vitest";
import { editableTarget } from "../src/lib/sqlContext";

describe("editableTarget", () => {
  it("accepts simple single-table SELECTs", () => {
    expect(editableTarget("SELECT * FROM vicidial_users")).toMatchObject({ schema: null, table: "vicidial_users" });
    expect(editableTarget("select id, name from People where id > 5 order by id limit 10"))
      .toMatchObject({ table: "People" });
    expect(editableTarget("SELECT * FROM app.orders")).toMatchObject({ schema: "app", table: "orders" });
    expect(editableTarget('SELECT * FROM "MySchema"."MyTable"'))
      .toMatchObject({ schema: "MySchema", table: "MyTable", schemaQuoted: true, tableQuoted: true });
    expect(editableTarget("SELECT * FROM t AS alias WHERE alias.x = 1")).toMatchObject({ table: "t" });
  });

  it("rejects everything unsafe", () => {
    expect(editableTarget("SELECT * FROM a JOIN b ON a.id = b.id")).toBeNull();
    expect(editableTarget("SELECT * FROM a, b")).toBeNull();
    expect(editableTarget("SELECT x, count(*) FROM t")).toBeNull();
    expect(editableTarget("SELECT DISTINCT x FROM t")).toBeNull();
    expect(editableTarget("SELECT x FROM t GROUP BY x")).toBeNull();
    expect(editableTarget("SELECT x FROM t UNION SELECT y FROM u")).toBeNull();
    expect(editableTarget("SELECT * FROM (SELECT 1) sub")).toBeNull();
    expect(editableTarget("SELECT *, (SELECT max(id) FROM u) FROM t")).toBeNull();
    expect(editableTarget("UPDATE t SET x = 1")).toBeNull();
    expect(editableTarget("WITH c AS (SELECT 1) SELECT * FROM c")).toBeNull();
  });

  it("keywords inside strings do not block", () => {
    expect(editableTarget("SELECT * FROM t WHERE note = 'join the group by union'"))
      .toMatchObject({ table: "t" });
  });

  it("mysql # comments are stripped before analysis", () => {
    expect(editableTarget("SELECT * FROM t # JOIN comment", { hashComments: true }))
      .toMatchObject({ table: "t" });
    expect(editableTarget("SELECT * FROM t JOIN u", { hashComments: true })).toBeNull();
  });
});
