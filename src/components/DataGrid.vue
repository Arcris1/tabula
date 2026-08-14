<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, reactive, ref, watch } from "vue";
import { api, type Filter, type FilterOp, type RowsPage, type Sort } from "../lib/api";
import { parseCellInput } from "../lib/cellInput";
import { parseFk, type FkRef } from "../lib/fk";
import { asJson } from "../lib/jsonCell";
import { visibleRange } from "../lib/virtual";
import { useChangesetStore } from "../stores/changeset";
import { useConnectionsStore } from "../stores/connections";
import { usePanelsStore } from "../stores/panels";
import { useSettingsStore } from "../stores/settings";
import { useShortcutsStore } from "../stores/shortcuts";
import { useToastStore } from "../stores/toast";
import { useWorkspaceStore } from "../stores/workspace";
import ConfirmModal from "./ConfirmModal.vue";
import JsonViewer from "./JsonViewer.vue";
import ResizeHandle from "./ResizeHandle.vue";
import ResultActions from "./ResultActions.vue";
import RowInspector from "./RowInspector.vue";
import SqlPreviewModal from "./SqlPreviewModal.vue";

const emit = defineEmits<{ switchSub: [sub: "data" | "structure"] }>();
const ws = useWorkspaceStore();
const conns = useConnectionsStore();
const cs = useChangesetStore();
const panels = usePanelsStore();
const toast = useToastStore();
const PAGE = 300;
const ROW_H = 28;

const page = ref<RowsPage | null>(null);
const rows = ref<unknown[][]>([]);
const pageIndex = ref(0); // 0-based page
const loading = ref(false);
const highlight = ref(true); // "Enable syntax highlighting" for the SQL panel
const error = ref<string | null>(null);
const sort = ref<Sort | null>(null);
const filterDrafts = reactive<Record<string, string>>({});
const filters = ref<Filter[]>([]);
const scrollTop = ref(0);
const viewportH = ref(600);
const scroller = ref<HTMLElement | null>(null);

const editing = ref<{ rowIndex: number; column: string } | null>(null);
const editValue = ref("");
const editInput = ref<HTMLInputElement | null>(null);
const previewStatements = ref<string[] | null>(null);
const confirmProd = ref(false);
const conflictError = ref<string | null>(null);

const selectedRow = ref<number | null>(null);
// selected cell for keyboard nav + copy: row index + column position within
// visibleCols (so hidden columns are skipped and ⌘C reads the right value).
const selectedCell = ref<{ row: number; vcol: number } | null>(null);
function selectCell(row: number, vcol: number) {
  selectedRow.value = row;
  selectedCell.value = { row, vcol };
  scroller.value?.focus();
}
function copySelectedCell() {
  const sc = selectedCell.value;
  if (!sc) return;
  const raw = rows.value[sc.row]?.[visibleCols.value[sc.vcol]?.i];
  navigator.clipboard?.writeText(raw == null ? "" : String(raw));
}
function ensureRowVisible(row: number) {
  const el = scroller.value;
  if (!el) return;
  const top = row * ROW_H, bottom = top + ROW_H;
  if (top < el.scrollTop) el.scrollTop = top;
  else if (bottom > el.scrollTop + el.clientHeight) el.scrollTop = bottom - el.clientHeight;
}
function onGridKeydown(e: KeyboardEvent) {
  const sc = selectedCell.value;
  if (!sc || editing.value) return;
  // don't hijack a real text selection / copy
  if ((e.key === "c" || e.key === "C") && (e.metaKey || e.ctrlKey)) {
    if (window.getSelection()?.toString()) return;
    e.preventDefault();
    copySelectedCell();
    return;
  }
  const lastRow = rows.value.length - 1;
  const lastCol = visibleCols.value.length - 1;
  let handled = true;
  if (e.key === "ArrowDown") selectCell(Math.min(sc.row + 1, lastRow), sc.vcol);
  else if (e.key === "ArrowUp") selectCell(Math.max(sc.row - 1, 0), sc.vcol);
  else if (e.key === "ArrowRight") selectCell(sc.row, Math.min(sc.vcol + 1, lastCol));
  else if (e.key === "ArrowLeft") selectCell(sc.row, Math.max(sc.vcol - 1, 0));
  else if (e.key === "Enter") beginEdit(sc.row, visibleCols.value[sc.vcol].c.name);
  else handled = false;
  if (handled) { e.preventDefault(); ensureRowVisible(selectedCell.value!.row); }
}
const jsonView = ref<{ value: unknown; title: string } | null>(null);
function openJson(value: unknown, title: string) {
  const parsed = asJson(value);
  if (parsed !== null) jsonView.value = { value: parsed, title };
}
const showFilters = ref(false);
const showColumnsPicker = ref(false);
const hiddenCols = reactive(new Set<string>());
const visibleCols = computed(() =>
  (page.value?.columns ?? []).map((c, i) => ({ c, i })).filter((x) => !hiddenCols.has(x.c.name)),
);
function toggleCol(name: string) {
  if (hiddenCols.has(name)) hiddenCols.delete(name);
  else hiddenCols.add(name);
}
const inspectorRow = computed(() =>
  selectedRow.value !== null ? rows.value[selectedRow.value] ?? null : null,
);
function inspectorCell(column: string) {
  const row = inspectorRow.value!;
  return cs.cellValue(row, columnNames.value, column);
}
function inspectorEdit(column: string, textVal: string) {
  const row = inspectorRow.value;
  if (!row) return;
  const original = row[columnNames.value.indexOf(column)];
  cs.editCell(row, columnNames.value, column, parseCellInput(textVal, original));
}
function inspectorSetNull(column: string) {
  const row = inspectorRow.value;
  if (!row) return;
  cs.editCell(row, columnNames.value, column, null);
}

