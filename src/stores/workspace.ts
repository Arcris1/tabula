import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { api, type TableInfo } from "../lib/api";

/** Per-connection workspace state, so several connections can be open at once
 *  and each keeps its own tables, selection and active sub-view. */
interface ConnState {
  id: string;
  paradigm: "sql" | "kv";
  tables: TableInfo[];
  functions: string[];
  selectedTable: TableInfo | null;
  openTables: TableInfo[];
  pendingFilter: { column: string; value: string } | null;
  /** active sub-view for this connection: "data" | "structure" | <queryTabId> */
  mainTab: string;
  /** liveness shown in the tab bar so a blip doesn't leave a dead connection
   *  looking alive: "live" | "reconnecting" | "dead". */
  health: "live" | "reconnecting" | "dead";
}

export const useWorkspaceStore = defineStore("workspace", () => {
  const conns = ref<ConnState[]>([]);
  const activeId = ref<string | null>(null);

  const activeConn = computed(() => conns.value.find((c) => c.id === activeId.value) ?? null);
  const isOpen = computed(() => activeId.value !== null);

  // ---- proxies to the active connection (keep the old single-conn API) ----
  const paradigm = computed(() => activeConn.value?.paradigm ?? null);
  const tables = computed(() => activeConn.value?.tables ?? []);
  const functions = computed(() => activeConn.value?.functions ?? []);
  const selectedTable = computed(() => activeConn.value?.selectedTable ?? null);
  const openTables = computed(() => activeConn.value?.openTables ?? []);
  const pendingFilter = computed({
    get: () => activeConn.value?.pendingFilter ?? null,
    set: (v) => { if (activeConn.value) activeConn.value.pendingFilter = v; },
  });
  const mainTab = computed({
    get: () => activeConn.value?.mainTab ?? "data",
    set: (v) => { if (activeConn.value) activeConn.value.mainTab = v; },
  });

  function tableKey(t: TableInfo) {
    return `${t.schema ?? ""}.${t.name}`;
  }
  function get(id: string) {
    return conns.value.find((c) => c.id === id);
  }

  /** Add a connection (or focus it if already open) and make it active. */
  async function addConnection(id: string, p: "sql" | "kv") {
    if (!get(id)) {
      conns.value.push({
        id, paradigm: p, tables: [], functions: [],
        selectedTable: null, openTables: [], pendingFilter: null, mainTab: "data",
        health: "live",
      });
      activeId.value = id;
      if (p === "sql") { await loadTables(); loadFunctions(); }
    } else {
      activeId.value = id; // already open — just switch to it, state preserved
    }
  }
  function setActive(id: string) {
    if (get(id)) activeId.value = id;
  }

  /** Open a connection freshly (backend already connected, possibly to a
   *  different database) — reset any existing state and reload its schema. */
  async function openFresh(id: string, p: "sql" | "kv") {
    const existing = get(id);
    if (existing) {
      existing.paradigm = p;
      existing.tables = [];
      existing.functions = [];
      existing.selectedTable = null;
      existing.openTables = [];
      existing.pendingFilter = null;
      existing.mainTab = "data";
      activeId.value = id;
      if (p === "sql") { await loadTables(); loadFunctions(); }
    } else {
      await addConnection(id, p);
    }
  }

  async function loadTables() {
    const c = activeConn.value;
    if (c) c.tables = await api.listTables(c.id);
  }
  async function loadFunctions() {
    const c = activeConn.value;
    if (!c) return;
    try {
      c.functions = await api.listFunctions(c.id);
    } catch {
      c.functions = [];
    }
  }
  function selectTable(t: TableInfo) {
    const c = activeConn.value;
    if (!c) return;
    c.selectedTable = t;
    if (!c.openTables.some((o) => tableKey(o) === tableKey(t))) c.openTables.push(t);
  }
  /** Follow a foreign key: open `refTableName` filtered to `column = value`. */
  function navigateToRow(refTableName: string, column: string, value: string) {
    const c = activeConn.value;
    if (!c) return;
    const t = c.tables.find((x) => x.name === refTableName);
    if (!t) return;
    c.pendingFilter = { column, value };
    selectTable(t);
  }
  function closeTable(t: TableInfo) {
    const c = activeConn.value;
    if (!c) return;
    const i = c.openTables.findIndex((o) => tableKey(o) === tableKey(t));
    if (i < 0) return;
    c.openTables.splice(i, 1);
    if (c.selectedTable && tableKey(c.selectedTable) === tableKey(t)) {
      c.selectedTable = c.openTables[Math.max(0, i - 1)] ?? null;
    }
  }

  /** Rebuild a dead connection (network came back) without losing its
   *  workspace state. Schema is unchanged, so we only re-fetch it for the
   *  active connection to confirm connectivity. */
  async function reconnect(id: string) {
    const c = get(id);
    if (!c) return;
    c.health = "reconnecting";
    try {
      const p = await api.reconnect(id); // throws if still unreachable
      c.paradigm = p;
      c.health = "live";
      if (activeId.value === id && p === "sql") {
        await loadTables();
        loadFunctions();
      }
    } catch (e) {
      c.health = "dead"; // leave it visibly dead in the tab bar
      throw e;
    }
  }
  /** Reconnect every open connection. Best-effort (one failure doesn't block the
   *  rest) but returns the failures so the caller can surface them — a silent
   *  bulk reconnect used to leave dead connections looking alive. */
  async function reconnectAll(): Promise<{ id: string; error: string }[]> {
    const ids = conns.value.map((c) => c.id);
    const results = await Promise.allSettled(ids.map((id) => reconnect(id)));
    const failures: { id: string; error: string }[] = [];
    results.forEach((r, i) => {
      if (r.status === "rejected") failures.push({ id: ids[i], error: String((r.reason as any)?.message ?? r.reason) });
    });
    return failures;
  }
  /** Mark a connection dead — e.g. when a query fails because the link dropped. */
  function markDead(id: string) {
    const c = get(id);
    if (c) c.health = "dead";
  }

  /** Disconnect and remove ONE connection; switch active to a neighbour. */
  async function closeConnection(id: string) {
    const idx = conns.value.findIndex((c) => c.id === id);
    if (idx < 0) return;
    conns.value.splice(idx, 1);
    if (activeId.value === id) {
      activeId.value = conns.value[Math.max(0, idx - 1)]?.id ?? null;
    }
    try {
      await api.disconnect(id);
    } catch {
      /* best-effort */
    }
  }

  /** Disconnect and remove ALL connections (window teardown). */
  async function close() {
    const ids = conns.value.map((c) => c.id);
    conns.value = [];
    activeId.value = null;
    for (const id of ids) {
      try {
        await api.disconnect(id);
      } catch {
        /* best-effort */
      }
    }
  }

  return {
    conns, activeId, isOpen,
    paradigm, tables, functions, selectedTable, openTables, pendingFilter, mainTab,
    tableKey, get, addConnection, setActive, openFresh, loadTables, loadFunctions,
    selectTable, navigateToRow, closeTable, reconnect, reconnectAll, markDead, closeConnection, close,
  };
});
