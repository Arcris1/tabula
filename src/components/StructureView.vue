<script setup lang="ts">
import { computed, onBeforeUnmount, reactive, ref, watch } from "vue";
import { useShortcutsStore } from "../stores/shortcuts";
import { api, type ColumnDef, type DdlOp, type TableStructure } from "../lib/api";
import { useConnectionsStore } from "../stores/connections";
import { useSettingsStore } from "../stores/settings";
import { useWorkspaceStore } from "../stores/workspace";
import ConfirmModal from "./ConfirmModal.vue";
import DangerConfirmModal from "./DangerConfirmModal.vue";
import SqlPreviewModal from "./SqlPreviewModal.vue";

const emit = defineEmits<{ switchSub: [sub: "data" | "structure"] }>();
const ws = useWorkspaceStore();
const conns = useConnectionsStore();

const structure = ref<TableStructure | null>(null);
const estRows = ref<number | null>(null);
const showInfo = ref(false);
const colSearch = ref("");
const pkColumns = computed(() => structure.value?.columns.filter((c) => c.isPk).map((c) => c.name) ?? []);
const shownColumns = computed(() =>
  (structure.value?.columns ?? []).filter((c) => c.name.toLowerCase().includes(colSearch.value.toLowerCase())),
);

const createSql = computed(() => {
  const st = structure.value;
  const t = ws.selectedTable;
  if (!st || !t) return "";
  // Quote identifiers in the CONNECTION's dialect so the copied CREATE actually
  // runs there (MySQL backticks, MSSQL brackets, others double-quotes).
  const e = engine.value;
  const q = (n: string) =>
    e === "mysql" ? `\`${n.replace(/`/g, "``")}\`` :
    e === "mssql" ? `[${n.replace(/]/g, "]]")}]` :
    `"${n.replace(/"/g, '""')}"`;
  const cols = st.columns.map((c) => {
    let line = `  ${q(c.name)} ${c.dataType}`;
    if (!c.nullable) line += " NOT NULL";
    if (c.default != null) line += ` DEFAULT ${c.default}`;
    return line;
  });
  const pks = st.columns.filter((c) => c.isPk).map((c) => q(c.name));
  if (pks.length) cols.push(`  PRIMARY KEY (${pks.join(", ")})`);
  const name = t.schema && t.schema !== "public" ? `${q(t.schema)}.${q(t.name)}` : q(t.name);
  let sql = `CREATE TABLE ${name} (\n${cols.join(",\n")}\n);`;
  for (const ix of st.indexes) {
    sql += `\nCREATE ${ix.unique ? "UNIQUE " : ""}INDEX ${q(ix.name)} ON ${name} (${ix.columns.map(q).join(", ")});`;
  }
  return sql;
});
const error = ref<string | null>(null);
const creating = ref(false);
const addingColumn = ref(false);
const addingIndex = ref(false);

const newTable = reactive({ name: "", columns: [{ name: "id", dataType: "INTEGER", nullable: false, isPk: true }] as ColumnDef[] });
const newColumn = reactive<ColumnDef>({ name: "", dataType: "VARCHAR(255)", nullable: true, isPk: false, default: "" });
const newIndex = reactive({ name: "", columns: "", unique: false });

// inline column edits (alter type/nullability, rename) + rename table
const editCol = ref<{ name: string; dataType: string; nullable: boolean } | null>(null);
const renameCol = ref<{ from: string; to: string } | null>(null);
const renamingTable = ref(false);
const renameTbl = ref("");
// SQLite can't ALTER a column in place — hide the affordance there.
const engine = computed(() => conns.connections.find((c) => c.id === ws.activeId)?.engine);
const canAlterColumn = computed(() => engine.value !== "sqlite");

const pendingOp = ref<DdlOp | null>(null);
const previewSql = ref<string | null>(null);
const confirmProd = ref(false);
const dropColConfirm = ref<string | null>(null);

async function refresh() {
  error.value = null;
  structure.value = null;
  if (!ws.activeId || !ws.selectedTable) return;
  try {
    structure.value = await api.tableStructure(ws.activeId, ws.selectedTable);
    const page = await api.fetchRows(ws.activeId, ws.selectedTable, { limit: 1, offset: 0, sort: null, filters: [] });
    estRows.value = page.totalEstimate ?? page.rows.length;
  } catch (e: any) {
    error.value = e?.message ?? String(e);
  }
}
watch(() => ws.selectedTable, () => { creating.value = false; refresh(); }, { immediate: true });

