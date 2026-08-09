<script setup lang="ts">
import { computed, nextTick, ref } from "vue";
import { useSavedQueriesStore } from "../stores/savedQueries";

defineProps<{ width?: number }>();
const emit = defineEmits<{ pick: [sql: string, name?: string]; close: [] }>();
const store = useSavedQueriesStore();

const filter = ref("");
const filtered = computed(() => {
  const f = filter.value.trim().toLowerCase();
  if (!f) return store.queries;
  return store.queries.filter((q) => q.name.toLowerCase().includes(f) || q.sql.toLowerCase().includes(f));
});

// inline rename
const editingId = ref<string | null>(null);
const editName = ref("");
const renameInput = ref<HTMLInputElement | null>(null);
function beginRename(id: string, current: string) {
  editingId.value = id;
  editName.value = current;
  nextTick(() => renameInput.value?.focus());
}
function commitRename() {
  if (editingId.value && editName.value.trim()) store.rename(editingId.value, editName.value.trim());
  editingId.value = null;
}
</script>

<template>
  <aside class="shrink-0 border-l border-zinc-800 flex flex-col" :style="{ width: (width ?? 288) + 'px' }">
    <div class="px-3 py-2 border-b border-zinc-800 text-xs font-medium text-zinc-400 flex items-center">
      Saved queries
      <button @click="emit('close')" class="ml-auto text-zinc-600 hover:text-zinc-300">×</button>
    </div>
    <div v-if="store.queries.length" class="px-2 py-1.5 border-b border-zinc-900">
      <input v-model="filter" placeholder="Search saved queries…"
        class="w-full bg-zinc-900 border border-zinc-800 rounded px-2 py-1 text-[11px] outline-none focus:border-zinc-600" />
    </div>
    <ul class="flex-1 overflow-y-auto">
      <li v-for="q in filtered" :key="q.id"
        class="group px-3 py-2 border-b border-zinc-900 hover:bg-zinc-900"
        :class="editingId === q.id ? '' : 'cursor-pointer'"
        @click="editingId === q.id ? null : emit('pick', q.sql, q.name)">
        <div class="flex items-center gap-2">
          <template v-if="editingId === q.id">
            <input ref="renameInput" v-model="editName" @click.stop
              @keydown.enter="commitRename" @keydown.esc="editingId = null" @blur="commitRename"
              class="flex-1 bg-zinc-950 border border-zinc-700 rounded px-1 py-0.5 text-[12.5px] outline-none" />
          </template>
          <template v-else>
            <span class="text-[12.5px] truncate">{{ q.name }}</span>
            <button class="ml-auto text-zinc-600 hover:text-zinc-300 opacity-0 group-hover:opacity-100 text-xs"
              title="Rename" @click.stop="beginRename(q.id, q.name)">✎</button>
            <button class="text-red-500/60 hover:text-red-400 opacity-0 group-hover:opacity-100 text-xs"
              title="Delete" @click.stop="store.remove(q.id)">×</button>
          </template>
        </div>
        <div class="font-mono text-[10.5px] text-zinc-600 truncate">{{ q.sql }}</div>
      </li>
      <li v-if="!store.queries.length" class="p-3 text-xs text-zinc-600">
        No saved queries. Save one from a query tab with the ★ button.
      </li>
      <li v-else-if="!filtered.length" class="p-3 text-xs text-zinc-600">
        No saved queries match “{{ filter }}”.
      </li>
    </ul>
  </aside>
</template>
