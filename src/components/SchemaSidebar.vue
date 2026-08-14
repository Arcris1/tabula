<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
defineProps<{ width?: number }>();
const emit = defineEmits<{ "new-table": [] }>();
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { api, type DdlOp, type TableInfo } from "../lib/api";
import { useConnectionsStore } from "../stores/connections";
import { usePinnedTablesStore } from "../stores/pinnedTables";
import { useSettingsStore } from "../stores/settings";
import { useShortcutsStore } from "../stores/shortcuts";
import { useToastStore } from "../stores/toast";
import { useWorkspaceStore } from "../stores/workspace";
import ConfirmModal from "./ConfirmModal.vue";
import DangerConfirmModal from "./DangerConfirmModal.vue";
import FunctionViewer from "./FunctionViewer.vue";
import SqlPreviewModal from "./SqlPreviewModal.vue";

const ws = useWorkspaceStore();
const conns = useConnectionsStore();
const settings = useSettingsStore();
const pinned = usePinnedTablesStore();

function togglePin(t: TableInfo) {
  if (ws.activeId) pinned.toggle(ws.activeId, ws.tableKey(t));
  closeMenu();
}
function isPinned(t: TableInfo): boolean {
  return !!ws.activeId && pinned.isPinned(ws.activeId, ws.tableKey(t));
}

// ---- right-click table context menu ----
const menu = ref<{ x: number; y: number; table: TableInfo } | null>(null);
const previewSql = ref<string | null>(null);
const pendingOp = ref<DdlOp | null>(null);
const confirmProd = ref(false);
const dangerConfirm = ref<{ verb: string; table: string } | null>(null);
const opError = ref<string | null>(null);
const toast = useToastStore();

function openMenu(e: MouseEvent, table: TableInfo) {
  menu.value = { x: e.clientX, y: e.clientY, table };
  submenu.value = null;
}
function closeMenu() { menu.value = null; submenu.value = null; }
onMounted(() => window.addEventListener("click", closeMenu));
onBeforeUnmount(() => window.removeEventListener("click", closeMenu));

function copyName(t: TableInfo) {
  navigator.clipboard?.writeText(t.name);
  closeMenu();
}

const submenu = ref<null | "export" | "import">(null);
const busy = ref<string | null>(null);

async function exportTable(t: TableInfo, format: "csv" | "json") {
  closeMenu();
  if (!ws.activeId) return;
  const path = await saveDialog({
    defaultPath: `${t.name}.${format}`,
    filters: [{ name: format.toUpperCase(), extensions: [format] }],
  });
  if (!path) return;
  busy.value = `Exporting ${t.name}…`;
  try {
    const n = await api.exportTable(ws.activeId, t, format, path);
    toast.success(`Exported ${n.toLocaleString()} rows to ${path}`);
  } catch (e: any) {
    opError.value = e?.message ?? String(e);
  } finally {
    busy.value = null;
  }
}