// "start-end of ~total" range display
// True when the last fetch peeked a row beyond the visible page (we request
// PAGE+1, show PAGE). Deterministic end-of-data — no more "Next → empty grid".
const hasMore = ref(false);
const rangeLabel = computed(() => {
  if (!rows.value.length) return "0"; // template appends " rows" → "0 rows"
  const start = pageIndex.value * PAGE + 1;
  const end = start + rows.value.length - 1;
  const total = page.value?.totalEstimate;
  // total is an engine ESTIMATE (information_schema / sys.partitions) — mark it
  // approximate rather than implying an exact count that jumps around.
  if (total == null) return `${start}-${end}`;
  return end > total ? `${start}-${end} of ~${end}` : `${start}-${end} of ~${total}`;
});
const hasNextPage = computed(() => hasMore.value);

const columnNames = computed(() => (page.value?.columns ?? []).map((c) => c.name));
const range = computed(() => visibleRange(scrollTop.value, viewportH.value, ROW_H, rows.value.length));
const slice = computed(() => rows.value.slice(range.value.start, range.value.end));

/** loads the current page (offset = pageIndex * PAGE), replacing rows */
async function load(resetPage: boolean) {
  if (!ws.activeId || !ws.selectedTable || loading.value) return;
  if (resetPage) pageIndex.value = 0;
  loading.value = true;
  error.value = null;
  selectedRow.value = null;
  try {
    const p = await api.fetchRows(ws.activeId, ws.selectedTable, {
      limit: PAGE + 1, offset: pageIndex.value * PAGE, sort: sort.value, filters: filters.value,
    });
    hasMore.value = p.rows.length > PAGE;
    if (hasMore.value) p.rows = p.rows.slice(0, PAGE); // drop the peeked row
    page.value = p;
    rows.value = p.rows;
    if (scroller.value) scroller.value.scrollTop = 0;
  } catch (e: any) {
    error.value = e?.message ?? String(e);
  } finally {
    loading.value = false;
  }
}
function nextPage() { if (hasNextPage.value) { pageIndex.value++; load(false); } }
function prevPage() { if (pageIndex.value > 0) { pageIndex.value--; load(false); } }

// Full-result export: the whole table under the ACTIVE filters+sort (not just the
// visible page), streamed server-side. Wired into ResultActions' Export buttons.
async function exportCurrent(format: "csv" | "json", path: string): Promise<number> {
  if (!ws.activeId || !ws.selectedTable) throw new Error("no table selected");
  return api.exportTable(ws.activeId, ws.selectedTable, format, path, sort.value, filters.value);
}

async function loadPk() {
  if (!ws.activeId || !ws.selectedTable) return;
  try {
    cs.setContext(await api.tablePrimaryKey(ws.activeId, ws.selectedTable));
  } catch {
    cs.setContext([]);
  }
}

