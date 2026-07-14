<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { api, type ConnectionConfig } from "../lib/api";
import { useConnectionsStore } from "../stores/connections";
import { useShortcutsStore } from "../stores/shortcuts";
import ConnectionForm from "./ConnectionForm.vue";

defineProps<{ canCancel?: boolean }>();
const emit = defineEmits<{ connected: [id: string, paradigm: "sql" | "kv"]; settings: []; cancel: []; stack: [] }>();
const store = useConnectionsStore();
const editing = ref<ConnectionConfig | null>(null);
const showForm = ref(false);
const busyId = ref<string | null>(null);
const errorById = ref<Record<string, string>>({});

onMounted(store.load);
const shortcuts = useShortcutsStore();
const unregister = shortcuts.register("launcher:new", () => add());
onBeforeUnmount(unregister);

const colorClass: Record<string, string> = {
  gray: "bg-zinc-500", blue: "bg-blue-500", green: "bg-green-500",
  amber: "bg-amber-500", red: "bg-red-500",
};

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

function hostKeyChanged(id: string): boolean {
  return /host key/i.test(errorById.value[id] ?? "");
}
async function forgetAndRetry(c: ConnectionConfig) {
  if (!c.ssh) return;
  await api.forgetHostKey(c.ssh.host, c.ssh.port); // trust the new key on next connect
  await open(c);
}
</script>

<template>
  <div class="h-full flex items-center justify-center">
    <ConnectionForm v-if="showForm" :initial="editing"
      @saved="showForm = false" @cancel="showForm = false" />
    <div v-else class="w-[28rem]">
      <div class="mb-6 text-center">
        <div class="text-2xl font-semibold tracking-tight text-zinc-100">Tabula</div>
        <div class="text-[11px] text-zinc-600 mt-1">Created by Arcris Silang</div>
      </div>
      <div class="flex items-center justify-between mb-4">
        <h1 class="text-sm font-medium text-zinc-400">Connections</h1>
        <div class="flex gap-2">
          <button v-if="canCancel" @click="emit('cancel')" class="px-2 py-1 rounded text-zinc-500 hover:bg-zinc-800 text-xs">← Back</button>
          <button @click="emit('stack')" class="px-2 py-1 rounded text-zinc-500 hover:bg-zinc-800 text-xs" title="Local stack manager">Local Stack</button>
          <button @click="emit('settings')" class="px-2 py-1 rounded text-zinc-500 hover:bg-zinc-800 text-xs" title="Settings (⌘,)">⚙</button>
          <button @click="add" class="px-2 py-1 rounded border border-zinc-700 hover:bg-zinc-800 text-xs">+ New</button>
        </div>
      </div>
      <ul class="flex flex-col gap-1">
        <li v-for="c in store.connections" :key="c.id"
          class="group flex items-center gap-3 px-3 py-2 rounded border border-zinc-800 hover:border-zinc-600 cursor-pointer"
          @dblclick="open(c)">
          <span class="w-2.5 h-2.5 rounded-full" :class="colorClass[c.color]" />
          <div class="flex-1 min-w-0">
            <div class="truncate">{{ c.name }}</div>
            <div class="text-xs text-zinc-500">{{ c.engine }}<span v-if="c.host"> · {{ c.host }}</span></div>
            <div v-if="errorById[c.id]" class="text-xs text-red-400" :class="hostKeyChanged(c.id) ? 'whitespace-pre-wrap' : 'truncate'">{{ errorById[c.id] }}</div>
            <button v-if="hostKeyChanged(c.id)" @click.stop="forgetAndRetry(c)"
              class="mt-1 text-[11px] px-2 py-0.5 rounded border border-amber-800 text-amber-400 hover:bg-amber-950">
              Trust new key &amp; reconnect
            </button>
          </div>
          <button @click.stop="open(c)" class="text-xs px-2 py-1 rounded bg-zinc-800 opacity-0 group-hover:opacity-100">
            {{ busyId === c.id ? "…" : "Connect" }}
          </button>
          <button @click.stop="edit(c)" class="text-xs text-zinc-500 opacity-0 group-hover:opacity-100">Edit</button>
          <button @click.stop="store.remove(c.id)" class="text-xs text-red-500/70 opacity-0 group-hover:opacity-100">Delete</button>
        </li>
        <li v-if="!store.connections.length" class="text-zinc-600 text-center py-8 text-xs">
          No connections yet — create one.
        </li>
      </ul>
    </div>
  </div>
</template>
