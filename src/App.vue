<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import ConfirmModal from "./components/ConfirmModal.vue";
import ConnectionLauncher from "./components/ConnectionLauncher.vue";
import DataGrid from "./components/DataGrid.vue";
import HistoryPanel from "./components/HistoryPanel.vue";
import SavedQueriesPanel from "./components/SavedQueriesPanel.vue";
import KeyBrowser from "./components/KeyBrowser.vue";
import QueryEditor from "./components/QueryEditor.vue";
import RedisValuePane from "./components/RedisValuePane.vue";
import SchemaSidebar from "./components/SchemaSidebar.vue";
import SettingsModal from "./components/SettingsModal.vue";
import ShortcutHelpModal from "./components/ShortcutHelpModal.vue";
import StructureView from "./components/StructureView.vue";
import { api, type RedisEntry } from "./lib/api";
import { bindings, comboOf, detectPlatform } from "./lib/keymap";
import OverrideSavedModal from "./components/OverrideSavedModal.vue";
import ResizeHandle from "./components/ResizeHandle.vue";
import UnsavedCloseModal from "./components/UnsavedCloseModal.vue";
import { useConnectionsStore } from "./stores/connections";
import { useQueriesStore } from "./stores/queries";
import { usePanelsStore } from "./stores/panels";
import { useSavedQueriesStore } from "./stores/savedQueries";
import { useShortcutsStore } from "./stores/shortcuts";
import { useWorkspaceStore } from "./stores/workspace";

const ws = useWorkspaceStore();
const conns = useConnectionsStore();
const queries = useQueriesStore();
const savedQueries = useSavedQueriesStore();
const panels = usePanelsStore();
const active = computed(() => conns.connections.find((c) => c.id === ws.activeId));
const colorClass: Record<string, string> = {
  gray: "bg-zinc-500", blue: "bg-blue-500", green: "bg-green-500",
  amber: "bg-amber-500", red: "bg-red-500",
};

const redisEntry = ref<RedisEntry | null>(null);
async function loadKey(key: string) {
  if (ws.activeId) redisEntry.value = await api.getRedisValue(ws.activeId, key);
}

// the active sub-view lives per-connection in the workspace store
const mainTab = computed<string>({ get: () => ws.mainTab, set: (v) => { ws.mainTab = v; } });
const showLauncher = ref(false); // opening another connection while others are open
const showHistory = ref(false);
const showSaved = ref(false);
const editorRefs = ref<Record<string, InstanceType<typeof QueryEditor> | null>>({});
const structureRef = ref<InstanceType<typeof StructureView> | null>(null);
// query tabs are per-connection; only the active connection's are shown
const activeQueryTabs = computed(() => queries.tabs.filter((t) => t.connectionId === ws.activeId));
function connMeta(id: string) {
  return conns.connections.find((c) => c.id === id);
}
function onConnected(id: string, p: "sql" | "kv") {
  showLauncher.value = false;
  ws.addConnection(id, p);
}

function newTableForm() {
  mainTab.value = "structure";
  setTimeout(() => structureRef.value?.startCreate(), 50);
}
function selectOpenTable(t: any) {
  ws.selectTable(t);
  mainTab.value = "data";
}

// picking a table in the sidebar jumps to its data (unless the user is
// browsing structures, where staying in Structure is the useful behavior)
let prevActiveId = ws.activeId;
watch(() => ws.selectedTable, (t) => {
  const switched = ws.activeId !== prevActiveId;
  prevActiveId = ws.activeId;
  // on a connection switch the selection changes too — don't override the
  // connection's own restored sub-view
  if (switched) return;
  if (t && mainTab.value !== "data" && mainTab.value !== "structure") mainTab.value = "data";
});

function newQueryTab() {
  if (!ws.activeId) return;
  const t = queries.addTab(ws.activeId);
  mainTab.value = t.id;
}

// focus the editor when a query tab becomes active so you can type immediately
watch(mainTab, (id) => {
  if (id !== "data" && id !== "structure") {
    nextTick(() => editorRefs.value[id as string]?.focus());
  }
});
const pendingCloseTab = ref<string | null>(null);
const unsavedCloseTab = ref<{ id: string; title: string } | null>(null);
function closeQueryTab(id: string) {
  const tab = queries.tabs.find((t) => t.id === id);
  if (tab?.txOpen) {
    pendingCloseTab.value = id; // closing rolls the open transaction back — confirm first
    return;
  }
  // unsaved query with real content: offer to save before closing
  if (tab && tab.sql.trim() && !savedQueries.isSaved(tab.sql)) {
    unsavedCloseTab.value = { id, title: tab.title };
    return;
  }
  reallyCloseTab(id);
}
function reallyCloseTab(id: string) {
  pendingCloseTab.value = null;
  unsavedCloseTab.value = null;
  queries.closeTab(id);
  if (mainTab.value === id) mainTab.value = activeQueryTabs.value[0]?.id ?? "data";
}
function saveAndCloseTab(name: string) {
  const pending = unsavedCloseTab.value;
  if (!pending) return;
  const tab = queries.tabs.find((t) => t.id === pending.id);
  if (tab) savedQueries.save(name, tab.sql.trim());
  reallyCloseTab(pending.id);
}
function pickHistory(sql: string) {
  if (mainTab.value === "data" || mainTab.value === "structure") newQueryTab();
  const tab = queries.tabs.find((t) => t.id === mainTab.value);
  // set tab.sql directly; the editor applies it reactively (on mount or via watch)
  if (tab) tab.sql = sql;
  showHistory.value = false;
}

