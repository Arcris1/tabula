<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { api, type ConnectionConfig, type DbInfo } from "../lib/api";
import { useConnectionsStore } from "../stores/connections";
import ConfirmModal from "./ConfirmModal.vue";

const conns = useConnectionsStore();
const emit = defineEmits<{ open: [connectionId: string, database: string] }>();
onMounted(() => conns.load());

// only server engines have a "databases" concept
const SERVER = ["mysql", "mariadb", "postgres", "mssql"];
const serverConns = computed(() => conns.connections.filter((c) => SERVER.includes(c.engine)));

const selected = ref<ConnectionConfig | null>(null);
const dbs = ref<DbInfo[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);
const newName = ref("");
const creating = ref(false);
const dropTarget = ref<string | null>(null);

async function select(c: ConnectionConfig) {
  selected.value = c;
  dbs.value = [];
  error.value = null;
  loading.value = true;
  try {
    await conns.connect(c.id); // ensure a live connection (idempotent)
    dbs.value = await api.listDatabases(c.id);
  } catch (e: any) {
    error.value = e?.message ?? String(e);
  } finally {
    loading.value = false;
  }
}
async function reload() {
  if (selected.value) await select(selected.value);
}
async function create() {
  const name = newName.value.trim();
  if (!name || !selected.value) return;
  creating.value = true;
  error.value = null;
  try {
    await api.createDatabase(selected.value.id, name);
    newName.value = "";
    await reload();
  } catch (e: any) {
    error.value = e?.message ?? String(e);
  } finally {
    creating.value = false;
  }
}
async function confirmDrop() {
  const name = dropTarget.value;
  dropTarget.value = null;
  if (!name || !selected.value) return;
  error.value = null;
  try {
    await api.dropDatabase(selected.value.id, name);
    await reload();
  } catch (e: any) {
    error.value = e?.message ?? String(e);
  }
}

function fmtSize(bytes?: number | null): string {
  if (bytes == null) return "";
  const u = ["B", "KB", "MB", "GB", "TB"];
  let n = bytes, i = 0;
  while (n >= 1024 && i < u.length - 1) { n /= 1024; i++; }
  return `${n.toFixed(n < 10 && i > 0 ? 1 : 0)} ${u[i]}`;
}
const colorClass: Record<string, string> = {
  gray: "bg-zinc-500", blue: "bg-blue-500", green: "bg-green-500", amber: "bg-amber-500", red: "bg-red-500",
};
</script>

<template>
  <div class="flex-1 flex min-h-0">
    <!-- connection list -->
    <aside class="w-64 shrink-0 border-r border-zinc-800 overflow-y-auto p-2">
      <div class="text-[11px] uppercase tracking-wide text-zinc-600 px-2 py-1">Connections</div>
      <button v-for="c in serverConns" :key="c.id" @click="select(c)"
        class="w-full text-left px-2 py-2 rounded flex items-center gap-2"
        :class="selected?.id === c.id ? 'bg-zinc-800 text-white' : 'text-zinc-300 hover:bg-zinc-900'">
        <span class="w-2 h-2 rounded-full shrink-0" :class="colorClass[c.color]" />
        <span class="min-w-0 flex-1">
          <span class="block truncate text-sm">{{ c.name }}</span>
          <span class="block text-[11px] text-zinc-500">{{ c.engine }}<span v-if="c.host"> · {{ c.host }}</span></span>
        </span>
      </button>
      <div v-if="!serverConns.length" class="px-2 py-3 text-xs text-zinc-600">
        No server connections. Add a MySQL / Postgres / SQL Server connection in the client.
      </div>
    </aside>

    <!-- databases of the selected connection -->
    <div class="flex-1 min-w-0 overflow-y-auto p-5">
      <div v-if="error" class="mb-3 px-3 py-2 text-xs text-red-400 border border-red-900 rounded">{{ error }}</div>

      <div v-if="!selected" class="text-zinc-600 text-sm flex items-center justify-center h-full">
        Select a connection to manage its databases.
      </div>

      <template v-else>
        <div class="flex items-center gap-2 mb-4">
          <h2 class="text-base font-medium text-zinc-100">{{ selected.name }}</h2>
          <span class="text-xs text-zinc-500">{{ selected.engine }}</span>
          <button @click="reload" :disabled="loading"
            class="ml-auto text-xs px-2.5 py-1.5 rounded border border-zinc-700 hover:bg-zinc-800 disabled:opacity-40">Refresh</button>
        </div>

        <!-- create -->
        <div class="flex gap-2 mb-4 max-w-md">
          <input v-model="newName" @keydown.enter="create" placeholder="New database name…"
            class="flex-1 bg-zinc-900 border border-zinc-800 rounded px-2 py-1.5 text-sm outline-none focus:border-blue-600" />
          <button @click="create" :disabled="!newName.trim() || creating"
            class="text-sm px-3 py-1.5 rounded bg-emerald-600 hover:bg-emerald-500 text-white disabled:opacity-40">Create</button>
        </div>

        <!-- list -->
        <div v-if="loading" class="text-xs text-zinc-500">Loading…</div>
        <ul v-else class="rounded-lg border border-zinc-800 divide-y divide-zinc-800/70">
          <li v-for="d in dbs" :key="d.name"
            class="group flex items-center gap-3 px-3 py-2.5 hover:bg-zinc-900/50 cursor-pointer"
            @dblclick="selected && emit('open', selected.id, d.name)">
            <span class="text-zinc-500">▤</span>
            <span class="text-sm text-zinc-200 flex-1 truncate">{{ d.name }}</span>
            <span class="text-xs text-zinc-500 font-mono">{{ fmtSize(d.sizeBytes) }}</span>
            <button @click.stop="selected && emit('open', selected.id, d.name)"
              class="text-xs px-2 py-1 rounded bg-blue-600 hover:bg-blue-500 text-white opacity-0 group-hover:opacity-100">Open ↗</button>
            <button @click.stop="dropTarget = d.name"
              class="text-xs px-2 py-1 rounded text-red-500/70 hover:text-red-400 hover:bg-red-950/40 opacity-0 group-hover:opacity-100">Drop</button>
          </li>
          <li v-if="!dbs.length" class="px-3 py-4 text-xs text-zinc-600">No user databases.</li>
        </ul>
      </template>
    </div>

    <ConfirmModal v-if="dropTarget" title="Drop database"
      :message="`Permanently DROP database “${dropTarget}” on ${selected?.name}? This deletes all its data and cannot be undone.`"
      confirm-label="Drop database" danger
      @confirm="confirmDrop" @cancel="dropTarget = null" />
  </div>
</template>