// column name -> foreign key reference, for click-to-navigate cells
const fkMap = ref<Record<string, FkRef>>({});
async function loadFks() {
  fkMap.value = {};
  if (!ws.activeId || !ws.selectedTable) return;
  try {
    const struct = await api.tableStructure(ws.activeId, ws.selectedTable);
    const map: Record<string, FkRef> = {};
    for (const c of struct.columns) {
      const ref = parseFk(c.foreignKey);
      if (ref) map[c.name] = ref;
    }
    fkMap.value = map;
  } catch {
    fkMap.value = {};
  }
}
function followFk(column: string, value: unknown) {
  const ref = fkMap.value[column];
  if (!ref || value === null || value === undefined) return;
  ws.navigateToRow(ref.table, ref.column, String(value));
}

watch(() => ws.selectedTable, () => {
  sort.value = null;
  filters.value = [];
  Object.keys(filterDrafts).forEach((k) => delete filterDrafts[k]);
  // consume a pending FK-navigation filter, if one was set
  if (ws.pendingFilter) {
    const pf = ws.pendingFilter;
    filters.value = [{ column: pf.column, op: "equals", value: pf.value }];
    filterOps[pf.column] = "equals";
    filterDrafts[pf.column] = pf.value;
    showFilters.value = true;
    ws.pendingFilter = null;
  }
  editing.value = null;
  selectedRow.value = null;
  conflictError.value = null;
  if (scroller.value) scroller.value.scrollTop = 0;
  load(true);
  loadPk();
  loadFks();
}, { immediate: true });

function toggleSort(col: string) {
  sort.value = sort.value?.column === col
    ? sort.value.desc ? null : { column: col, desc: true }
    : { column: col, desc: false };
  load(true);
}

const filterOps = reactive<Record<string, FilterOp>>({});

function applyFilter(col: string) {
  const op: FilterOp = filterOps[col] ?? "contains";
  const v = (filterDrafts[col] ?? "").trim();
  filters.value = filters.value.filter((f) => f.column !== col);
  const needsValue = op === "contains" || op === "equals";
  if (!needsValue || v) filters.value.push({ column: col, op, value: v });
  load(true);
}

function onOpChange(col: string) {
  const op = filterOps[col];
  if (op === "isNull" || op === "notNull") applyFilter(col);
  else if ((filterDrafts[col] ?? "").trim()) applyFilter(col);
  else { filters.value = filters.value.filter((f) => f.column !== col); load(true); }
}

function onScroll(e: Event) {
  const el = e.target as HTMLElement;
  scrollTop.value = el.scrollTop;
  viewportH.value = el.clientHeight;
}

function cellText(v: unknown): string {
  if (v === null || v === undefined) return "";
  return typeof v === "object" ? JSON.stringify(v) : String(v);
}

function beginEdit(rowIndex: number, column: string) {
  if (!cs.editable) return;
  const row = rows.value[rowIndex];
  const { value } = cs.cellValue(row, columnNames.value, column);
  editValue.value = value === null ? "NULL" : cellText(value);
  editing.value = { rowIndex, column };
  nextTick(() => editInput.value?.focus());
}

function commitEdit() {
  if (!editing.value) return;
  const { rowIndex, column } = editing.value;
  const row = rows.value[rowIndex];
  const original = row[columnNames.value.indexOf(column)];
  cs.editCell(row, columnNames.value, column, parseCellInput(editValue.value, original));
  editing.value = null;
}

async function preview() {
  if (!ws.activeId || !ws.selectedTable) return;
  previewStatements.value = await api.previewChangeset(ws.activeId, ws.selectedTable, cs.toChangeset());
}

async function commit(skipProdCheck = false) {
  if (!ws.activeId || !ws.selectedTable) return;
  const conn = conns.connections.find((c) => c.id === ws.activeId);
  if (conn?.isProduction && useSettingsStore().rails.warnProductionWrites && !skipProdCheck) {
    confirmProd.value = true;
    return;
  }
  confirmProd.value = false;
  conflictError.value = null;
  try {
    const n = cs.count;
    const r = await api.applyChangeset(ws.activeId, ws.selectedTable, cs.toChangeset());
    cs.clear();
    load(true);
    toast.success(`Committed ${(r.updated + r.inserted + r.deleted) || n} change(s)`);
  } catch (e: any) {
    conflictError.value = e?.message ?? String(e);
  }
}

function discard() {
  cs.clear();
  conflictError.value = null;
}

