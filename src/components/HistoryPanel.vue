<script setup lang="ts">
import { onMounted, ref } from "vue";
import { api, type HistoryEntry } from "../lib/api";
import { useWorkspaceStore } from "../stores/workspace";

defineProps<{ width?: number }>();
const emit = defineEmits<{ pick: [sql: string] }>();
const ws = useWorkspaceStore();
const entries = ref<HistoryEntry[]>([]);

onMounted(async () => {
  if (ws.activeId) entries.value = await api.listHistory(ws.activeId);
});
function fmt(ms: number) {
  return new Date(ms).toLocaleString();
}
</script>

<template>
  <aside class="shrink-0 border-l border-zinc-800 flex flex-col" :style="{ width: (width ?? 288) + 'px' }">
    <div class="px-3 py-2 border-b border-zinc-800 text-xs font-medium text-zinc-400">Query history</div>
    <ul class="flex-1 overflow-y-auto">
      <li v-for="(e, i) in entries" :key="i"
        class="px-3 py-2 border-b border-zinc-900 cursor-pointer hover:bg-zinc-900"
        @click="emit('pick', e.sql)">
        <div class="font-mono text-[11px] truncate" :class="e.status === 'error' ? 'text-red-400' : ''">{{ e.sql }}</div>
        <div class="text-[10px] text-zinc-600">
          {{ fmt(e.startedAtMs) }} · {{ e.durationMs }}ms
          <template v-if="e.rowCount != null"> · {{ e.rowCount }} rows</template>
        </div>
      </li>
      <li v-if="!entries.length" class="p-3 text-xs text-zinc-600">No history yet.</li>
    </ul>
  </aside>
</template>