async function stage(op: DdlOp) {
  if (!ws.activeId) return;
  error.value = null;
  try {
    pendingOp.value = op;
    previewSql.value = await api.previewDdl(ws.activeId, op);
  } catch (e: any) {
    error.value = e?.message ?? String(e);
  }
}

async function execute(skipProdCheck = false, skipDropCol = false) {
  if (!ws.activeId || !pendingOp.value) return;
  const op = pendingOp.value;
  // DROP COLUMN is irreversible data loss — gate it behind typing the column name,
  // same as DROP TABLE / TRUNCATE elsewhere (honors the warnDropTruncate rail).
  if (!skipDropCol && op.op === "dropColumn" && useSettingsStore().rails.warnDropTruncate) {
    dropColConfirm.value = (op as any).column;
    return;
  }
  dropColConfirm.value = null;
  const conn = conns.connections.find((c) => c.id === ws.activeId);
  if (conn?.isProduction && useSettingsStore().rails.warnProductionWrites && !skipProdCheck) {
    confirmProd.value = true;
    return;
  }
  confirmProd.value = false;
  try {
    await api.applyDdl(ws.activeId, pendingOp.value);
    const wasCreate = (pendingOp.value as any).op === "createTable";
    previewSql.value = null;
    pendingOp.value = null;
    addingColumn.value = false;
    addingIndex.value = false;
    if (wasCreate) {
      creating.value = false;
      newTable.name = "";
      newTable.columns = [{ name: "id", dataType: "INTEGER", nullable: false, isPk: true }];
    }
    await ws.loadTables();
    await refresh();
  } catch (e: any) {
    previewSql.value = null;
    pendingOp.value = null;
    error.value = e?.message ?? String(e);
  }
}

function stageCreateTable() {
  stage({ op: "createTable", name: newTable.name, columns: newTable.columns.filter((c) => c.name) });
}
function stageAddColumn() {
  if (!ws.selectedTable) return;
  const d = (newColumn.default ?? "").trim();
  stage({ op: "addColumn", table: ws.selectedTable, column: { ...newColumn, default: d || null } });
}
function stageDropColumn(column: string) {
  if (!ws.selectedTable) return;
  stage({ op: "dropColumn", table: ws.selectedTable, column });
}
function stageAlterColumn() {
  if (!ws.selectedTable || !editCol.value) return;
  const e = editCol.value;
  stage({ op: "alterColumn", table: ws.selectedTable, column: e.name, dataType: e.dataType, nullable: e.nullable });
  editCol.value = null;
}
function stageRenameColumn() {
  if (!ws.selectedTable || !renameCol.value || !renameCol.value.to.trim()) return;
  const r = renameCol.value;
  stage({ op: "renameColumn", table: ws.selectedTable, column: r.from, newName: r.to.trim() });
  renameCol.value = null;
}
function stageRenameTable() {
  if (!ws.selectedTable || !renameTbl.value.trim()) return;
  stage({ op: "renameTable", table: ws.selectedTable, newName: renameTbl.value.trim() });
  renamingTable.value = false;
  renameTbl.value = "";
}
function stageAddIndex() {
  if (!ws.selectedTable) return;
  stage({
    op: "addIndex", table: ws.selectedTable, name: newIndex.name,
    columns: newIndex.columns.split(",").map((s) => s.trim()).filter(Boolean),
    unique: newIndex.unique,
  });
}
function stageDropIndex(name: string) {
  if (!ws.selectedTable) return;
  stage({ op: "dropIndex", table: ws.selectedTable, name });
}
function startCreate() {
  creating.value = true;
}
defineExpose({ startCreate });

const unregisterRefresh = useShortcutsStore().register("structure:refresh", () => refresh());
onBeforeUnmount(unregisterRefresh);
</script>