function loadSavedInto(id: string, sql: string, name: string) {
  const tab = queries.tabs.find((t) => t.id === id);
  if (tab) { tab.title = name; tab.sql = sql; }
}
function openSavedInNewTab(sql: string, name: string) {
  if (!ws.activeId) return;
  const t = queries.addTab(ws.activeId);
  mainTab.value = t.id;
  loadSavedInto(t.id, sql, name);
}
const overridePick = ref<{ sql: string; name: string } | null>(null);
// Picking a saved query never clobbers work: reuse the current tab only when it's
// empty or already shows this exact query; otherwise open a new tab (asking first
// if the current tab holds different content).
function pickSaved(sql: string, name = "") {
  const activeIsQuery = mainTab.value !== "data" && mainTab.value !== "structure";
  const tab = activeIsQuery ? queries.tabs.find((t) => t.id === mainTab.value) : undefined;
  if (!tab) { openSavedInNewTab(sql, name); return; }
  const cur = tab.sql.trim();
  if (!cur) { loadSavedInto(tab.id, sql, name); return; }      // empty tab → reuse
  if (cur === sql.trim()) { tab.title = name; return; }        // already this query
  overridePick.value = { sql, name };                          // different content → ask
}
function confirmOverride(mode: "override" | "newTab") {
  const p = overridePick.value;
  overridePick.value = null;
  if (!p) return;
  if (mode === "override") loadSavedInto(mainTab.value as string, p.sql, p.name);
  else openSavedInNewTab(p.sql, p.name);
}
// close ONE connection (and its query tabs); confirm first if it has an open tx
const pendingCloseConn = ref<string | null>(null);
function requestCloseConn(id: string | null) {
  if (!id) return;
  const hasTx = queries.tabs.some((t) => t.connectionId === id && t.txOpen);
  if (hasTx) { pendingCloseConn.value = id; return; }
  closeConnectionFull(id);
}
async function closeConnectionFull(id: string) {
  pendingCloseConn.value = null;
  queries.closeConnectionTabs(id);
  await ws.closeConnection(id);
  if (!ws.isOpen) { redisEntry.value = null; showHistory.value = false; showSaved.value = false; }
}

// ---- global keyboard shortcuts ----
const shortcuts = useShortcutsStore();
const showHelp = ref(false);
const showSettings = ref(false);
const platform = detectPlatform();
const keyBindings = bindings(platform);

function tabOrder(): string[] {
  return ["data", "structure", ...activeQueryTabs.value.map((t) => t.id)];
}
function cycleTab(dir: 1 | -1) {
  const order = tabOrder();
  const i = order.indexOf(mainTab.value);
  mainTab.value = order[(i + dir + order.length) % order.length];
}

function resolveAction(action: string): string | null {
  const sql = ws.isOpen && ws.paradigm === "sql";
  if (action === "launcher:new") return ws.isOpen ? null : action;
  if (action.startsWith("tabs:") || action === "table:new" || action === "sidebar:focusFilter" || action === "history:toggle") {
    return sql ? action : null;
  }
  if (action === "view:refresh") {
    if (!sql) return null;
    if (mainTab.value === "data") return "grid:refresh";
    if (mainTab.value === "structure") return "structure:refresh";
    return `query:rerun:${mainTab.value}`;
  }
  if (action === "grid:commit") {
    // ⌘S/Ctrl+S: commit pending edits in the Data grid, but save the query
    // when a query tab is active.
    if (!sql) return null;
    if (mainTab.value === "data") return action;
    if (mainTab.value === "structure") return null;
    return `query:save:${mainTab.value}`;
  }
  if (action.startsWith("grid:")) return sql && mainTab.value === "data" ? action : null;
  if (action === "query:cancel") {
    return sql && mainTab.value !== "data" && mainTab.value !== "structure" ? `query:cancel:${mainTab.value}` : null;
  }
  if (action === "workspace:disconnect") return ws.isOpen ? action : null;
  return action; // window:new, settings:toggle, help:toggle
}

