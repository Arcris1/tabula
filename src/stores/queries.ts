import { defineStore } from "pinia";
import { ref } from "vue";
import { listen } from "@tauri-apps/api/event";
import { api, type ColumnMeta } from "../lib/api";
import { txEffect, type TxDialect } from "../lib/txState";
import { useConnectionsStore } from "./connections";
import { useWorkspaceStore } from "./workspace";

export interface QueryRun {
  queryId: string;
  columns: ColumnMeta[];
  rows: unknown[][];
  /** true once the in-memory row buffer hit ROW_CAP and further rows were dropped from the grid */
  truncated?: boolean;
  /** server info messages (T-SQL PRINT etc.) — SSMS's "Messages" */
  messages: string[];
  status: "running" | "done" | "error";
  rowCount?: number;
  affectedRows?: number;
  elapsedMs?: number;
  error?: string;
  sql: string;
  startedAtMs: number;
}

export interface QueryTab {
  id: string;
  /** the connection this tab belongs to (tabs are per-connection) */
  connectionId: string;
  title: string;
  sql: string;
  runs: QueryRun[];
  activeRunIndex: number;
  /** this tab's session has an open transaction (heuristic: BEGIN seen, no COMMIT/ROLLBACK yet) */
  txOpen: boolean;
}

let tabCounter = 0;