<template>
  <div class="flex-1 min-w-0 flex flex-col">
   <div class="flex-1 min-w-0 flex flex-col overflow-auto p-3 gap-4 text-[12.5px]">
    <div v-if="error" class="px-3 py-2 text-xs text-red-400 border border-red-900 rounded">{{ error }}</div>

    <!-- create table form -->
    <div v-if="creating" class="border border-zinc-800 rounded p-3 flex flex-col gap-2">
      <div class="text-xs font-medium text-zinc-400">New table</div>
      <input v-model="newTable.name" placeholder="table name"
        class="bg-zinc-900 border border-zinc-800 rounded px-2 py-1 w-64 outline-none focus:border-zinc-600" />
      <div v-for="(c, i) in newTable.columns" :key="i" class="flex gap-2 items-center">
        <input v-model="c.name" placeholder="column" class="w-40 bg-zinc-900 border border-zinc-800 rounded px-2 py-1" />
        <input v-model="c.dataType" placeholder="type" class="w-40 bg-zinc-900 border border-zinc-800 rounded px-2 py-1" />
        <label class="text-xs text-zinc-500 flex items-center gap-1"><input type="checkbox" v-model="c.nullable" /> null</label>
        <label class="text-xs text-zinc-500 flex items-center gap-1"><input type="checkbox" v-model="c.isPk" /> pk</label>
        <button class="text-red-500/70" @click="newTable.columns.splice(i, 1)">×</button>
      </div>
      <div class="flex gap-2">
        <button @click="newTable.columns.push({ name: '', dataType: 'VARCHAR(255)', nullable: true, isPk: false })"
          class="px-2 py-1 rounded border border-zinc-700 hover:bg-zinc-800 text-xs">+ column</button>
        <button @click="stageCreateTable" :disabled="!newTable.name"
          class="px-2 py-1 rounded bg-blue-600 hover:bg-blue-500 text-white text-xs disabled:opacity-40">Preview DDL</button>
        <button @click="creating = false" class="px-2 py-1 rounded text-zinc-400 hover:bg-zinc-800 text-xs">Cancel</button>
      </div>
    </div>

    <div v-if="!creating && !structure && !error" class="flex-1 flex items-center justify-center text-zinc-600 text-xs">
      Select a table, or use “+ Table” to create one
    </div>

    <template v-if="!creating && structure">
      <!-- Name / Primary / column search header -->
      <div class="flex items-center gap-3 text-xs">
        <span class="text-zinc-500">Name</span>
        <span class="px-2 py-1 rounded bg-zinc-900 border border-zinc-800 font-mono">{{ ws.selectedTable?.name }}</span>
        <template v-if="!renamingTable">
          <button class="text-zinc-600 hover:text-blue-400"
            @click="renamingTable = true; renameTbl = ws.selectedTable?.name ?? ''">rename</button>
        </template>
        <template v-else>
          <input v-model="renameTbl" class="w-40 bg-zinc-900 border border-zinc-800 rounded px-2 py-1 font-mono" @keydown.enter="stageRenameTable" />
          <button @click="stageRenameTable" :disabled="!renameTbl.trim() || renameTbl.trim() === ws.selectedTable?.name"
            class="px-2 py-0.5 rounded bg-blue-600 hover:bg-blue-500 text-white disabled:opacity-40">Preview DDL</button>
          <button @click="renamingTable = false" class="text-zinc-500 hover:text-zinc-300">cancel</button>
        </template>
        <span class="text-zinc-500">Primary</span>
        <span class="px-2 py-1 rounded bg-blue-950/50 border border-blue-900 font-mono">{{ pkColumns.join(", ") || "—" }}</span>
        <input v-model="colSearch" placeholder="Search for column…"
          class="ml-auto w-56 bg-zinc-900 border border-zinc-800 rounded px-2 py-1 outline-none focus:border-zinc-600" />
      </div>

      <!-- Info: generated CREATE statement + estimate (toggled from the bottom bar) -->
      <div v-if="showInfo">
        <div class="border border-zinc-800 rounded">
          <div class="px-3 py-1 text-[11px] text-zinc-500 border-b border-zinc-800">
            Estimated rows: {{ estRows ?? "—" }}
          </div>
          <pre class="p-3 text-[12px] font-mono text-zinc-300 whitespace-pre-wrap max-h-64 overflow-auto">{{ createSql }}</pre>
        </div>
      </div>

      <!-- columns -->
      <div>
        <div class="flex items-center gap-2 mb-2">
          <span class="text-xs font-medium text-zinc-400">Columns</span>
        </div>
        <div v-if="addingColumn" class="flex gap-2 items-center mb-2">
          <input v-model="newColumn.name" placeholder="name" class="w-40 bg-zinc-900 border border-zinc-800 rounded px-2 py-1" />
          <input v-model="newColumn.dataType" placeholder="type" class="w-40 bg-zinc-900 border border-zinc-800 rounded px-2 py-1" />
          <input v-model="newColumn.default" placeholder="default (optional)" class="w-40 bg-zinc-900 border border-zinc-800 rounded px-2 py-1" />
          <label class="text-xs text-zinc-500 flex items-center gap-1"><input type="checkbox" v-model="newColumn.nullable" /> null</label>
          <button @click="stageAddColumn" :disabled="!newColumn.name"
            class="px-2 py-1 rounded bg-blue-600 hover:bg-blue-500 text-white text-xs disabled:opacity-40">Preview DDL</button>
        </div>
        <table class="border-collapse font-mono text-[12px]">
          <thead>
            <tr class="text-zinc-500">
              <th class="text-left pr-6 pb-1 font-medium">name</th>
              <th class="text-left pr-6 pb-1 font-medium">type</th>
              <th class="text-left pr-6 pb-1 font-medium">nullable</th>
              <th class="text-left pr-6 pb-1 font-medium">computed</th>
              <th class="text-left pr-6 pb-1 font-medium">default</th>
              <th class="text-left pr-6 pb-1 font-medium">foreign key</th>
              <th class="text-left pr-6 pb-1 font-medium">comment</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            <template v-for="c in shownColumns" :key="c.name">
            <tr class="border-t border-zinc-900">
              <td class="pr-6 py-1">
                {{ c.name }}
                <span v-if="c.isPk" class="ml-1 text-[10px] uppercase text-amber-400 border border-amber-900 rounded px-1">pk</span>
              </td>
              <td class="pr-6 py-1 text-zinc-400">{{ c.dataType }}</td>
              <td class="pr-6 py-1 text-zinc-500">{{ c.nullable ? "yes" : "no" }}</td>
              <td class="pr-6 py-1 text-zinc-500">{{ c.isComputed ? "yes" : "" }}</td>
              <td class="pr-6 py-1 text-zinc-500">{{ c.default ?? "" }}</td>
              <td class="pr-6 py-1 text-emerald-400/80">{{ c.foreignKey ?? "" }}</td>
              <td class="pr-6 py-1 text-zinc-500 max-w-64 truncate" :title="c.comment ?? ''">{{ c.comment ?? "" }}</td>
              <td class="py-1 whitespace-nowrap">
                <button v-if="canAlterColumn" class="text-zinc-700 hover:text-blue-400 text-xs mr-2"
                  @click="renameCol = null; editCol = { name: c.name, dataType: c.dataType, nullable: c.nullable }">alter</button>
                <button class="text-zinc-700 hover:text-blue-400 text-xs mr-2"
                  @click="editCol = null; renameCol = { from: c.name, to: c.name }">rename</button>
                <button class="text-zinc-700 hover:text-red-400 text-xs" @click="stageDropColumn(c.name)">drop</button>
              </td>
            </tr>
            <!-- inline alter editor -->
            <tr v-if="editCol && editCol.name === c.name" class="bg-zinc-900/40">
              <td colspan="8" class="py-2 pl-2">
                <div class="flex gap-2 items-center">
                  <span class="text-xs text-zinc-500">alter {{ c.name }}:</span>
                  <input v-model="editCol.dataType" placeholder="type" class="w-44 bg-zinc-900 border border-zinc-800 rounded px-2 py-1" />
                  <label class="text-xs text-zinc-500 flex items-center gap-1"><input type="checkbox" v-model="editCol.nullable" /> null</label>
                  <button @click="stageAlterColumn" :disabled="!editCol.dataType.trim()"
                    class="px-2 py-1 rounded bg-blue-600 hover:bg-blue-500 text-white text-xs disabled:opacity-40">Preview DDL</button>
                  <button @click="editCol = null" class="text-xs text-zinc-500 hover:text-zinc-300">cancel</button>
                </div>
              </td>
            </tr>
            <!-- inline rename editor -->
            <tr v-if="renameCol && renameCol.from === c.name" class="bg-zinc-900/40">
              <td colspan="8" class="py-2 pl-2">
                <div class="flex gap-2 items-center">
                  <span class="text-xs text-zinc-500">rename {{ c.name }} to:</span>
                  <input v-model="renameCol.to" class="w-44 bg-zinc-900 border border-zinc-800 rounded px-2 py-1" @keydown.enter="stageRenameColumn" />
                  <button @click="stageRenameColumn" :disabled="!renameCol.to.trim() || renameCol.to.trim() === c.name"
                    class="px-2 py-1 rounded bg-blue-600 hover:bg-blue-500 text-white text-xs disabled:opacity-40">Preview DDL</button>
                  <button @click="renameCol = null" class="text-xs text-zinc-500 hover:text-zinc-300">cancel</button>
                </div>
              </td>
            </tr>
            </template>
          </tbody>
        </table>
        <div v-if="structure.comment" class="mt-1 text-[11px] text-zinc-500">table comment: {{ structure.comment }}</div>
      </div>

      <!-- indexes -->
      <div>
        <div class="flex items-center gap-2 mb-2">
          <span class="text-xs font-medium text-zinc-400">Indexes</span>
        </div>
        <div v-if="addingIndex" class="flex gap-2 items-center mb-2">
          <input v-model="newIndex.name" placeholder="index name" class="w-40 bg-zinc-900 border border-zinc-800 rounded px-2 py-1" />
          <input v-model="newIndex.columns" placeholder="col1, col2" class="w-48 bg-zinc-900 border border-zinc-800 rounded px-2 py-1" />
          <label class="text-xs text-zinc-500 flex items-center gap-1"><input type="checkbox" v-model="newIndex.unique" /> unique</label>
          <button @click="stageAddIndex" :disabled="!newIndex.name || !newIndex.columns"
            class="px-2 py-1 rounded bg-blue-600 hover:bg-blue-500 text-white text-xs disabled:opacity-40">Preview DDL</button>
        </div>
        <div v-for="i in structure.indexes" :key="i.name"
          class="flex items-center gap-3 border-t border-zinc-900 py-1 font-mono text-[12px]">
          <span>{{ i.name }}</span>
          <span class="text-zinc-500">({{ i.columns.join(", ") }})</span>
          <span v-if="i.unique" class="text-[10px] uppercase text-blue-400 border border-blue-900 rounded px-1">unique</span>
          <button class="ml-auto text-zinc-700 hover:text-red-400 text-xs" @click="stageDropIndex(i.name)">drop</button>
        </div>
        <div v-if="!structure.indexes.length" class="text-zinc-600 text-xs border-t border-zinc-900 pt-2">No secondary indexes.</div>
      </div>

      <!-- triggers -->
      <div>
        <div class="text-xs font-medium text-zinc-400 mb-2">Triggers</div>
        <div v-for="t in structure.triggers" :key="t.name"
          class="flex items-center gap-3 border-t border-zinc-900 py-1 font-mono text-[12px]">
          <span>{{ t.name }}</span>
          <span v-if="t.timing || t.event" class="text-zinc-500">{{ t.timing }} {{ t.event }}</span>
        </div>
        <div v-if="!structure.triggers.length" class="text-zinc-600 text-xs border-t border-zinc-900 pt-2">No triggers.</div>
      </div>
    </template>
   </div>

   <!-- bottom bar: Data / Structure toggle + structure actions -->
   <div class="h-8 shrink-0 flex items-center gap-2 px-3 border-t border-zinc-800 text-[11px] text-zinc-400">
     <button @click="emit('switchSub', 'data')" class="px-2 py-0.5 rounded text-zinc-400 hover:bg-zinc-800">Data</button>
     <button class="px-2 py-0.5 rounded bg-blue-600 text-white">Structure</button>
     <div class="w-px h-4 bg-zinc-800"></div>
     <button @click="addingIndex = !addingIndex" class="px-2 py-0.5 rounded border border-zinc-700 hover:bg-zinc-800">+ Index</button>
     <button @click="addingColumn = !addingColumn" class="px-2 py-0.5 rounded border border-zinc-700 hover:bg-zinc-800">+ Column</button>
     <button @click="showInfo = !showInfo" class="px-2 py-0.5 rounded border border-zinc-700 hover:bg-zinc-800"
       :class="showInfo ? 'text-blue-400' : ''">Info</button>
   </div>

    <SqlPreviewModal v-if="previewSql !== null" :statements="[previewSql]" executable
      @execute="execute()" @close="previewSql = null; pendingOp = null" />
    <ConfirmModal v-if="confirmProd" title="DDL on PRODUCTION"
      :message="previewSql ?? ''" confirm-label="Execute" danger
      @confirm="execute(true)" @cancel="confirmProd = false" />
    <DangerConfirmModal v-if="dropColConfirm"
      :title="`Drop column ${dropColConfirm}`"
      :message="`This permanently removes the column “${dropColConfirm}” and all of its data. This cannot be undone.`"
      :expected="dropColConfirm" confirm-label="Drop column"
      @confirm="execute(false, true)" @cancel="dropColConfirm = null; previewSql = null; pendingOp = null" />
  </div>
</template>
