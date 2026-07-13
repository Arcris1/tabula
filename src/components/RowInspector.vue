<script setup lang="ts">
import { computed, ref } from "vue";
import type { ColumnMeta } from "../lib/api";

const search = ref("");

/** A single selected row shown as an editable, type-labelled form (TablePlus-style). */
const props = defineProps<{
  columns: ColumnMeta[];
  /** effective value + dirty flag per column, from the owning changeset */
  cell: (column: string) => { value: unknown; dirty: boolean };
  editable: boolean;
  /** true when no row is selected — shows an empty state */
  empty: boolean;
  width?: number;
}>();
const emit = defineEmits<{ edit: [column: string, text: string] }>();

function text(v: unknown): string {
  if (v === null || v === undefined) return "";
  return typeof v === "object" ? JSON.stringify(v) : String(v);
}

const fields = computed(() =>
  props.columns
    .filter((c) => c.name.toLowerCase().includes(search.value.toLowerCase()))
    .map((c) => {
      const { value, dirty } = props.cell(c.name);
      return { name: c.name, type: c.dataType, value, dirty, isNull: value === null };
    }),
);
</script>

<template>
  <aside class="shrink-0 border-l border-zinc-800 flex flex-col" :style="{ width: (width ?? 288) + 'px' }">
    <input v-model="search" placeholder="Search for field…"
      class="m-2 bg-zinc-900 border border-zinc-800 rounded px-2 py-1 text-[11px] outline-none focus:border-zinc-600" />
    <div v-if="empty" class="flex-1 flex items-center justify-center text-zinc-600 text-xs">No row selected</div>
    <div v-else class="flex-1 overflow-y-auto px-3 py-2">
      <div v-for="f in fields" :key="f.name" class="mb-2">
        <div class="flex items-baseline justify-between">
          <label class="text-[11px] text-zinc-400">{{ f.name }}</label>
          <span class="text-[10px] text-zinc-600">{{ f.type }}</span>
        </div>
        <input
          :value="f.isNull ? 'NULL' : text(f.value)"
          :disabled="!editable"
          @change="emit('edit', f.name, ($event.target as HTMLInputElement).value)"
          @keydown.enter="emit('edit', f.name, ($event.target as HTMLInputElement).value)"
          class="w-full mt-0.5 bg-zinc-900 border rounded px-1.5 py-1 text-[12px] font-mono outline-none focus:border-zinc-500 disabled:opacity-60"
          :class="[
            f.dirty ? 'border-amber-700 text-amber-300' : 'border-zinc-800',
            f.isNull && !f.dirty ? 'text-zinc-600 italic' : '',
          ]" />
      </div>
      <div v-if="!editable" class="text-[10px] text-zinc-600 mt-1">
        read-only (no primary key)
      </div>
    </div>
  </aside>
</template>
