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
}>();

const open = ref(false);
const note = ref<string | null>(null);

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
  const text = format === "csv" ? rowsToCsv(props.columns, props.rows) : rowsToJson(props.columns, props.rows);
  await api.writeTextFile(path, text);
  note.value = `Exported ${props.rows.length} rows`;
  setTimeout(() => (note.value = null), 1500);
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
      <button class="w-full text-left px-3 py-1 hover:bg-zinc-800" @click="copy(rowsToTsv(columns, rows), 'all rows (TSV)')">Copy all (TSV)</button>
      <button class="w-full text-left px-3 py-1 hover:bg-zinc-800" @click="copy(rowsToInsert(tbl(), columns, rows), 'all rows as INSERT')">Copy all as INSERT</button>
      <div class="my-1 border-t border-zinc-800"></div>
      <button class="w-full text-left px-3 py-1 hover:bg-zinc-800" @click="exportFile('csv')">Export CSV…</button>
      <button class="w-full text-left px-3 py-1 hover:bg-zinc-800" @click="exportFile('json')">Export JSON…</button>
    </div>
  </div>
</template>
