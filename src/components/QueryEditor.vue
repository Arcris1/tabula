<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, shallowRef, watch } from "vue";
import { EditorView, keymap } from "@codemirror/view";
import { Prec } from "@codemirror/state";
import type { CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { indentWithTab } from "@codemirror/commands";
import { basicSetup } from "codemirror";
import { sql, MSSQL, MySQL, PostgreSQL, SQLite } from "@codemirror/lang-sql";
import { oneDark } from "@codemirror/theme-one-dark";
import { api, type TableInfo } from "../lib/api";
import { statementAt, statements } from "../lib/sqlStatement";
import { createChangeset } from "../lib/changesetCore";
import { parseCellInput } from "../lib/cellInput";
import { checkStatements } from "../lib/railChecks";
import { editableTarget, tablesInStatement } from "../lib/sqlContext";
import { visibleRange } from "../lib/virtual";
import { useConnectionsStore } from "../stores/connections";
import { useQueriesStore, type QueryTab } from "../stores/queries";
import { usePanelsStore } from "../stores/panels";
import { useSettingsStore } from "../stores/settings";
import { useSavedQueriesStore } from "../stores/savedQueries";
import { useToastStore } from "../stores/toast";
import { useShortcutsStore } from "../stores/shortcuts";
import { useWorkspaceStore } from "../stores/workspace";
import { asJson } from "../lib/jsonCell";
import ConfirmModal from "./ConfirmModal.vue";
import DangerConfirmModal from "./DangerConfirmModal.vue";
import JsonViewer from "./JsonViewer.vue";
import PromptModal from "./PromptModal.vue";
import ResizeHandle from "./ResizeHandle.vue";
import ResultActions from "./ResultActions.vue";
import RowInspector from "./RowInspector.vue";
import SqlPreviewModal from "./SqlPreviewModal.vue";

const props = defineProps<{ tab: QueryTab }>();
const queries = useQueriesStore();
const savedQueries = useSavedQueriesStore();
const toast = useToastStore();
const savePromptOpen = ref(false);
const saveInitial = ref("");
const pendingSql = ref("");
const overwriteName = ref<string | null>(null);
function saveCurrent() {
  const v = view.value;
  const sql = (v ? v.state.doc.toString() : props.tab.sql).trim();
  if (!sql) return;
  pendingSql.value = sql;
  saveInitial.value = props.tab.title;
  savePromptOpen.value = true;
}
function doSave(name: string, sql: string) {
  savedQueries.save(name, sql);
  props.tab.title = name; // rename the tab to match the saved query
  toast.success(`Saved “${name}”`);
}
function confirmSave(name: string) {
  savePromptOpen.value = false;
  const sql = pendingSql.value;
  if (!name || !sql) return;
  // overwriting a *different* saved query with the same name → confirm first
  const existing = savedQueries.queries.find((q) => q.name === name);
  if (existing && existing.sql.trim() !== sql.trim()) {
    overwriteName.value = name;
    return;
  }
  doSave(name, sql);
}
function confirmOverwrite() {
  if (overwriteName.value) doSave(overwriteName.value, pendingSql.value);
  overwriteName.value = null;
}
function cancelOverwrite() {
  saveInitial.value = overwriteName.value ?? "";
  overwriteName.value = null;
  savePromptOpen.value = true; // back to the name prompt to pick a different name
}
const ws = useWorkspaceStore();
const conns = useConnectionsStore();
const settings = useSettingsStore();
const panels = usePanelsStore();

const host = ref<HTMLElement | null>(null);
const view = shallowRef<EditorView | null>(null);
const ROW_H = 26;
const scrollTop = ref(0);
const viewportH = ref(400);

/** columns per bare table name (lowercased), for statement-aware completion */
let columnsByTable: Record<string, string[]> = {};

const run = computed(() => props.tab.runs[props.tab.activeRunIndex] ?? null);
const running = computed(() => props.tab.runs.some((r) => r.status === "running"));
const range = computed(() => visibleRange(scrollTop.value, viewportH.value, ROW_H, run.value?.rows.length ?? 0));
const slice = computed(() => run.value?.rows.slice(range.value.start, range.value.end) ?? []);

function dialectFor() {
  const engine = conns.connections.find((c) => c.id === ws.activeId)?.engine;
  return engine === "postgres" ? PostgreSQL : engine === "sqlite" ? SQLite
    : engine === "mssql" ? MSSQL : MySQL;
}

async function loadSchema(): Promise<Record<string, string[]>> {
  if (!ws.activeId) return {};
  try {
    const meta = await api.schemaColumns(ws.activeId);
    const forCm: Record<string, string[]> = {};
    columnsByTable = {};
    for (const t of meta) {
      forCm[t.schema ? `${t.schema}.${t.table}` : t.table] = t.columns;
      columnsByTable[t.table.toLowerCase()] = t.columns;
    }
    return forCm;
  } catch {
    return {};
  }
}

/** Offers columns of the tables referenced by the statement under the cursor. */
function statementColumns(ctx: CompletionContext): CompletionResult | null {
  const word = ctx.matchBefore(/[\w$]+/);
  if (!word && !ctx.explicit) return null;
  // let lang-sql own "table." member completion
  if (ctx.matchBefore(/\.[\w$]*/)) return null;
  const doc = ctx.state.doc.toString();
  const stmt = statementAt(doc, ctx.pos);
  if (!stmt) return null;
  const options = tablesInStatement(stmt.text).flatMap((t) =>
    (columnsByTable[t] ?? []).map((c) => ({
      label: c, type: "property", detail: t, boost: 3,
    })),
  );
  if (!options.length) return null;
  return { from: word ? word.from : ctx.pos, options, validFor: /^[\w$]*$/ };
}

// ---- safety rails: one combined modal per batch ----
const pendingStmts = ref<string[] | null>(null);
const railWarning = ref<{ title: string; message: string; severe: boolean } | null>(null);
// For severe (irreversible / production) batches the confirm requires typing the
// connection name — same "stop and think" gate the structure UI already uses,
// so the raw-SQL path is no longer the cheap one-click way around the rails.
const activeConnName = computed(() => conns.connections.find((c) => c.id === ws.activeId)?.name ?? "");

function guardAndRun(stmts: string[]) {
  const conn = conns.connections.find((c) => c.id === ws.activeId);
  const warning = checkStatements(stmts, settings.rails, !!conn?.isProduction, isMysql() ? "mysql" : "other");
  if (warning) {
    pendingStmts.value = stmts;
    railWarning.value = warning;
    return;
  }
  queries.run(props.tab, stmts);
}
function confirmRail() {
  const stmts = pendingStmts.value!;
  railWarning.value = null;
  pendingStmts.value = null;
  queries.run(props.tab, stmts);
}
function cancelRail() {
  railWarning.value = null;
  pendingStmts.value = null;
}

function isMysql() {
  return conns.connections.find((c) => c.id === ws.activeId)?.engine === "mysql";
}
function splitOpts() {
  const engine = conns.connections.find((c) => c.id === ws.activeId)?.engine;
  const mysql = engine === "mysql";
  const mssql = engine === "mssql";
  // T-SQL uses SSMS batch semantics: GO separates batches, `;` does not —
  // a CREATE PROCEDURE body (which contains semicolons) must ship whole.
  return { backslashEscapes: mysql, hashComments: mysql, goSeparator: mssql, semicolons: !mssql };
}

function runCurrent() {
  const v = view.value;
  if (!v || running.value) return; // a re-run mid-batch would orphan the in-flight query
  const sel = v.state.sliceDoc(v.state.selection.main.from, v.state.selection.main.to);
  if (sel.trim()) {
    const stmts = statements(sel, splitOpts()).map((s) => s.text);
    if (stmts.length) guardAndRun(stmts);
    return;
  }
  const stmt = statementAt(v.state.doc.toString(), v.state.selection.main.head, splitOpts())?.text;
  if (stmt) guardAndRun([stmt]);
}

// Run every statement in the editor (or the selection, if any) — an explicit
// alternative to select-all-then-run, and to ⌘↵'s run-statement-under-cursor.
function runAll() {
  const v = view.value;
  if (!v || running.value) return;
  const sel = v.state.sliceDoc(v.state.selection.main.from, v.state.selection.main.to);
  const src = sel.trim() ? sel : v.state.doc.toString();
  const stmts = statements(src, splitOpts()).map((s) => s.text).filter((s) => s.trim());
  if (stmts.length) guardAndRun(stmts);
}

onMounted(async () => {
  const schema = await loadSchema();
  const lang = sql({ dialect: dialectFor(), schema, upperCaseKeywords: true });
  view.value = new EditorView({
    parent: host.value!,
    doc: props.tab.sql,
    extensions: [
      basicSetup,
      oneDark,
      lang,
      lang.language.data.of({ autocomplete: statementColumns }),
      Prec.highest(keymap.of([{ key: "Mod-Enter", run: () => { runCurrent(); return true; } }])),
      keymap.of([indentWithTab]), // Tab indents in the editor instead of moving focus
      EditorView.updateListener.of((u) => {
        if (u.docChanged) props.tab.sql = u.state.doc.toString();
      }),
      EditorView.theme({ "&": { fontSize: "13px" }, ".cm-content": { fontFamily: "ui-monospace, monospace" } }),
    ],
  });
  view.value.focus(); // type immediately without clicking into the editor
});
onBeforeUnmount(() => view.value?.destroy());

// Apply external changes to tab.sql (e.g. loading a saved query) into the editor.
// If the view isn't built yet (schema still loading), onMounted seeds it from
// tab.sql, so this only needs to handle an already-mounted editor.
watch(() => props.tab.sql, (newSql) => {
  const v = view.value;
  if (!v) return;
  if (newSql !== v.state.doc.toString()) {
    v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: newSql } });
  }
});

