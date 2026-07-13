<script setup lang="ts">
import { onBeforeUnmount, onMounted } from "vue";
import { useSettingsStore } from "../stores/settings";

const emit = defineEmits<{ close: [] }>();
const settings = useSettingsStore();

const OPTIONS = [
  {
    key: "warnNoWhere" as const,
    label: "Warn on UPDATE / DELETE without WHERE",
    detail: "The statement would affect every row in the table.",
  },
  {
    key: "warnDropTruncate" as const,
    label: "Warn on DROP / TRUNCATE",
    detail: "These cannot be undone, on any connection.",
  },
  {
    key: "warnProductionWrites" as const,
    label: "Confirm writes on production connections",
    detail: "Applies to queries, grid commits and schema changes on red-flagged connections.",
  },
];

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") emit("close");
}
onMounted(() => window.addEventListener("keydown", onKey));
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60" @click.self="emit('close')">
    <div class="w-[26rem] rounded border border-zinc-700 bg-zinc-900 p-4">
      <div class="text-sm font-medium mb-3">Settings</div>
      <div class="text-[11px] uppercase tracking-wide text-zinc-600 mb-2">Safety warnings</div>
      <label v-for="o in OPTIONS" :key="o.key" class="flex items-start gap-3 py-2 border-t border-zinc-800/60 cursor-pointer">
        <input type="checkbox" v-model="settings.rails[o.key]" class="mt-0.5" />
        <span>
          <span class="block text-xs text-zinc-200">{{ o.label }}</span>
          <span class="block text-[11px] text-zinc-500">{{ o.detail }}</span>
        </span>
      </label>
      <div class="mt-4 pt-3 border-t border-zinc-800/60 text-[11px] text-zinc-600 flex items-center justify-between">
        <span>Tabula</span>
        <span>Created by Arcris Silang</span>
      </div>
      <div class="flex justify-end mt-3">
        <button @click="emit('close')" class="px-3 py-1.5 rounded text-xs text-zinc-400 hover:bg-zinc-800">Close</button>
      </div>
    </div>
  </div>
</template>
