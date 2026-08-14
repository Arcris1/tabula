<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { api, type ConnectionConfig, type Engine } from "../lib/api";
import { useConnectionsStore } from "../stores/connections";
import { useShortcutsStore } from "../stores/shortcuts";
import ConnectionForm from "./ConnectionForm.vue";

defineProps<{ canCancel?: boolean }>();
const emit = defineEmits<{ connected: [id: string, paradigm: "sql" | "kv"]; settings: []; cancel: []; databases: [] }>();
const store = useConnectionsStore();
const editing = ref<ConnectionConfig | null>(null);
const showForm = ref(false);
const busyId = ref<string | null>(null);
const errorById = ref<Record<string, string>>({});
const deletingId = ref<string | null>(null);

const filter = ref("");
const highlight = ref(0);
const searchEl = ref<HTMLInputElement | null>(null);

onMounted(async () => {
  await store.load();
  nextTick(() => searchEl.value?.focus());
});
const shortcuts = useShortcutsStore();
const unregister = shortcuts.register("launcher:new", () => add());
onBeforeUnmount(unregister);

// engine identity: monogram badge + brand-ish colour
const engineMeta: Record<Engine, { label: string; cls: string }> = {
  postgres: { label: "Pg", cls: "bg-blue-600" },
  mysql: { label: "My", cls: "bg-cyan-600" },
  mssql: { label: "MS", cls: "bg-red-600" },
  redis: { label: "Rd", cls: "bg-rose-600" },
  sqlite: { label: "Li", cls: "bg-slate-600" },
};
function badge(engine: Engine) {
  return engineMeta[engine] ?? { label: engine.slice(0, 2), cls: "bg-zinc-600" };
}
// environment accent (user-assigned) shown as a left bar
const envBar: Record<string, string> = {
  gray: "bg-zinc-600", blue: "bg-blue-500", green: "bg-emerald-500",
  amber: "bg-amber-500", red: "bg-red-500",
};

const filtered = computed(() => {
  const q = filter.value.trim().toLowerCase();
  if (!q) return store.connections;
  return store.connections.filter((c) =>
    `${c.name} ${c.engine} ${c.host ?? ""}`.toLowerCase().includes(q),
  );
});
watch(filtered, () => { highlight.value = 0; });

async function open(c: ConnectionConfig) {
  busyId.value = c.id;
  errorById.value = {};
  try {
    const paradigm = await store.connect(c.id);
    emit("connected", c.id, paradigm);
  } catch (e: any) {
    errorById.value[c.id] = e?.message ?? String(e);
  } finally {
    busyId.value = null;
  }
}
function edit(c: ConnectionConfig) { editing.value = c; showForm.value = true; }
function add() { editing.value = null; showForm.value = true; }
function confirmDelete(id: string) { store.remove(id); deletingId.value = null; }

function hostKeyChanged(id: string): boolean {
  return /host key/i.test(errorById.value[id] ?? "");
}
async function forgetAndRetry(c: ConnectionConfig) {
  if (!c.ssh) return;
  await api.forgetHostKey(c.ssh.host, c.ssh.port);
  await open(c);
}

// keyboard-driven picking
function onKeydown(e: KeyboardEvent) {
  const n = filtered.value.length;
  if (e.key === "ArrowDown") { e.preventDefault(); highlight.value = Math.min(highlight.value + 1, n - 1); }
  else if (e.key === "ArrowUp") { e.preventDefault(); highlight.value = Math.max(highlight.value - 1, 0); }
  else if (e.key === "Enter") { const c = filtered.value[highlight.value]; if (c) open(c); }
  else if (e.key === "Escape" && filter.value) { e.preventDefault(); filter.value = ""; }
}
</script>

