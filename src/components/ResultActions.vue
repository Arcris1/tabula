<script setup lang="ts">
import { ref } from "vue";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/api";
import { rowsToTsv, rowsToInsert, rowsToCsv, rowsToJson } from "../lib/copyRows";

const props = defineProps<{
  columns: string[];
  rows: unknown[][];
  selected: unknown[] | null;
  table?: string;
  /** When provided, "Export" streams the FULL server-side result (current
   *  filters+sort) to `path` instead of serializing the in-memory rows. Returns
   *  the number of rows written. Used by the data grid where `rows` is only the
   *  current page. */
  serverExport?: (format: "csv" | "json", path: string) => Promise<number>;
  /** Honest label for the in-memory copy actions: "page" when `rows` is a
   *  single page, "all" when it's the complete result (e.g. a query result). */
  rowsScope?: "page" | "all";
}>();

const open = ref(false);
const note = ref<string | null>(null);
const busy = ref(false);
const scopeWord = () => (props.rowsScope === "page" ? "page" : "all rows");

function copy(text: string, what: string) {
  navigator.clipboard?.writeText(text);
  note.value = `Copied ${what}`;
  open.value = false;
  setTimeout(() => (note.value = null), 1500);
}
async function exportFile(format: "csv" | "json") {
  open.value = false;
  const name = (props.table ?? "result") + "." + format;
  const path = await saveDialog({ defaultPath: name, filters: [{ name: format.toUpperCase(), extensions: [format] }] });
  if (!path) return;
  try {
    if (props.serverExport) {
      busy.value = true;
      note.value = "Exporting…";
      const n = await props.serverExport(format, path);
      note.value = `Exported ${n.toLocaleString()} rows`;
    } else {
      const text = format === "csv" ? rowsToCsv(props.columns, props.rows) : rowsToJson(props.columns, props.rows);
      await api.writeTextFile(path, text);
      note.value = `Exported ${props.rows.length.toLocaleString()} rows`;
    }
  } catch (e: any) {
    note.value = `Export failed: ${e?.message ?? e}`;
  } finally {
    busy.value = false;
    setTimeout(() => (note.value = null), 2500);
  }
}
const tbl = () => props.table ?? "table";
</script>

<template>
  <div class="relative inline-block">
    <button @click="open = !open" class="px-2 py-0.5 rounded border border-zinc-700 hover:bg-zinc-800">Copy / Export ▾</button>
    <span v-if="note" class="ml-2 text-green-500">{{ note }}</span>
    <div v-if="open" class="absolute bottom-7 right-0 z-40 w-52 rounded border border-zinc-700 bg-zinc-900 py-1 shadow-lg text-[12px]"
      @mouseleave="open = false">
      <template v-if="selected">
        <button class="w-full text-left px-3 py-1 hover:bg-zinc-800" @click="copy(rowsToTsv(columns, [selected]), 'row (TSV)')">Copy row (TSV)</button>
        <button class="w-full text-left px-3 py-1 hover:bg-zinc-800" @click="copy(rowsToInsert(tbl(), columns, [selected]), 'row as INSERT')">Copy row as INSERT</button>
        <div class="my-1 border-t border-zinc-800"></div>
      </template>
      <button class="w-full text-left px-3 py-1 hover:bg-zinc-800" @click="copy(rowsToTsv(columns, rows), `${scopeWord()} (TSV)`)">Copy {{ scopeWord() }} (TSV)</button>
      <button class="w-full text-left px-3 py-1 hover:bg-zinc-800" @click="copy(rowsToInsert(tbl(), columns, rows), `${scopeWord()} as INSERT`)">Copy {{ scopeWord() }} as INSERT</button>
      <div class="my-1 border-t border-zinc-800"></div>
      <button class="w-full text-left px-3 py-1 hover:bg-zinc-800" :disabled="busy" @click="exportFile('csv')">
        Export CSV{{ serverExport ? " (full result)" : "" }}…
      </button>
      <button class="w-full text-left px-3 py-1 hover:bg-zinc-800" :disabled="busy" @click="exportFile('json')">
        Export JSON{{ serverExport ? " (full result)" : "" }}…
      </button>
    </div>
  </div>
</template>