function focus() {
  view.value?.focus();
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
// ---- editable results: single-table SELECT + pk present in columns ----
const cs = createChangeset();
const editTable = ref<TableInfo | null>(null);
const readOnlyReason = ref<string | null>(null);
const editingCell = ref<{ rowIndex: number; column: string } | null>(null);
const editValue = ref("");
const editInput = ref<HTMLInputElement | null>(null);
const previewStatements = ref<string[] | null>(null);
const confirmProd = ref(false);
const commitError = ref<string | null>(null);

const colNames = computed(() => run.value?.columns.map((c) => c.name) ?? []);

// row inspector on query results (permanent panel)
const selectedRow = ref<number | null>(null);
const jsonView = ref<{ value: unknown; title: string } | null>(null);
function openJson(value: unknown, title: string) {
  const parsed = asJson(value);
  if (parsed !== null) jsonView.value = { value: parsed, title };
}
const inspectorRow = computed(() =>
  selectedRow.value !== null ? run.value?.rows[selectedRow.value] ?? null : null,
);
function inspectorCell(column: string) {
  return cs.cellValue(inspectorRow.value!, colNames.value, column);
}
function inspectorEdit(column: string, textVal: string) {
  const row = inspectorRow.value;
  if (!row) return;
  const original = row[colNames.value.indexOf(column)];
  cs.editCell(row, colNames.value, column, parseCellInput(textVal, original));
}
function inspectorSetNull(column: string) {
  const row = inspectorRow.value;
  if (!row) return;
  cs.editCell(row, colNames.value, column, null);
}
// No inline editing on a truncated result: the buffer is an incomplete slice, so
// row identity / "affects 1 row" guarantees the changeset relies on can't hold.
const canEdit = computed(() => !!editTable.value && !run.value?.truncated);

watch([() => props.tab.activeRunIndex, () => run.value?.status, () => run.value?.queryId], async () => {
  cs.setContext([]);
  editTable.value = null;
  readOnlyReason.value = null;
  editingCell.value = null;
  selectedRow.value = null;
  commitError.value = null;
  const r = run.value;
  if (!r || r.status !== "done" || !r.columns.length || !ws.activeId) return;
  const target = editableTarget(r.sql, { hashComments: isMysql() });
  if (!target) {
    readOnlyReason.value = "read-only: not a simple single-table SELECT";
    return;
  }
  const engine = conns.connections.find((c) => c.id === ws.activeId)?.engine;
  // postgres folds unquoted identifiers to lowercase
  const fold = (name: string, quoted: boolean) =>
    engine === "postgres" && !quoted ? name.toLowerCase() : name;
  const tinfo: TableInfo = {
    schema: target.schema ? fold(target.schema, target.schemaQuoted) : null,
    name: fold(target.table, target.tableQuoted),
    kind: "table",
  };
  let pk: string[] = [];
  try {
    pk = await api.tablePrimaryKey(ws.activeId, tinfo);
  } catch {
    /* not resolvable -> read-only below */
  }
  if (!pk.length) {
    readOnlyReason.value = `read-only: ${tinfo.name} has no primary key`;
    return;
  }
  const missing = pk.filter((p) => !r.columns.some((c) => c.name === p));
  if (missing.length) {
    readOnlyReason.value = `read-only: include ${missing.join(", ")} in the SELECT to edit`;
    return;
  }
  editTable.value = tinfo;
  cs.setContext(pk);
});

function beginCellEdit(rowIndex: number, column: string) {
  if (!canEdit.value || !run.value) return;
  const row = run.value.rows[rowIndex];
  const { value } = cs.cellValue(row, colNames.value, column);
  editValue.value = value === null ? "NULL" : cellText(value);
  editingCell.value = { rowIndex, column };
  nextTick(() => editInput.value?.focus());
}
function commitCellEdit() {
  if (!editingCell.value || !run.value) return;
  const { rowIndex, column } = editingCell.value;
  const row = run.value.rows[rowIndex];
  const original = row[colNames.value.indexOf(column)];
  cs.editCell(row, colNames.value, column, parseCellInput(editValue.value, original));
  editingCell.value = null;
}
async function previewCs() {
  if (!ws.activeId || !editTable.value) return;
  previewStatements.value = await api.previewChangeset(ws.activeId, editTable.value, cs.toChangeset());
}
async function commitCs(skipProdCheck = false) {
  if (!ws.activeId || !editTable.value || !run.value) return;
  const conn = conns.connections.find((c) => c.id === ws.activeId);
  if (conn?.isProduction && settings.rails.warnProductionWrites && !skipProdCheck) {
    confirmProd.value = true;
    return;
  }
  confirmProd.value = false;
  commitError.value = null;
  try {
    await api.applyChangeset(ws.activeId, editTable.value, cs.toChangeset());
    const sql = run.value.sql;
    cs.clear();
    queries.run(props.tab, [sql]); // refresh with committed data
  } catch (e: any) {
    commitError.value = e?.message ?? String(e);
  }
}

function resultLabel(i: number): string {
  const r = props.tab.runs[i];
  if (r.status === "error") return `Result ${i + 1} ✕`;
  if (r.status === "running") return `Result ${i + 1} …`;
  if (r.columns.length) return `Result ${i + 1} (${r.rowCount ?? r.rows.length} rows)`;
  return `Result ${i + 1} (OK)`;
}
function setSql(text: string) {
  const v = view.value;
  if (!v) return;
  v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: text } });
}
defineExpose({ setSql, focus });