function runAction(action: string) {
  switch (action) {
    case "tabs:new": newQueryTab(); return;
    case "table:new": newTableForm(); return;
    case "tabs:close":
      if (mainTab.value !== "data" && mainTab.value !== "structure") closeQueryTab(mainTab.value);
      return;
    case "tabs:data":
      if (ws.openTables.length && !ws.selectedTable) ws.selectTable(ws.openTables[0]);
      mainTab.value = "data";
      return;
    case "tabs:structure": if (ws.selectedTable) mainTab.value = "structure"; return;
    case "tabs:next": cycleTab(1); return;
    case "tabs:prev": cycleTab(-1); return;
    case "history:toggle": showHistory.value = !showHistory.value; return;
    case "workspace:disconnect": requestCloseConn(ws.activeId); return;
    case "window:new": api.openNewWindow(); return;
    case "settings:toggle": showSettings.value = !showSettings.value; return;
    case "help:toggle": showHelp.value = !showHelp.value; return;
  }
  if (action.startsWith("tabs:index:")) {
    const n = Number(action.split(":")[2]);
    const tab = activeQueryTabs.value[n];
    if (tab) mainTab.value = tab.id;
    return;
  }
  shortcuts.dispatch(action);
}

function onKeydown(e: KeyboardEvent) {
  const combo = comboOf(e);
  if (!combo) return;
  const action = keyBindings[combo];
  if (!action) return;
  // bare keys (F5) don't fire while typing; mod-combos do
  const target = e.target as HTMLElement | null;
  const typing = !!target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable);
  const hasMod = e.metaKey || e.ctrlKey;
  if (typing && !hasMod) return;
  const resolved = resolveAction(action);
  if (!resolved) return;
  e.preventDefault();
  runAction(resolved);
}
onMounted(() => window.addEventListener("keydown", onKeydown));
onBeforeUnmount(() => window.removeEventListener("keydown", onKeydown));
</script>

