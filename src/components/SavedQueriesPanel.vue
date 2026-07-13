<script setup lang="ts">
import { useSavedQueriesStore } from "../stores/savedQueries";

defineProps<{ width?: number }>();
const emit = defineEmits<{ pick: [sql: string, name?: string]; close: [] }>();
const store = useSavedQueriesStore();
</script>

<template>
  <aside class="shrink-0 border-l border-zinc-800 flex flex-col" :style="{ width: (width ?? 288) + 'px' }">
    <div class="px-3 py-2 border-b border-zinc-800 text-xs font-medium text-zinc-400 flex items-center">
      Saved queries
      <button @click="emit('close')" class="ml-auto text-zinc-600 hover:text-zinc-300">×</button>
    </div>
    <ul class="flex-1 overflow-y-auto">
      <li v-for="q in store.queries" :key="q.id"
        class="group px-3 py-2 border-b border-zinc-900 cursor-pointer hover:bg-zinc-900"
        @click="emit('pick', q.sql, q.name)">
        <div class="flex items-center gap-2">
          <span class="text-[12.5px] truncate">{{ q.name }}</span>
          <button class="ml-auto text-red-500/60 hover:text-red-400 opacity-0 group-hover:opacity-100 text-xs"
            @click.stop="store.remove(q.id)">×</button>
        </div>
        <div class="font-mono text-[10.5px] text-zinc-600 truncate">{{ q.sql }}</div>
      </li>
      <li v-if="!store.queries.length" class="p-3 text-xs text-zinc-600">
        No saved queries. Save one from a query tab with the ★ button.
      </li>
    </ul>
  </aside>
</template>