async function importTable(t: TableInfo, format: "csv" | "json" | "sql") {
  closeMenu();
  if (!ws.activeId) return;
  const ext = format === "sql" ? "sql" : format;
  const path = await openDialog({ filters: [{ name: format.toUpperCase(), extensions: [ext] }] });
  if (!path || typeof path !== "string") return;
  busy.value = `Importing into ${t.name}…`;
  try {
    const r = await api.importTable(ws.activeId, t, format, path);
    toast.success(`Imported ${r.inserted.toLocaleString()} ${format === "sql" ? "statement(s)" : "row(s)"} into ${t.name}`);
    if (ws.selectedTable && ws.tableKey(ws.selectedTable) === ws.tableKey(t)) ws.loadTables();
  } catch (e: any) {
    opError.value = e?.message ?? String(e);
  } finally {
    busy.value = null;
  }
}
function openStructure(t: TableInfo) {
  ws.selectTable(t);
  closeMenu();
}
async function stageOp(op: DdlOp) {
  if (!ws.activeId) return;
  closeMenu();
  opError.value = null;
  try {
    pendingOp.value = op;
    previewSql.value = await api.previewDdl(ws.activeId, op);
  } catch (e: any) {
    opError.value = e?.message ?? String(e);
  }
}
async function executeOp(skipProd = false, skipDanger = false) {
  if (!ws.activeId || !pendingOp.value) return;
  const op = pendingOp.value;
  // Type-the-name gate for irreversible table ops (honors the warnDropTruncate rail).
  if (
    !skipDanger &&
    settings.rails.warnDropTruncate &&
    (op.op === "dropTable" || op.op === "truncateTable")
  ) {
    dangerConfirm.value = {
      verb: op.op === "dropTable" ? "DROP" : "TRUNCATE",
      table: (op as any).table.name,
    };
    return;
  }
  dangerConfirm.value = null;
  const prod = conns.connections.find((c) => c.id === ws.activeId)?.isProduction;
  if (prod && settings.rails.warnProductionWrites && !skipProd) { confirmProd.value = true; return; }
  confirmProd.value = false;
  try {
    await api.applyDdl(ws.activeId, op);
    previewSql.value = null;
    pendingOp.value = null;
    // if the affected table was open/selected, refresh listing
    await ws.loadTables();
    if ((op.op === "dropTable") && ws.selectedTable && ws.tableKey(ws.selectedTable) === ws.tableKey((op as any).table)) {
      ws.closeTable((op as any).table);
    }
  } catch (e: any) {
    previewSql.value = null; pendingOp.value = null;
    opError.value = e?.message ?? String(e);
  }
}
const fnView = ref<{ name: string; definition: string; loading: boolean } | null>(null);
async function openFunction(name: string) {
  fnView.value = { name, definition: "", loading: true };
  try {
    if (ws.activeId) fnView.value.definition = await api.functionDefinition(ws.activeId, name);
  } catch {
    /* leave empty -> "no source" */
  }
  if (fnView.value) fnView.value.loading = false;
}
const filter = ref("");
const filterInput = ref<HTMLInputElement | null>(null);
const showFunctions = ref(true);
const showTables = ref(true);
const unregister = useShortcutsStore().register("sidebar:focusFilter", () => filterInput.value?.focus());
onBeforeUnmount(unregister);

const q = computed(() => filter.value.toLowerCase());

// schema selector (postgres has multiple; mysql/sqlite have one)
const schemas = computed(() => {
  const set = new Set<string>();
  for (const t of ws.tables) if (t.schema) set.add(t.schema);
  return [...set].sort();
});
const schema = ref<string | null>(null);
const filteredTables = computed(() => {
  const base = ws.tables.filter((t) =>
    t.name.toLowerCase().includes(q.value) && (!schema.value || t.schema === schema.value),
  );
  // pinned tables float to the top, in pin order
  if (!ws.activeId) return base;
  const pins = pinned.list(ws.activeId);
  return [...base].sort((a, b) => {
    const ai = pins.indexOf(ws.tableKey(a));
    const bi = pins.indexOf(ws.tableKey(b));
    if (ai === -1 && bi === -1) return 0;
    if (ai === -1) return 1;
    if (bi === -1) return -1;
    return ai - bi;
  });
});
const filteredFunctions = computed(() => ws.functions.filter((f) => f.toLowerCase().includes(q.value)));
const label = (t: { schema?: string | null; name: string }) =>
  t.schema && t.schema !== "public" ? `${t.schema}.${t.name}` : t.name;

// lazy column expansion per table
const expanded = ref<Set<string>>(new Set());
const columnsByTable = ref<Record<string, { name: string; dataType: string; isPk: boolean }[]>>({});
async function toggleExpand(t: TableInfo) {
  const key = ws.tableKey(t);
  if (expanded.value.has(key)) {
    expanded.value.delete(key);
  } else {
    expanded.value.add(key);
    if (!columnsByTable.value[key] && ws.activeId) {
      try {
        const st = await api.tableStructure(ws.activeId, t);
        columnsByTable.value[key] = st.columns.map((c) => ({ name: c.name, dataType: c.dataType, isPk: c.isPk }));
      } catch {
        columnsByTable.value[key] = [];
      }
    }
  }
  expanded.value = new Set(expanded.value);
}
</script>