const filterInputs = ref<HTMLInputElement[]>([]);
const shortcuts = useShortcutsStore();
const unregisters = [
  shortcuts.register("grid:refresh", () => load(true)),
  shortcuts.register("grid:commit", () => { if (cs.count > 0) commit(); }),
  shortcuts.register("grid:discard", () => { if (cs.count > 0) discard(); }),
  shortcuts.register("grid:insertRow", () => { if (cs.editable) cs.addDraft(columnNames.value); }),
  shortcuts.register("grid:focusFilter", () => {
    showFilters.value = true; // reveal the filter row first — it's collapsed by default
    nextTick(() => filterInputs.value[0]?.focus());
  }),
];
onBeforeUnmount(() => unregisters.forEach((u) => u()));
</script>

<template>
  <div class="flex-1 min-w-0 flex">
   <div class="flex-1 min-w-0 flex flex-col">
    <div v-if="error" class="m-2 px-3 py-2 text-xs text-red-400 border border-red-900 rounded">{{ error }}</div>
    <div v-if="conflictError" class="m-2 px-3 py-2 text-xs text-amber-400 border border-amber-900 rounded">
      {{ conflictError }} — nothing was applied; your pending changes are kept.
    </div>
    <div v-if="page && !cs.editable" class="mx-2 mt-2 px-3 py-1.5 text-[11px] text-zinc-500 border border-zinc-800 rounded">
      read-only: table has no primary key
    </div>

    <div ref="scroller" tabindex="0" class="flex-1 overflow-auto font-mono text-[12px] focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-blue-500/60" @scroll="onScroll" @keydown="onGridKeydown">
      <table class="border-collapse min-w-full">
        <thead class="sticky top-0 bg-zinc-950 z-10">
          <tr>
            <th class="w-6 border-b border-zinc-800"></th>
            <th v-for="x in visibleCols" :key="x.c.name"
              class="text-left border-b border-zinc-800 font-medium text-zinc-400 whitespace-nowrap">
              <button type="button" class="w-full text-left px-2 py-1 flex items-center gap-1 hover:text-zinc-200 select-none"
                :title="`Sort by ${x.c.name}`" @click="toggleSort(x.c.name)">
                {{ x.c.name }}
                <span v-if="sort?.column === x.c.name" aria-hidden="true">{{ sort.desc ? "↓" : "↑" }}</span>
              </button>
            </th>
          </tr>
          <tr v-if="showFilters">
            <th class="border-b border-zinc-800"></th>
            <th v-for="x in visibleCols" :key="x.c.name" class="px-1 pb-1 border-b border-zinc-800"><template v-for="c in [x.c]" :key="c.name">
              <div class="flex gap-0.5">
                <select v-model="filterOps[c.name]" @change="onOpChange(c.name)"
                  title="Filter operator" aria-label="Filter operator"
                  class="bg-zinc-900 border border-zinc-800 rounded text-[10px] font-sans text-zinc-400 outline-none w-20">
                  <option value="contains">contains</option>
                  <option value="equals">equals</option>
                  <option value="isNull">is null</option>
                  <option value="notNull">not null</option>
                </select>
                <input ref="filterInputs" v-model="filterDrafts[c.name]" @keydown.enter="applyFilter(c.name)"
                  placeholder="filter…"
                  :disabled="filterOps[c.name] === 'isNull' || filterOps[c.name] === 'notNull'"
                  class="w-full min-w-16 bg-zinc-900 border border-zinc-800 rounded px-1 py-0.5 text-[11px] font-sans outline-none focus:border-zinc-600 disabled:opacity-40" />
              </div>
            </template></th>
          </tr>
        </thead>
        <tbody>
          <!-- insert drafts -->
          <tr v-for="(d, di) in cs.drafts" :key="'draft' + di" class="bg-green-950/30">
            <td class="text-center border-b border-zinc-900">
              <button class="text-red-500/70 hover:text-red-400" @click="cs.removeDraft(di)">×</button>
            </td>
            <td v-for="x in visibleCols" :key="x.c.name" class="border-b border-zinc-900 px-1">
              <input v-model="(d[x.c.name] as any)" :placeholder="x.c.name"
                class="w-full bg-transparent border border-zinc-800 rounded px-1 py-0.5 text-[11px] outline-none focus:border-green-700" />
            </td>
          </tr>
          <tr :style="{ height: `${range.start * ROW_H}px` }" v-if="range.start > 0" />
          <tr v-for="(r, i) in slice" :key="range.start + i"
            class="hover:bg-zinc-900 cursor-default"
            :class="[
              cs.isDeleted(r, columnNames) ? 'line-through opacity-50 bg-red-950/30' : '',
              selectedRow === range.start + i ? 'bg-blue-950/40' : '',
            ]"
            :style="{ height: `${ROW_H}px` }"
            @click="selectedRow = range.start + i">
            <td class="text-center border-b border-zinc-900">
              <button v-if="cs.editable" class="text-zinc-700 hover:text-red-400"
                :class="cs.isDeleted(r, columnNames) ? 'text-red-400' : ''"
                @click="cs.markDelete(r, columnNames)">×</button>
            </td>
            <td v-for="(x, vc) in visibleCols" :key="x.i"
              class="px-2 border-b border-zinc-900 whitespace-nowrap max-w-96 overflow-hidden text-ellipsis"
              :class="selectedCell && selectedCell.row === range.start + i && selectedCell.vcol === vc ? 'ring-1 ring-inset ring-blue-500 bg-blue-500/10' : ''"
              @click="selectCell(range.start + i, vc)"
              @dblclick="beginEdit(range.start + i, x.c.name)">
              <input v-if="editing && editing.rowIndex === range.start + i && editing.column === x.c.name"
                ref="editInput" v-model="editValue" @click.stop @mousedown.stop
                @keydown.enter="commitEdit" @keydown.esc="editing = null" @blur="commitEdit"
                class="w-full bg-zinc-900 border border-amber-700 rounded px-1 py-0.5 text-[11px] outline-none" />
              <template v-else>
                <span v-if="cs.cellValue(r, columnNames, x.c.name).dirty" class="text-amber-300">
                  {{ cs.cellValue(r, columnNames, x.c.name).value === null ? "NULL" : cellText(cs.cellValue(r, columnNames, x.c.name).value) }}
                </span>
                <span v-else-if="r[x.i] === null" class="inline-block rounded px-1 text-[10px] font-medium tracking-wide text-zinc-400 bg-zinc-800/80" title="NULL">NULL</span>
                <span v-else-if="r[x.i] === ''" class="inline-block rounded px-1 text-[10px] text-zinc-400 bg-zinc-800/80" title="empty string">EMPTY</span>
                <template v-else-if="asJson(r[x.i]) !== null">
                  <button class="mr-1 text-[9px] px-1 rounded bg-zinc-800 text-sky-300 hover:bg-zinc-700"
                    @click.stop="openJson(r[x.i], x.c.name)" title="View JSON">{}</button>{{ cellText(r[x.i]) }}
                </template>
                <button v-else-if="fkMap[x.c.name]"
                  class="text-sky-400 hover:text-sky-300 hover:underline"
                  :title="`Go to ${fkMap[x.c.name].table}.${fkMap[x.c.name].column} = ${cellText(r[x.i])}`"
                  @click.stop="followFk(x.c.name, r[x.i])">{{ cellText(r[x.i]) }} ↗</button>
                <span v-else :title="cellText(r[x.i])">{{ cellText(r[x.i]) }}</span>
              </template>
            </td>
          </tr>
          <tr :style="{ height: `${(rows.length - range.end) * ROW_H}px` }" v-if="range.end < rows.length" />
        </tbody>
      </table>
      <div v-if="loading" class="p-2 text-xs text-zinc-500">Loading…</div>
    </div>

    <!-- middle bar (between grid and SQL panel) -->
    <div class="relative h-8 shrink-0 flex items-center gap-2 px-3 border-t border-zinc-800 text-[11px] text-zinc-400">
      <button class="px-2 py-0.5 rounded bg-blue-600 text-white">Data</button>
      <button @click="emit('switchSub', 'structure')"
        class="px-2 py-0.5 rounded text-zinc-400 hover:bg-zinc-800">Structure</button>
      <div class="w-px h-4 bg-zinc-800"></div>
      <button v-if="cs.editable" @click="cs.addDraft(columnNames)"
        class="px-2 py-0.5 rounded border border-zinc-700 hover:bg-zinc-800">+ Row</button>
      <!-- centered row count / selection -->
      <span class="absolute left-1/2 -translate-x-1/2 text-zinc-500">
        <template v-if="selectedRow !== null">row {{ selectedRow + 1 }} selected</template>
        <template v-else>{{ rangeLabel }} rows</template>
      </span>
      <div class="ml-auto flex items-center gap-2">
        <button @click="showFilters = !showFilters"
          class="px-2 py-0.5 rounded border border-zinc-700 hover:bg-zinc-800"
          :class="showFilters || filters.length ? 'text-blue-400' : ''">
          Filters<template v-if="filters.length"> ({{ filters.length }})</template>
        </button>
        <button @click="showColumnsPicker = !showColumnsPicker"
          class="px-2 py-0.5 rounded border border-zinc-700 hover:bg-zinc-800"
          :class="showColumnsPicker || hiddenCols.size ? 'text-blue-400' : ''">Columns</button>
        <ResultActions :columns="columnNames" :rows="rows"
          :selected="selectedRow !== null ? rows[selectedRow] : null" :table="ws.selectedTable?.name"
          rows-scope="page" :server-export="exportCurrent" />
        <button @click="prevPage" :disabled="pageIndex === 0"
          class="px-1 disabled:opacity-30 hover:text-zinc-200">‹</button>
        <button @click="nextPage" :disabled="!hasNextPage"
          class="px-1 disabled:opacity-30 hover:text-zinc-200">›</button>
      </div>
      <div v-if="showColumnsPicker && page" class="absolute bottom-8 right-16 z-30 w-48 max-h-64 overflow-auto rounded border border-zinc-700 bg-zinc-900 p-1 shadow-lg">
        <label v-for="c in page.columns" :key="c.name" class="flex items-center gap-2 px-2 py-0.5 text-[12px] hover:bg-zinc-800 rounded cursor-pointer">
          <input type="checkbox" :checked="!hiddenCols.has(c.name)" @change="toggleCol(c.name)" />
          <span class="truncate">{{ c.name }}</span>
        </label>
      </div>
    </div>

    <!-- pending changes bar (only when there are edits) -->
    <div v-if="cs.count > 0" class="h-8 shrink-0 flex items-center gap-2 px-3 border-t border-amber-900/60 bg-amber-950/20 text-xs">
      <span class="text-amber-300">{{ cs.count }} pending change(s)</span>
      <button @click="preview" class="px-2 py-1 rounded border border-zinc-700 hover:bg-zinc-800">Preview SQL</button>
      <button @click="commit()" class="px-2 py-1 rounded bg-blue-600 hover:bg-blue-500 text-white">Commit</button>
      <button @click="discard" class="px-2 py-1 rounded text-zinc-400 hover:bg-zinc-800">Discard</button>
    </div>

    <!-- permanent SQL-used panel -->
    <ResizeHandle axis="y" invert :min="60" :max="500" v-model="panels.sqlPanelH" />
    <div class="shrink-0 flex flex-col border-t border-zinc-800 bg-zinc-950" :style="{ height: panels.sqlPanelH + 'px' }">
      <div class="flex-1 overflow-auto p-2">
        <pre class="text-[11px] font-mono whitespace-pre-wrap"
          :class="highlight ? 'text-sky-300/90' : 'text-zinc-400'">{{ page?.query ?? "" }}</pre>
      </div>
      <label class="shrink-0 h-6 flex items-center gap-2 px-3 border-t border-zinc-800 text-[11px] text-zinc-500">
        <input type="checkbox" v-model="highlight" /> Enable syntax highlighting
      </label>
    </div>

    <SqlPreviewModal v-if="previewStatements" :statements="previewStatements" @close="previewStatements = null" />
    <ConfirmModal v-if="confirmProd" title="Write on PRODUCTION"
      :message="`Commit ${cs.count} change(s) to a production database?`"
      confirm-label="Commit" danger
      @confirm="commit(true)" @cancel="confirmProd = false" />
   </div>

   <!-- permanent row inspector -->
   <ResizeHandle v-if="page" axis="x" invert :min="200" :max="560" v-model="panels.inspectorW" />
   <RowInspector v-if="page"
     :columns="page.columns" :cell="inspectorCell" :editable="cs.editable"
     :empty="inspectorRow === null" :width="panels.inspectorW" @edit="inspectorEdit" @set-null="inspectorSetNull" />

   <JsonViewer v-if="jsonView" :value="jsonView.value" :title="jsonView.title" @close="jsonView = null" />
  </div>
</template>
