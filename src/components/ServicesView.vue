<script setup lang="ts">
import { onMounted } from "vue";
import { useServicesStore } from "../stores/services";

const store = useServicesStore();
onMounted(() => store.load());

const kindStyle: Record<string, string> = {
  mysql: "bg-blue-600", mariadb: "bg-amber-600",
  postgres: "bg-indigo-600", redis: "bg-red-600",
};
function badge(kind: string) {
  return kindStyle[kind] ?? "bg-zinc-600";
}
</script>

<template>
  <div class="flex-1 overflow-y-auto p-5">
    <div v-if="store.error" class="mb-3 px-3 py-2 text-xs text-red-400 border border-red-900 rounded">
      {{ store.error }}
    </div>

    <div class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3">
      <div v-for="s in store.services" :key="s.id"
        class="rounded-lg border border-zinc-800 bg-zinc-900/40 p-4 flex flex-col gap-3">
        <!-- header -->
        <div class="flex items-start gap-3">
          <div class="w-8 h-8 rounded-md flex items-center justify-center text-white text-sm font-semibold shrink-0"
            :class="badge(s.kind)">{{ s.name.charAt(0) }}</div>
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-2">
              <span class="text-sm font-medium text-zinc-100">{{ s.name }}</span>
              <span v-if="s.running"
                class="text-[10px] px-1.5 py-0.5 rounded-full bg-emerald-950/60 text-emerald-400 border border-emerald-900 whitespace-nowrap">
                ● Running<template v-if="s.port"> · {{ s.port }}</template>
              </span>
              <span v-else-if="s.installed"
                class="text-[10px] px-1.5 py-0.5 rounded-full bg-zinc-800 text-zinc-500 border border-zinc-700">Stopped</span>
              <span v-else
                class="text-[10px] px-1.5 py-0.5 rounded-full bg-zinc-800 text-zinc-600 border border-zinc-700">Not installed</span>
            </div>
            <div class="text-xs text-zinc-500 mt-0.5">{{ s.description }}</div>
          </div>
        </div>

        <!-- footer: version + actions -->
        <div class="flex items-center gap-2 mt-auto">
          <span v-if="s.version"
            class="text-[11px] font-mono px-2 py-0.5 rounded border border-zinc-800 text-emerald-400/90">{{ s.version }}</span>
          <div class="ml-auto flex items-center gap-1.5">
            <template v-if="s.installed && s.manageable">
              <button v-if="!s.running" @click="store.act(s.id, 'start')" :disabled="store.busy.has(s.id)"
                class="text-xs px-2.5 py-1 rounded bg-emerald-600 hover:bg-emerald-500 text-white disabled:opacity-40">Start</button>
              <template v-else>
                <button @click="store.act(s.id, 'restart')" :disabled="store.busy.has(s.id)"
                  class="text-xs px-2.5 py-1 rounded border border-zinc-700 hover:bg-zinc-800 disabled:opacity-40">Restart</button>
                <button @click="store.act(s.id, 'stop')" :disabled="store.busy.has(s.id)"
                  class="text-xs px-2.5 py-1 rounded border border-red-900/70 text-red-400 hover:bg-red-950/60 disabled:opacity-40">Stop</button>
              </template>
              <span v-if="store.busy.has(s.id)" class="text-[11px] text-zinc-500 animate-pulse">…</span>
            </template>
            <span v-else-if="s.running && !s.manageable" class="text-[11px] text-zinc-600" title="Running outside a managed service (e.g. Docker or manual)">unmanaged</span>
            <span v-else class="text-[11px] text-zinc-600">not installed</span>
          </div>
        </div>
      </div>
    </div>

    <div v-if="!store.services.length && !store.loading" class="text-center text-zinc-600 text-xs py-12">
      No services detected.
    </div>
  </div>
</template>
