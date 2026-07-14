<script setup lang="ts">
import { computed, onMounted } from "vue";
import { useServicesStore } from "../stores/services";

const store = useServicesStore();
onMounted(() => store.load());

const installed = computed(() => store.services.filter((s) => s.installed));
const running = computed(() => installed.value.filter((s) => s.running));
const byCat = (cat: string) => installed.value.filter((s) => s.category === cat);

const tiles = computed(() => [
  { label: "Services running", value: `${running.value.length} / ${installed.value.length}` },
  { label: "Web", value: `${byCat("web").filter((s) => s.running).length} / ${byCat("web").length}` },
  { label: "Databases", value: `${byCat("database").filter((s) => s.running).length} / ${byCat("database").length}` },
]);

const kindStyle: Record<string, string> = {
  nginx: "bg-emerald-600", apache: "bg-red-600", php: "bg-indigo-600", node: "bg-green-600",
  mysql: "bg-blue-600", mariadb: "bg-amber-600", postgres: "bg-indigo-600", redis: "bg-red-600",
};
</script>

<template>
  <div class="flex-1 overflow-y-auto p-5">
    <!-- stat tiles -->
    <div class="grid grid-cols-3 gap-3 mb-5">
      <div v-for="t in tiles" :key="t.label" class="rounded-lg border border-zinc-800 bg-zinc-900/40 p-4">
        <div class="text-2xl font-semibold text-zinc-100">{{ t.value }}</div>
        <div class="text-xs text-zinc-500 mt-1">{{ t.label }}</div>
      </div>
    </div>

    <!-- service status at a glance -->
    <div class="rounded-lg border border-zinc-800 divide-y divide-zinc-800/70">
      <div v-for="s in installed" :key="s.id" class="flex items-center gap-3 px-3 py-2.5">
        <div class="w-6 h-6 rounded flex items-center justify-center text-white text-[11px] font-semibold shrink-0"
          :class="kindStyle[s.kind] ?? 'bg-zinc-600'">{{ s.name.charAt(0) }}</div>
        <span class="text-sm text-zinc-200 flex-1 truncate">{{ s.name }}</span>
        <span v-if="s.version" class="text-[11px] font-mono text-zinc-500">{{ s.version }}</span>
        <span v-if="s.running" class="text-[11px] text-emerald-400 flex items-center gap-1">● Running</span>
        <span v-else-if="s.manageable" class="text-[11px] text-zinc-500">Stopped</span>
        <span v-else class="text-[11px] text-zinc-600">—</span>
        <button v-if="s.manageable" @click="store.act(s.id, s.running ? 'stop' : 'start')" :disabled="store.busy.has(s.id)"
          class="text-[11px] px-2 py-1 rounded border border-zinc-700 hover:bg-zinc-800 disabled:opacity-40 w-14 text-center">
          {{ s.running ? 'Stop' : 'Start' }}
        </button>
        <span v-else class="w-14"></span>
      </div>
      <div v-if="!installed.length && !store.loading" class="px-3 py-6 text-center text-xs text-zinc-600">
        No local services detected. Install some via Homebrew (mac) or as Windows services.
      </div>
    </div>
  </div>
</template>
