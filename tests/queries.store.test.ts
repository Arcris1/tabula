import { describe, it, expect, vi, beforeEach } from "vitest";
import { setActivePinia, createPinia } from "pinia";

vi.mock("../src/lib/api", () => ({
  api: {
    runQuery: vi.fn().mockResolvedValue("ok"),
    closeSession: vi.fn().mockResolvedValue(undefined),
    cancelQuery: vi.fn().mockResolvedValue(undefined),
    addHistory: vi.fn().mockResolvedValue(undefined),
    listConnections: vi.fn().mockResolvedValue([]),
    listTables: vi.fn().mockResolvedValue([]),
    disconnect: vi.fn().mockResolvedValue(undefined),
  },
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

import { api } from "../src/lib/api";
import { useQueriesStore } from "../src/stores/queries";
import { useWorkspaceStore } from "../src/stores/workspace";

const runQuery = api.runQuery as ReturnType<typeof vi.fn>;

async function tick(times = 12) {
  for (let i = 0; i < times; i++) await Promise.resolve();
}

describe("queries store (multi-run)", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    runQuery.mockClear();
    runQuery.mockResolvedValue("ok");
    (api.addHistory as ReturnType<typeof vi.fn>).mockClear();
    (api.cancelQuery as ReturnType<typeof vi.fn>).mockClear();
    const ws = useWorkspaceStore();
    (ws as any).activeId = "conn-1";
  });

  it("adds and closes tabs, releasing the tab's session", () => {
    const q = useQueriesStore();
    const t1 = q.addTab("conn-1");
    const t2 = q.addTab("conn-1");
    expect(q.tabs).toHaveLength(2);
    expect(t2.title).toBe("Query 2");
    q.closeTab(t2.id);
    expect(q.activeTabId).toBe(t1.id);
    expect(api.closeSession).toHaveBeenCalledWith("conn-1", t2.id);
  });

  it("registers the run before invoking (fast-event race)", async () => {
    const q = useQueriesStore();
    const tab = q.addTab("conn-1");
    const batch = q.run(tab, ["SELECT 1"]);
    await tick();
    // the run exists and carries the queryId that was PASSED to the backend,
    // plus the tab id so the backend reuses this tab's session (transactions)
    expect(tab.runs).toHaveLength(1);
    expect(runQuery).toHaveBeenCalledWith("conn-1", "SELECT 1", tab.runs[0].queryId, tab.id);
    q.handleEvent("query:done", { queryId: tab.runs[0].queryId, rowCount: 0, affectedRows: 0, elapsedMs: 1 });
    await batch;
  });

  it("runs statements sequentially and keeps each result set", async () => {
    const q = useQueriesStore();
    const tab = q.addTab("conn-1");
    const batch = q.run(tab, ["SELECT 1", "SELECT 2"]);

    await tick();
    // statement 2 must NOT start before statement 1 completes
    expect(runQuery).toHaveBeenCalledTimes(1);
    const q1 = tab.runs[0].queryId;
    q.handleEvent("query:rows", { queryId: q1, rows: [[1]] });
    q.handleEvent("query:done", { queryId: q1, rowCount: 1, affectedRows: 0, elapsedMs: 3 });
    await tick();
    expect(runQuery).toHaveBeenCalledTimes(2);
    expect(tab.activeRunIndex).toBe(1);

    const q2 = tab.runs[1].queryId;
    q.handleEvent("query:rows", { queryId: q2, rows: [[2], [3]] });
    q.handleEvent("query:done", { queryId: q2, rowCount: 2, affectedRows: 0, elapsedMs: 4 });
    await batch;

    expect(tab.runs).toHaveLength(2);
    expect(tab.runs[0].rows).toEqual([[1]]);
    expect(tab.runs[1].rows).toEqual([[2], [3]]);
    expect(api.addHistory).toHaveBeenCalledTimes(2);
  });

  it("stops the batch at the first error", async () => {
    const q = useQueriesStore();
    const tab = q.addTab("conn-1");
    const batch = q.run(tab, ["SELECT nope", "SELECT 2"]);
    await tick();
    q.handleEvent("query:error", { queryId: tab.runs[0].queryId, error: { message: "boom" } });
    await batch;
    expect(tab.runs).toHaveLength(1);
    expect(tab.runs[0].status).toBe("error");
    expect(runQuery).toHaveBeenCalledTimes(1);
  });

  it("a rejected invoke marks the run as error instead of hanging", async () => {
    runQuery.mockRejectedValueOnce({ message: "not connected" });
    const q = useQueriesStore();
    const tab = q.addTab("conn-1");
    await q.run(tab, ["SELECT 1", "SELECT 2"]);
    expect(tab.runs).toHaveLength(1);
    expect(tab.runs[0].status).toBe("error");
    expect(tab.runs[0].error).toBe("not connected");
  });

  it("cancel drops the remaining queue", async () => {
    const q = useQueriesStore();
    const tab = q.addTab("conn-1");
    const batch = q.run(tab, ["SELECT SLEEP(10)", "SELECT 2"]);
    await tick();
    await q.cancel(tab);
    expect(api.cancelQuery).toHaveBeenCalledWith(tab.runs[0].queryId);
    q.handleEvent("query:error", { queryId: tab.runs[0].queryId, error: { message: "killed" } });
    await batch;
    expect(runQuery).toHaveBeenCalledTimes(1); // second statement never started
  });

  it("tracks open transactions per tab", async () => {
    const q = useQueriesStore();
    const tab = q.addTab("conn-1");
    expect(tab.txOpen).toBe(false);

    let batch = q.run(tab, ["BEGIN", "DELETE FROM t WHERE id = 1"]);
    await tick();
    q.handleEvent("query:done", { queryId: tab.runs[0].queryId, rowCount: 0, affectedRows: 0, elapsedMs: 1 });
    await tick();
    q.handleEvent("query:done", { queryId: tab.runs[1].queryId, rowCount: 0, affectedRows: 1, elapsedMs: 1 });
    await batch;
    expect(tab.txOpen).toBe(true);

    batch = q.run(tab, ["ROLLBACK"]);
    await tick();
    q.handleEvent("query:done", { queryId: tab.runs[0].queryId, rowCount: 0, affectedRows: 0, elapsedMs: 1 });
    await batch;
    expect(tab.txOpen).toBe(false);
  });

  it("a failed BEGIN does not mark the tab as in-transaction", async () => {
    const q = useQueriesStore();
    const tab = q.addTab("conn-1");
    const batch = q.run(tab, ["BEGIN"]);
    await tick();
    q.handleEvent("query:error", { queryId: tab.runs[0].queryId, error: { message: "nope" } });
    await batch;
    expect(tab.txOpen).toBe(false);
  });

  it("run while a batch is in flight is a no-op", async () => {
    const q = useQueriesStore();
    const tab = q.addTab("conn-1");
    const batch = q.run(tab, ["SELECT pg_sleep(5)"]);
    await tick();
    expect(runQuery).toHaveBeenCalledTimes(1);
    const firstRunId = tab.runs[0].queryId;

    await q.run(tab, ["SELECT 2"]); // must not start
    expect(runQuery).toHaveBeenCalledTimes(1);
    expect(tab.runs[0].queryId).toBe(firstRunId); // runs not wiped

    q.handleEvent("query:done", { queryId: firstRunId, rowCount: 0, affectedRows: 0, elapsedMs: 1 });
    await batch;
  });

  it("caps the in-memory row buffer at 10k and flags truncation", async () => {
    const q = useQueriesStore();
    const tab = q.addTab("conn-1");
    const batch = q.run(tab, ["SELECT * FROM huge"]);
    await tick();
    const qid = tab.runs[0].queryId;
    // stream 15k rows in 1k batches
    const batchRows = Array.from({ length: 1000 }, (_, i) => [i]);
    for (let b = 0; b < 15; b++) q.handleEvent("query:rows", { queryId: qid, rows: batchRows });
    q.handleEvent("query:done", { queryId: qid, rowCount: 15000, affectedRows: 0, elapsedMs: 9 });
    await batch;
    expect(tab.runs[0].rows).toHaveLength(10000);
    expect(tab.runs[0].truncated).toBe(true);
    // the true total is still reported from the server's rowCount
    expect(tab.runs[0].rowCount).toBe(15000);
  });

  it("does not flag truncation for a result at exactly the cap", async () => {
    const q = useQueriesStore();
    const tab = q.addTab("conn-1");
    const batch = q.run(tab, ["SELECT * FROM justright"]);
    await tick();
    const qid = tab.runs[0].queryId;
    const batchRows = Array.from({ length: 1000 }, (_, i) => [i]);
    for (let b = 0; b < 10; b++) q.handleEvent("query:rows", { queryId: qid, rows: batchRows });
    q.handleEvent("query:done", { queryId: qid, rowCount: 10000, affectedRows: 0, elapsedMs: 5 });
    await batch;
    expect(tab.runs[0].rows).toHaveLength(10000);
    expect(tab.runs[0].truncated).toBeFalsy();
  });

  it("ignores events for unknown query ids", async () => {
    const q = useQueriesStore();
    const tab = q.addTab("conn-1");
    const batch = q.run(tab, ["SELECT 1"]);
    await tick();
    q.handleEvent("query:rows", { queryId: "other", rows: [[9]] });
    expect(tab.runs[0].rows).toHaveLength(0);
    q.handleEvent("query:done", { queryId: tab.runs[0].queryId, rowCount: 0, affectedRows: 0, elapsedMs: 1 });
    await batch;
  });
});