<template>
  <aside class="shrink-0 border-r border-zinc-800 flex flex-col" :style="{ width: (width ?? 224) + 'px' }">
    <div class="m-2 flex items-center gap-1">
      <input ref="filterInput" v-model="filter" placeholder="Filter…"
        class="flex-1 min-w-0 bg-zinc-900 border border-zinc-800 rounded px-2 py-1 text-xs outline-none focus:border-zinc-600" />
      <button @click="emit('new-table')" title="New table (⌘⇧T)"
        class="shrink-0 w-7 h-[26px] flex items-center justify-center rounded border border-zinc-800 text-zinc-400 hover:bg-zinc-800 hover:text-zinc-100">
        <span class="text-base leading-none">+</span>
      </button>
    </div>
    <div class="flex-1 overflow-y-auto px-1 pb-2">
      <!-- Functions -->
      <template v-if="ws.functions.length">
        <button @click="showFunctions = !showFunctions"
          class="w-full text-left px-2 py-1 text-[11px] uppercase tracking-wide text-zinc-500 hover:text-zinc-300">
          <span class="inline-block w-3">{{ showFunctions ? "▾" : "▸" }}</span>Functions ({{ filteredFunctions.length }})
        </button>
        <ul v-show="showFunctions">
          <li v-for="f in filteredFunctions" :key="f"
            class="px-2 py-1 rounded truncate text-[12.5px] text-zinc-400 cursor-pointer hover:bg-zinc-900"
            :title="f" @click="openFunction(f)">
            <span class="text-zinc-600 mr-1">ƒ</span>{{ f }}
          </li>
        </ul>
      </template>

      <!-- Tables -->
      <button @click="showTables = !showTables"
        class="w-full text-left px-2 py-1 text-[11px] uppercase tracking-wide text-zinc-500 hover:text-zinc-300">
        <span class="inline-block w-3">{{ showTables ? "▾" : "▸" }}</span>Tables ({{ filteredTables.length }})
      </button>
      <ul v-show="showTables">
        <template v-for="t in filteredTables" :key="label(t)">
        <li class="flex items-center px-1 py-1 rounded text-[12.5px]"
          :class="ws.selectedTable === t ? 'bg-zinc-800 text-white' : 'text-zinc-400 hover:bg-zinc-900'"
          @contextmenu.prevent="openMenu($event, t)">
          <button class="w-4 text-zinc-500 hover:text-zinc-300 shrink-0"
            :aria-label="expanded.has(ws.tableKey(t)) ? 'Collapse columns' : 'Expand columns'"
            @click.stop="toggleExpand(t)">{{ expanded.has(ws.tableKey(t)) ? "▾" : "▸" }}</button>
          <button class="flex items-center min-w-0 flex-1 text-left" :title="label(t)" @click="ws.selectTable(t)">
            <span class="text-zinc-500 mr-1" aria-hidden="true">{{ t.kind === "view" ? "◇" : "▤" }}</span>
            <span class="truncate">{{ label(t) }}</span>
          </button>
          <span v-if="isPinned(t)" class="ml-auto text-amber-400 shrink-0" title="Pinned" aria-label="Pinned">
            <svg width="11" height="11" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M14 4v6l3 3v2H7v-2l3-3V4H9V2h6v2z"/><path d="M11 15h2v7h-2z"/></svg>
          </span>
        </li>
        <ul v-if="expanded.has(ws.tableKey(t))" class="ml-4">
          <li v-for="c in columnsByTable[ws.tableKey(t)] ?? []" :key="c.name"
            class="flex items-center gap-1 px-2 py-0.5 text-[11.5px] text-zinc-400">
            <span v-if="c.isPk" class="text-[9px] font-semibold text-amber-400 shrink-0" title="primary key">PK</span>
            <span v-else class="w-4 shrink-0"></span>
            <span class="truncate">{{ c.name }}</span>
            <span class="ml-auto text-zinc-600 shrink-0">{{ c.dataType }}</span>
          </li>
          <li v-if="!(columnsByTable[ws.tableKey(t)] ?? []).length" class="px-2 py-0.5 text-[11px] text-zinc-600">loading…</li>
        </ul>
        </template>
        <li v-if="!filteredTables.length" class="text-zinc-600 text-xs px-2 py-2">No tables</li>
      </ul>
    </div>
    <!-- schema selector -->
    <div v-if="schemas.length > 1" class="shrink-0 border-t border-zinc-800 p-1">
      <select v-model="schema"
        class="w-full bg-zinc-900 border border-zinc-800 rounded px-2 py-1 text-[11px] outline-none focus:border-zinc-600">
        <option :value="null">all schemas</option>
        <option v-for="s in schemas" :key="s" :value="s">{{ s }}</option>
      </select>
    </div>
    <FunctionViewer v-if="fnView" :name="fnView.name" :definition="fnView.definition"
      :loading="fnView.loading" @close="fnView = null" />

    <!-- table context menu -->
    <div v-if="menu" class="fixed z-50 w-44 rounded border border-zinc-700 bg-zinc-900 py-1 text-xs shadow-lg"
      :style="{ left: menu.x + 'px', top: menu.y + 'px' }" @click.stop>
      <button class="w-full text-left px-3 py-1 hover:bg-zinc-800" @click="openStructure(menu.table)">Open</button>
      <button class="w-full text-left px-3 py-1 hover:bg-zinc-800" @click="copyName(menu.table)">Copy name</button>
      <button class="w-full text-left px-3 py-1 hover:bg-zinc-800" @click="togglePin(menu.table)">
        {{ isPinned(menu.table) ? "Unpin" : "Pin to top" }}
      </button>
      <div class="my-1 border-t border-zinc-800"></div>
      <!-- Export flyout -->
      <div class="relative" @mouseenter="submenu = 'export'">
        <button class="w-full text-left px-3 py-1 hover:bg-zinc-800 flex items-center">Export<span class="ml-auto text-zinc-600">›</span></button>
        <div v-if="submenu === 'export'" class="absolute left-full top-0 w-40 rounded border border-zinc-700 bg-zinc-900 py-1 shadow-lg">
          <button class="w-full text-left px-3 py-1 hover:bg-zinc-800" @click="exportTable(menu.table, 'csv')">To CSV…</button>
          <button class="w-full text-left px-3 py-1 hover:bg-zinc-800" @click="exportTable(menu.table, 'json')">To JSON…</button>
        </div>
      </div>
      <!-- Import flyout -->
      <div class="relative" @mouseenter="submenu = 'import'">
        <button class="w-full text-left px-3 py-1 hover:bg-zinc-800 flex items-center">Import<span class="ml-auto text-zinc-600">›</span></button>
        <div v-if="submenu === 'import'" class="absolute left-full top-0 w-40 rounded border border-zinc-700 bg-zinc-900 py-1 shadow-lg">
          <button class="w-full text-left px-3 py-1 hover:bg-zinc-800" @click="importTable(menu.table, 'csv')">From CSV…</button>
          <button class="w-full text-left px-3 py-1 hover:bg-zinc-800" @click="importTable(menu.table, 'json')">From JSON…</button>
          <button class="w-full text-left px-3 py-1 hover:bg-zinc-800" @click="importTable(menu.table, 'sql')">From SQL Dump…</button>
        </div>
      </div>
      <div class="my-1 border-t border-zinc-800"></div>
      <button class="w-full text-left px-3 py-1 text-amber-400 hover:bg-zinc-800"
        @click="stageOp({ op: 'truncateTable', table: menu.table })">Truncate…</button>
      <button class="w-full text-left px-3 py-1 text-red-400 hover:bg-zinc-800"
        @click="stageOp({ op: 'dropTable', table: menu.table })">Delete (drop)…</button>
    </div>

    <div v-if="busy" class="fixed bottom-2 left-2 right-2 z-50 px-3 py-2 text-xs text-blue-400 border border-blue-900 rounded bg-zinc-900">{{ busy }}</div>

    <div v-if="opError" class="fixed bottom-2 left-2 right-2 z-50 px-3 py-2 text-xs text-red-400 border border-red-900 rounded bg-zinc-900">
      {{ opError }}
      <button class="ml-2 text-zinc-500" @click="opError = null">dismiss</button>
    </div>
    <SqlPreviewModal v-if="previewSql !== null" :statements="[previewSql]" executable
      @execute="executeOp()" @close="previewSql = null; pendingOp = null" />
    <ConfirmModal v-if="confirmProd" title="DDL on PRODUCTION"
      :message="previewSql ?? ''" confirm-label="Execute" danger
      @confirm="executeOp(true)" @cancel="confirmProd = false" />
    <DangerConfirmModal v-if="dangerConfirm"
      :title="`${dangerConfirm.verb} ${dangerConfirm.table}`"
      :message="dangerConfirm.verb === 'DROP'
        ? `This permanently removes the table “${dangerConfirm.table}” and all of its data. This cannot be undone.`
        : `This permanently deletes every row in “${dangerConfirm.table}”. This cannot be undone.`"
      :expected="dangerConfirm.table"
      :confirm-label="dangerConfirm.verb === 'DROP' ? 'Drop table' : 'Truncate table'"
      @confirm="executeOp(false, true)" @cancel="dangerConfirm = null; previewSql = null; pendingOp = null" />
  </aside>
</template>