<template>
  <main class="h-full flex flex-col">
    <ConnectionLauncher v-if="!ws.isOpen || showLauncher" :can-cancel="ws.isOpen"
      @connected="onConnected" @cancel="showLauncher = false" @settings="showSettings = true" />
    <template v-else>
      <header class="h-9 shrink-0 flex items-center gap-2 px-3 border-b border-zinc-800">
        <button @click="showSettings = true"
          class="text-zinc-500 hover:text-zinc-200 text-sm leading-none" title="Settings (⌘,)">⚙</button>
        <div class="w-px h-4 bg-zinc-800 mx-1"></div>
        <span v-if="active?.isProduction" class="text-[10px] uppercase tracking-wide text-red-400 border border-red-900 rounded px-1">prod</span>
        <button @click="api.openNewWindow()" class="ml-auto text-xs text-zinc-500 hover:text-zinc-300" title="New window (⌘⇧N)">New Window</button>
        <button @click="requestCloseConn(ws.activeId)"
          class="text-xs px-2 py-0.5 rounded border border-red-900/70 text-red-400 hover:bg-red-950/60">Disconnect</button>
      </header>
      <!-- connection tabs: switch between open databases -->
      <nav class="h-8 shrink-0 flex items-center gap-1 px-2 border-b border-zinc-800 text-xs overflow-x-auto">
        <button v-for="c in ws.conns" :key="c.id" @click="ws.setActive(c.id)"
          class="px-2 py-1 rounded flex items-center gap-1.5 whitespace-nowrap"
          :class="ws.activeId === c.id ? 'bg-zinc-800 text-white' : 'text-zinc-400 hover:bg-zinc-900'">
          <span class="w-2 h-2 rounded-full shrink-0" :class="colorClass[connMeta(c.id)?.color ?? 'gray']" />
          {{ connMeta(c.id)?.name ?? c.id }}
          <span class="text-[9px] uppercase text-zinc-500">{{ connMeta(c.id)?.engine }}</span>
          <span class="text-zinc-600 hover:text-red-400" @click.stop="requestCloseConn(c.id)">×</span>
        </button>
        <button @click="showLauncher = true" class="px-2 py-1 rounded text-zinc-500 hover:bg-zinc-900"
          title="Open another connection">+</button>
      </nav>
      <div class="flex-1 flex min-h-0">
        <SchemaSidebar v-if="ws.paradigm === 'sql'" :width="panels.sidebarW" @new-table="newTableForm" />
        <ResizeHandle v-if="ws.paradigm === 'sql'" axis="x" :min="160" :max="520"
          v-model="panels.sidebarW" />
        <div v-if="ws.paradigm === 'sql'" class="flex-1 min-w-0 flex flex-col">
          <nav class="h-8 shrink-0 flex items-center gap-1 px-2 border-b border-zinc-800 text-xs overflow-x-auto">
            <!-- one tab per open table (TablePlus-style) -->
            <button v-for="t in ws.openTables" :key="ws.tableKey(t)"
              @click="selectOpenTable(t)"
              class="px-2 py-1 rounded flex items-center gap-1 whitespace-nowrap"
              :class="(mainTab === 'data' || mainTab === 'structure') && ws.selectedTable && ws.tableKey(ws.selectedTable) === ws.tableKey(t)
                ? 'bg-zinc-800 text-white' : 'text-zinc-500 hover:bg-zinc-900'">
              {{ t.name }}
              <span class="text-zinc-600 hover:text-red-400" @click.stop="ws.closeTable(t)">×</span>
            </button>
            <button v-for="t in activeQueryTabs" :key="t.id" @click="mainTab = t.id"
              class="px-2 py-1 rounded flex items-center gap-1"
              :class="mainTab === t.id ? 'bg-zinc-800 text-white' : 'text-zinc-500 hover:bg-zinc-900'">
              {{ t.title }}
              <span v-if="t.txOpen"
                class="text-[9px] font-semibold uppercase text-amber-400 border border-amber-800 rounded px-1"
                title="Open transaction — COMMIT or ROLLBACK to finish">TX</span>
              <span class="text-zinc-600 hover:text-red-400" @click.stop="closeQueryTab(t.id)">×</span>
            </button>
            <button @click="newQueryTab" class="px-2 py-1 rounded text-zinc-500 hover:bg-zinc-900">+</button>
            <button @click="showSaved = !showSaved; if (showSaved) showHistory = false"
              class="ml-auto px-2 py-1 rounded" :class="showSaved ? 'bg-zinc-800 text-white' : 'text-zinc-500 hover:bg-zinc-900'">Saved</button>
            <button @click="showHistory = !showHistory; if (showHistory) showSaved = false"
              class="px-2 py-1 rounded" :class="showHistory ? 'bg-zinc-800 text-white' : 'text-zinc-500 hover:bg-zinc-900'">History</button>
          </nav>
          <div class="flex-1 flex min-h-0">
            <template v-if="mainTab === 'data'">
              <DataGrid v-if="ws.selectedTable" @switch-sub="mainTab = $event" />
              <section v-else class="flex-1 flex items-center justify-center text-zinc-600 text-xs">Select a table</section>
            </template>
            <StructureView v-show="mainTab === 'structure'" ref="structureRef" @switch-sub="mainTab = $event" />
            <QueryEditor v-for="t in activeQueryTabs" :key="t.id" v-show="mainTab === t.id"
              :ref="(el) => (editorRefs[t.id] = el as any)" :tab="t" />
            <ResizeHandle v-if="showHistory || showSaved" axis="x" invert :min="200" :max="560"
              v-model="panels.sidePanelW" />
            <HistoryPanel v-if="showHistory" :width="panels.sidePanelW" @pick="pickHistory" />
            <SavedQueriesPanel v-if="showSaved" :width="panels.sidePanelW" @pick="pickSaved" @close="showSaved = false" />
          </div>
        </div>
        <template v-else>
          <KeyBrowser @select="loadKey" />
          <RedisValuePane :entry="redisEntry"
            @changed="redisEntry && loadKey(redisEntry.key)"
            @deleted="redisEntry = null" />
        </template>
      </div>
    </template>
    <ShortcutHelpModal v-if="showHelp" @close="showHelp = false" />
    <SettingsModal v-if="showSettings" @close="showSettings = false" />
    <ConfirmModal v-if="pendingCloseConn" title="Disconnect"
      :message="`Disconnect from ${connMeta(pendingCloseConn)?.name}? A tab has an open transaction that will be ROLLED BACK.`"
      confirm-label="Disconnect & rollback" danger
      @confirm="closeConnectionFull(pendingCloseConn!)" @cancel="pendingCloseConn = null" />
    <ConfirmModal v-if="pendingCloseTab" title="Open transaction"
      message="This tab has an uncommitted transaction. Closing it will ROLLBACK those changes."
      confirm-label="Close & rollback" danger
      @confirm="reallyCloseTab(pendingCloseTab!)" @cancel="pendingCloseTab = null" />
    <UnsavedCloseModal v-if="unsavedCloseTab" :initial-name="unsavedCloseTab.title"
      @save="saveAndCloseTab" @discard="reallyCloseTab(unsavedCloseTab!.id)" @cancel="unsavedCloseTab = null" />
    <OverrideSavedModal v-if="overridePick" :name="overridePick.name"
      @new-tab="confirmOverride('newTab')" @override="confirmOverride('override')" @cancel="overridePick = null" />
  </main>
</template>
