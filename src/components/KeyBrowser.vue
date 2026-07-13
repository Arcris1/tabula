<script setup lang="ts">
import { onMounted, ref } from "vue";
import { api, type KeyTreeNode } from "../lib/api";
import { useWorkspaceStore } from "../stores/workspace";
import KeyTreeItem from "./KeyTreeItem";

const emit = defineEmits<{ select: [key: string] }>();
const ws = useWorkspaceStore();
const pattern = ref("*");
const tree = ref<KeyTreeNode[]>([]);
const keyCount = ref(0);
const loading = ref(false);
const done = ref(false);
const expanded = ref<Set<string>>(new Set());
let cursor = 0;
let allKeys: string[] = [];

async function scan(reset: boolean) {
  if (!ws.activeId || loading.value) return;
  if (reset) { cursor = 0; allKeys = []; done.value = false; }
  loading.value = true;
  try {
    // pull a few SCAN pages per click, stop at cursor 0
    for (let i = 0; i < 5; i++) {
      const page = await api.scanKeys(ws.activeId, pattern.value, cursor);
      allKeys = allKeys.concat(page.keys);
      cursor = page.cursor;
      if (cursor === 0) { done.value = true; break; }
    }
    keyCount.value = allKeys.length;
    tree.value = await api.buildKeyTree(allKeys);
  } finally {
    loading.value = false;
  }
}
function toggle(path: string) {
  if (expanded.value.has(path)) expanded.value.delete(path);
  else expanded.value.add(path);
  expanded.value = new Set(expanded.value);
}
onMounted(() => scan(true));
</script>

<template>
  <aside class="w-72 shrink-0 border-r border-zinc-800 flex flex-col">
    <div class="p-2 flex gap-1">
      <input v-model="pattern" @keydown.enter="scan(true)" placeholder="Pattern (e.g. cache:*)"
        class="flex-1 bg-zinc-900 border border-zinc-800 rounded px-2 py-1 text-xs outline-none focus:border-zinc-600" />
      <button @click="scan(true)" class="text-xs px-2 rounded border border-zinc-700 hover:bg-zinc-800">Scan</button>
    </div>
    <div class="flex-1 overflow-y-auto px-1 pb-2 text-[12.5px]">
      <KeyTreeItem v-for="node in tree" :key="node.name" :node="node" :depth="0" :expanded="expanded"
        @toggle="toggle" @select="(k: string) => emit('select', k)" />
      <div v-if="!tree.length && !loading" class="text-zinc-600 text-xs px-2 py-2">No keys</div>
    </div>
    <footer class="px-2 py-1 border-t border-zinc-800 text-[11px] text-zinc-500 flex gap-2">
      <span>{{ keyCount }} keys{{ done ? "" : " (partial)" }}</span>
      <button v-if="!done" @click="scan(false)" class="text-blue-400">load more</button>
    </footer>
  </aside>
</template>