const shortcuts = useShortcutsStore();
const unregisters = [
  shortcuts.register(`query:save:${props.tab.id}`, () => saveCurrent()),
  shortcuts.register(`query:cancel:${props.tab.id}`, () => queries.cancel(props.tab)),
  shortcuts.register(`query:rerun:${props.tab.id}`, () => {
    if (running.value) return;
    const last = props.tab.runs.map((r) => r.sql);
    if (last.length) guardAndRun(last);
    else runCurrent();
  }),
];
onBeforeUnmount(() => unregisters.forEach((u) => u()));
</script>

<template>
  <div class="flex-1 min-w-0 flex flex-col">
    <div class="h-8 shrink-0 flex items-center gap-2 px-2 border-b border-zinc-800">
      <button @click="runCurrent" :disabled="running"
        class="text-xs px-2 py-1 rounded bg-blue-600 hover:bg-blue-500 text-white disabled:opacity-40 disabled:cursor-not-allowed">Run ⌘↵</button>
      <button @click="runAll" :disabled="running" title="Run every statement in the editor"
        class="text-xs px-2 py-1 rounded border border-zinc-700 hover:bg-zinc-800 disabled:opacity-40 disabled:cursor-not-allowed">Run All</button>
      <button @click="saveCurrent" title="Save this query"
        class="text-xs px-2 py-1 rounded border border-zinc-700 hover:bg-zinc-800">★ Save</button>
      <button v-if="running" @click="queries.cancel(tab)"
        class="text-xs px-2 py-1 rounded border border-red-800 text-red-400 hover:bg-red-950">Cancel</button>
      <span v-if="running" class="text-xs text-zinc-500 animate-pulse">running…</span>
    </div>

    <div ref="host" class="shrink-0 overflow-auto border-b border-zinc-800" :style="{ height: panels.editorH + 'px' }" />
    <ResizeHandle axis="y" :min="80" :max="600" v-model="panels.editorH" />

    <!-- result set picker -->
    <nav v-if="tab.runs.length > 1" class="h-7 shrink-0 flex items-center gap-1 px-2 border-b border-zinc-800 text-[11px]">
      <button v-for="(r, i) in tab.runs" :key="r.queryId" @click="tab.activeRunIndex = i"
        class="px-2 py-0.5 rounded"
        :class="[
          tab.activeRunIndex === i ? 'bg-zinc-800 text-white' : 'text-zinc-500 hover:bg-zinc-900',
          r.status === 'error' ? 'text-red-400' : '',
        ]">
        {{ resultLabel(i) }}
      </button>
    </nav>

    <div v-if="run?.status === 'error'" class="m-2 px-3 py-2 text-xs text-red-400 border border-red-900 rounded">
      {{ run.error }}
    </div>

    <div class="flex-1 flex min-h-0">
    <div class="flex-1 overflow-auto font-mono text-[12px]" @scroll="onScroll">
      <pre v-if="run && run.columns.length && run.messages.length"
        class="px-3 py-2 font-mono text-[11.5px] text-zinc-400 whitespace-pre-wrap border-b border-zinc-800">{{ run.messages.join("\n") }}</pre>
      <div v-if="run && run.truncated"
        class="px-3 py-1.5 text-[11.5px] text-amber-300 bg-amber-950/40 border-b border-amber-900/60 sticky top-0 z-20">
        Showing the first {{ run.rows.length.toLocaleString() }} rows. The result is larger —
        editing is disabled here; add a <span class="font-mono">LIMIT</span>/<span class="font-mono">WHERE</span>
        or use Export to get the full set.
      </div>
      <table v-if="run && run.columns.length" class="border-collapse min-w-full">
        <thead class="sticky top-0 bg-zinc-950 z-10">
          <tr>
            <th v-if="canEdit" class="w-6 border-b border-zinc-800"></th>
            <th v-for="c in run.columns" :key="c.name"
              class="text-left px-2 py-1 border-b border-zinc-800 font-medium text-zinc-400 whitespace-nowrap">
              {{ c.name }}
            </th>
          </tr>
        </thead>
        <tbody>
          <!-- insert drafts -->
          <tr v-for="(d, di) in cs.drafts.value" :key="'draft' + di" class="bg-green-950/30">
            <td class="text-center border-b border-zinc-900">
              <button class="text-red-500/70 hover:text-red-400" @click="cs.removeDraft(di)">×</button>
            </td>
            <td v-for="c in run.columns" :key="c.name" class="border-b border-zinc-900 px-1">
              <input v-model="(d[c.name] as any)" :placeholder="c.name"
                class="w-full bg-transparent border border-zinc-800 rounded px-1 py-0.5 text-[11px] outline-none focus:border-green-700" />
            </td>
          </tr>
          <tr :style="{ height: `${range.start * ROW_H}px` }" v-if="range.start > 0" />
          <tr v-for="(r, i) in slice" :key="range.start + i"
            class="hover:bg-zinc-900 cursor-default"
            :class="[
              canEdit && cs.isDeleted(r, colNames) ? 'line-through opacity-50 bg-red-950/30' : '',
              selectedRow === range.start + i ? 'bg-blue-950/40' : '',
            ]"
            :style="{ height: `${ROW_H}px` }"
            @click="selectedRow = range.start + i">
            <td v-if="canEdit" class="text-center border-b border-zinc-900">
              <button class="text-zinc-700 hover:text-red-400"
                :class="cs.isDeleted(r, colNames) ? 'text-red-400' : ''"
                @click="cs.markDelete(r, colNames)">×</button>
            </td>
            <td v-for="(v, j) in r" :key="j"
              class="px-2 border-b border-zinc-900 whitespace-nowrap max-w-96 overflow-hidden text-ellipsis"
              @dblclick="beginCellEdit(range.start + i, run.columns[j].name)">
              <input v-if="editingCell && editingCell.rowIndex === range.start + i && editingCell.column === run.columns[j].name"
                ref="editInput" v-model="editValue"
                @keydown.enter="commitCellEdit" @keydown.esc="editingCell = null" @blur="commitCellEdit"
                class="w-full bg-zinc-900 border border-amber-700 rounded px-1 py-0.5 text-[11px] outline-none" />
              <template v-else>
                <span v-if="canEdit && cs.cellValue(r, colNames, run.columns[j].name).dirty" class="text-amber-300">
                  {{ cs.cellValue(r, colNames, run.columns[j].name).value === null ? "NULL" : cellText(cs.cellValue(r, colNames, run.columns[j].name).value) }}
                </span>
                <span v-else-if="v === null" class="inline-block rounded px-1 text-[10px] font-medium tracking-wide text-zinc-400 bg-zinc-800/80" title="NULL">NULL</span>
                <span v-else-if="v === ''" class="inline-block rounded px-1 text-[10px] text-zinc-400 bg-zinc-800/80" title="empty string">EMPTY</span>
                <template v-else-if="asJson(v) !== null">
                  <button class="mr-1 text-[9px] px-1 rounded bg-zinc-800 text-sky-300 hover:bg-zinc-700"
                    @click.stop="openJson(v, run.columns[j].name)" title="View JSON">{}</button>{{ cellText(v) }}
                </template>
                <span v-else :title="cellText(v)">{{ cellText(v) }}</span>
              </template>
            </td>
          </tr>
          <tr :style="{ height: `${((run?.rows.length ?? 0) - range.end) * ROW_H}px` }"
            v-if="run && range.end < run.rows.length" />
        </tbody>
      </table>
      <div v-else-if="run?.status === 'done'" class="p-3 text-xs text-zinc-500">
        <pre v-if="run.messages.length"
          class="mb-2 font-mono text-[11.5px] text-zinc-300 whitespace-pre-wrap">{{ run.messages.join("\n") }}</pre>
        OK — {{ run.affectedRows }} row(s) affected.
      </div>
      <div v-else-if="!run" class="h-full flex items-center justify-center text-zinc-600 text-xs">
        ⌘↵ to run the statement under the cursor — select several to run them all
      </div>
    </div>

    <ResizeHandle v-if="run && run.columns.length" axis="x" invert :min="200" :max="560"
      v-model="panels.inspectorW" />
    <RowInspector v-if="run && run.columns.length"
      :columns="run.columns" :cell="inspectorCell" :editable="canEdit"
      :empty="inspectorRow === null" :width="panels.inspectorW" @edit="inspectorEdit" @set-null="inspectorSetNull" />
    </div>

    <div v-if="commitError" class="mx-2 mb-1 px-3 py-2 text-xs text-amber-400 border border-amber-900 rounded">
      {{ commitError }} — nothing was applied; your pending changes are kept.
    </div>
    <div v-if="canEdit && cs.count.value > 0"
      class="h-9 shrink-0 flex items-center gap-2 px-3 border-t border-amber-900/60 bg-amber-950/20 text-xs">
      <span class="text-amber-300">{{ cs.count.value }} pending change(s)</span>
      <button @click="previewCs" class="px-2 py-1 rounded border border-zinc-700 hover:bg-zinc-800">Preview SQL</button>
      <button @click="commitCs()" class="px-2 py-1 rounded bg-blue-600 hover:bg-blue-500 text-white">Commit</button>
      <button @click="cs.clear()" class="px-2 py-1 rounded text-zinc-400 hover:bg-zinc-800">Discard</button>
    </div>

    <footer class="h-7 shrink-0 flex items-center gap-3 px-3 border-t border-zinc-800 text-[11px] text-zinc-500">
      <template v-if="run?.status === 'done'">
        <span v-if="selectedRow !== null">row {{ selectedRow + 1 }} selected</span>
        <span v-else>{{ run.rowCount }} rows</span>
        <span>{{ run.elapsedMs }} ms</span>
        <span v-if="canEdit" class="text-emerald-400">editable</span>
        <span v-else-if="readOnlyReason" class="text-zinc-600">{{ readOnlyReason }}</span>
        <ResultActions class="ml-auto" :columns="colNames" :rows="run.rows"
          :selected="selectedRow !== null ? run.rows[selectedRow] : null" :table="editTable?.name" />
        <button v-if="canEdit" @click="cs.addDraft(colNames)" class="text-blue-400 hover:text-blue-300">+ Row</button>
      </template>
      <span v-else-if="run?.status === 'running'">{{ run.rows.length }} rows so far…</span>
    </footer>

    <JsonViewer v-if="jsonView" :value="jsonView.value" :title="jsonView.title" @close="jsonView = null" />
    <SqlPreviewModal v-if="previewStatements" :statements="previewStatements" @close="previewStatements = null" />
    <ConfirmModal v-if="confirmProd" title="Write on PRODUCTION"
      :message="`Commit ${cs.count.value} change(s) to a production database?`"
      confirm-label="Commit" danger
      @confirm="commitCs(true)" @cancel="confirmProd = false" />

    <!-- Irreversible / production batches: demand the connection name be typed. -->
    <DangerConfirmModal v-if="railWarning && railWarning.severe"
      :title="railWarning.title"
      :message="`${railWarning.message}\n\nThis runs against “${activeConnName}”.`"
      :expected="activeConnName" confirm-label="Run anyway"
      @confirm="confirmRail" @cancel="cancelRail" />
    <!-- Milder warnings (e.g. no-WHERE on a non-prod box): one-click acknowledge. -->
    <ConfirmModal v-else-if="railWarning" :title="railWarning.title" :message="railWarning.message"
      confirm-label="Run anyway" danger @confirm="confirmRail" @cancel="cancelRail" />

    <PromptModal v-if="savePromptOpen" title="Save query" label="Name"
      :initial="saveInitial" placeholder="e.g. active users last 24h" confirm-label="Save"
      @submit="confirmSave" @cancel="savePromptOpen = false" />
    <ConfirmModal v-if="overwriteName" title="Overwrite saved query"
      :message="`A saved query named “${overwriteName}” already exists. Overwrite it with the current query?`"
      confirm-label="Overwrite" @confirm="confirmOverwrite" @cancel="cancelOverwrite" />
  </div>
</template>