export const useQueriesStore = defineStore("queries", () => {
  const tabs = ref<QueryTab[]>([]);
  const activeTabId = ref<string | null>(null);
  let listenersReady = false;
  /** queryId -> resolve fn, settled when that query reaches done/error */
  const completions = new Map<string, () => void>();
  /** tab ids whose in-flight batch should stop after the current statement */
  const cancelled = new Set<string>();

  function addTab(connectionId: string): QueryTab {
    tabCounter += 1;
    const tab: QueryTab = {
      id: crypto.randomUUID(), connectionId, title: `Query ${tabCounter}`, sql: "",
      runs: [], activeRunIndex: 0, txOpen: false,
    };
    tabs.value.push(tab);
    activeTabId.value = tab.id;
    return tabs.value[tabs.value.length - 1];
  }

  function closeTab(id: string) {
    const i = tabs.value.findIndex((t) => t.id === id);
    const tab = tabs.value[i];
    if (i >= 0) tabs.value.splice(i, 1);
    if (activeTabId.value === id) activeTabId.value = tabs.value[Math.max(0, i - 1)]?.id ?? null;
    if (tab) api.closeSession(tab.connectionId, id).catch(() => {}); // releases the tab's connection
  }

  /** Close every query tab belonging to a connection (when it's disconnected). */
  function closeConnectionTabs(connectionId: string) {
    for (const t of tabs.value.filter((t) => t.connectionId === connectionId)) {
      api.closeSession(connectionId, t.id).catch(() => {});
    }
    tabs.value = tabs.value.filter((t) => t.connectionId !== connectionId);
  }

  function setActive(id: string) {
    activeTabId.value = id;
  }

  function activeRun(tab: QueryTab): QueryRun | null {
    return tab.runs[tab.activeRunIndex] ?? null;
  }

  /** Executes statements sequentially; stops at the first error or on cancel. */
  async function run(tab: QueryTab, stmts: string[]) {
    const ws = useWorkspaceStore();
    if (!ws.activeId || !stmts.length) return;
    // a second run while a batch is in flight would orphan the running query
    // and leak a duplicate backend session for this tab
    if (tab.runs.some((r) => r.status === "running")) return;
    const engine = useConnectionsStore().connections.find((c) => c.id === ws.activeId)?.engine;
    const dialect: TxDialect = engine === "mysql" ? "mysql" : "other";
    await initListeners();
    cancelled.delete(tab.id);
    tab.runs = [];
    tab.activeRunIndex = 0;
    for (let i = 0; i < stmts.length; i++) {
      if (cancelled.has(tab.id)) break;
      // register the run BEFORE invoking so a fast query's events can never
      // arrive unmatched (the invoke round-trip may resolve after query:done)
      const queryId = crypto.randomUUID();
      tab.runs.push({
        queryId, columns: [], rows: [], messages: [], status: "running",
        sql: stmts[i], startedAtMs: Date.now(),
      });
      tab.activeRunIndex = i;
      const done = new Promise<void>((resolve) => completions.set(queryId, resolve));
      api.runQuery(ws.activeId, stmts[i], queryId, tab.id).catch((e: any) => {
        const run = findRun(queryId);
        if (run && run.status === "running") {
          run.status = "error";
          run.error = e?.message ?? String(e);
          record(run, "error");
        }
        settle(queryId);
      });
      await done;
      if (tab.runs[i]?.status === "done") {
        const effect = txEffect(stmts[i], dialect);
        if (effect === "begin") tab.txOpen = true;
        else if (effect === "end") tab.txOpen = false;
      }
      if (tab.runs[i]?.status === "error") break;
    }
    cancelled.delete(tab.id);
  }

  async function cancel(tab: QueryTab) {
    cancelled.add(tab.id);
    const current = tab.runs.find((r) => r.status === "running");
    if (current) await api.cancelQuery(current.queryId);
  }

  /** Max rows the editor grid holds in memory. The server still streams the full
   *  result (rowCount/affected reflect the true total); we just stop growing the
   *  in-memory array so a `SELECT *` on a huge table can't wedge the UI. */
  const ROW_CAP = 10_000;

  function appendRows(run: QueryRun, incoming: unknown[][]) {
    if (run.truncated) return; // already at the cap — drop silently, banner is shown
    const room = ROW_CAP - run.rows.length;
    if (incoming.length <= room) {
      // push in place (avoids reallocating the whole array on every batch)
      for (const r of incoming) run.rows.push(r as unknown[]);
    } else {
      for (let i = 0; i < room; i++) run.rows.push(incoming[i] as unknown[]);
      run.truncated = true;
    }
  }

  function findRun(queryId: string): QueryRun | null {
    for (const t of tabs.value) {
      const r = t.runs.find((r) => r.queryId === queryId);
      if (r) return r;
    }
    return null;
  }

  function record(run: QueryRun, status: "ok" | "error") {
    const ws = useWorkspaceStore();
    if (!ws.activeId) return;
    api
      .addHistory({
        connectionId: ws.activeId,
        sql: run.sql,
        startedAtMs: run.startedAtMs,
        durationMs: run.elapsedMs ?? Date.now() - run.startedAtMs,
        status,
        rowCount: run.rowCount ?? null,
        error: run.error ?? null,
      })
      .catch(() => {});
  }

  function settle(queryId: string) {
    completions.get(queryId)?.();
    completions.delete(queryId);
  }

  function handleEvent(name: string, payload: any) {
    const run = findRun(payload.queryId);
    if (!run) return;
    if (name === "query:columns") run.columns = payload.columns;
    else if (name === "query:rows") appendRows(run, payload.rows);
    else if (name === "query:message") run.messages.push(payload.message);
    else if (name === "query:done") {
      run.status = "done";
      run.rowCount = payload.rowCount;
      run.affectedRows = payload.affectedRows;
      run.elapsedMs = payload.elapsedMs;
      record(run, "ok");
      settle(payload.queryId);
    } else if (name === "query:error") {
      run.status = "error";
      run.error = payload.error?.message ?? "query failed";
      record(run, "error");
      settle(payload.queryId);
    }
  }

  async function initListeners() {
    if (listenersReady) return;
    listenersReady = true;
    for (const name of ["query:columns", "query:rows", "query:message", "query:done", "query:error"] as const) {
      await listen(name, (e) => handleEvent(name, e.payload as any));
    }
  }

  function reset() {
    tabs.value = [];
    activeTabId.value = null;
    tabCounter = 0;
    completions.clear();
    cancelled.clear();
  }

  return { tabs, activeTabId, addTab, closeTab, closeConnectionTabs, setActive, activeRun, run, cancel, handleEvent, initListeners, reset };
});