<template>
  <div class="h-full flex items-center justify-center p-6">
    <ConnectionForm v-if="showForm" :initial="editing"
      @saved="showForm = false" @cancel="showForm = false" />

    <div v-else class="w-full max-w-lg">
      <!-- wordmark -->
      <div class="mb-7 text-center">
        <div class="text-[26px] font-semibold tracking-tight text-zinc-50">Tabula</div>
        <div class="text-[11px] text-zinc-600 mt-1 tracking-wide">Created by Arcris Silang</div>
      </div>

      <!-- toolbar: search + actions -->
      <div class="flex items-center gap-2 mb-3">
        <div class="relative flex-1">
          <span class="absolute left-2.5 top-1/2 -translate-y-1/2 text-zinc-600 text-sm pointer-events-none">⌕</span>
          <input ref="searchEl" v-model="filter" @keydown="onKeydown"
            placeholder="Search connections…"
            class="w-full bg-zinc-900 border border-zinc-800 rounded-lg pl-8 pr-3 py-2 text-sm outline-none focus:border-blue-600/70 placeholder:text-zinc-500" />
        </div>
        <button @click="add"
          class="shrink-0 px-3 py-2 rounded-lg bg-blue-600 hover:bg-blue-500 text-white text-sm font-medium">+ New</button>
      </div>

      <!-- meta row -->
      <div class="flex items-center justify-between px-1 mb-2">
        <span class="text-[11px] uppercase tracking-wide text-zinc-600">
          {{ filtered.length }} connection{{ filtered.length === 1 ? "" : "s" }}
        </span>
        <div class="flex items-center gap-3 text-[11px] text-zinc-500">
          <button @click="emit('databases')" class="hover:text-zinc-300">Databases</button>
          <button @click="emit('settings')" class="hover:text-zinc-300" title="Settings (⌘,)">Settings</button>
          <button v-if="canCancel" @click="emit('cancel')" class="hover:text-zinc-300">← Back</button>
        </div>
      </div>

      <!-- list -->
      <ul class="flex flex-col gap-1.5">
        <li v-for="(c, i) in filtered" :key="c.id"
          @mouseenter="highlight = i" @dblclick="open(c)"
          class="group relative flex items-center gap-3 pl-4 pr-2.5 py-2.5 rounded-lg border bg-zinc-900/40 cursor-pointer overflow-hidden transition-colors"
          :class="highlight === i ? 'border-blue-600/60 bg-zinc-900' : 'border-zinc-800 hover:border-zinc-700'">
          <!-- environment accent bar -->
          <span class="absolute left-0 top-0 bottom-0 w-1" :class="envBar[c.color] ?? 'bg-zinc-600'" />

          <!-- engine badge -->
          <span class="w-9 h-9 rounded-md flex items-center justify-center text-white text-xs font-semibold shrink-0"
            :class="badge(c.engine).cls">{{ badge(c.engine).label }}</span>

          <!-- name + address -->
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2">
              <span class="text-[13.5px] font-medium text-zinc-100 truncate">{{ c.name }}</span>
              <span v-if="c.isProduction"
                class="text-[9px] font-semibold uppercase tracking-wide text-red-400 border border-red-900/70 rounded px-1 shrink-0">prod</span>
            </div>
            <div class="text-[11.5px] text-zinc-500 font-mono truncate">
              {{ c.engine }}<span v-if="c.host"> · {{ c.host }}<span v-if="c.port" class="text-zinc-600">:{{ c.port }}</span></span>
            </div>
            <template v-if="errorById[c.id]">
              <div class="text-[11px] text-red-400 mt-1" :class="hostKeyChanged(c.id) ? 'whitespace-pre-wrap' : 'truncate'">{{ errorById[c.id] }}</div>
              <button v-if="hostKeyChanged(c.id)" @click.stop="forgetAndRetry(c)"
                class="mt-1 text-[11px] px-2 py-0.5 rounded border border-amber-800 text-amber-400 hover:bg-amber-950">Trust new key &amp; reconnect</button>
            </template>
          </div>

          <!-- actions -->
          <div v-if="deletingId === c.id" class="flex items-center gap-1.5 shrink-0" @click.stop>
            <span class="text-[11px] text-zinc-400">Delete?</span>
            <button @click="confirmDelete(c.id)" class="text-[11px] px-2 py-1 rounded bg-red-600 hover:bg-red-500 text-white">Delete</button>
            <button @click="deletingId = null" class="text-[11px] px-2 py-1 rounded text-zinc-400 hover:bg-zinc-800">Cancel</button>
          </div>
          <div v-else class="flex items-center gap-1 shrink-0">
            <span v-if="highlight === i" class="text-[11px] text-zinc-600 mr-1 hidden sm:inline">⏎</span>
            <button @click.stop="open(c)"
              class="text-xs px-2.5 py-1 rounded bg-zinc-800 hover:bg-zinc-700 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
              :class="{ 'opacity-100': highlight === i }">
              {{ busyId === c.id ? "…" : "Connect" }}
            </button>
            <button @click.stop="edit(c)" aria-label="Edit connection"
              class="w-6 h-6 flex items-center justify-center rounded text-zinc-400 hover:text-zinc-100 hover:bg-zinc-800 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
              :class="{ 'opacity-100': highlight === i }" title="Edit">✎</button>
            <button @click.stop="deletingId = c.id" aria-label="Delete connection"
              class="w-6 h-6 flex items-center justify-center rounded text-zinc-400 hover:text-red-400 hover:bg-red-950/40 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
              :class="{ 'opacity-100': highlight === i }" title="Delete">
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 6h18M8 6V4a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v2m2 0v14a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V6"/><path d="M10 11v6M14 11v6"/></svg>
            </button>
          </div>
        </li>

        <!-- empty states -->
        <li v-if="!store.connections.length" class="rounded-lg border border-dashed border-zinc-800 py-10 text-center">
          <div class="text-sm text-zinc-400">No connections yet</div>
          <div class="text-xs text-zinc-600 mt-1 mb-3">Add a database to get started.</div>
          <button @click="add" class="text-xs px-3 py-1.5 rounded-lg bg-blue-600 hover:bg-blue-500 text-white">+ New connection</button>
        </li>
        <li v-else-if="!filtered.length" class="py-8 text-center text-xs text-zinc-600">
          No connections match “{{ filter }}”.
        </li>
      </ul>
    </div>
  </div>
</template>
