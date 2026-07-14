<script setup lang="ts">
import { computed, nextTick, onMounted, onBeforeUnmount, ref } from "vue";
import { api, type LogSource } from "../lib/api";

const sources = ref<LogSource[]>([]);
const selected = ref<LogSource | null>(null);
const content = ref("");
const offset = ref<number | null>(null);
const error = ref<string | null>(null);
const paused = ref(false);
const customPath = ref("");
const body = ref<HTMLElement | null>(null);
let timer: number | null = null;

const GROUPS: { key: string; label: string }[] = [
  { key: "web", label: "Web server" },
  { key: "database", label: "Database" },
  { key: "runtime", label: "Runtime" },
  { key: "tool", label: "Tools" },
];
const groups = computed(() =>
  GROUPS.map((g) => ({ ...g, items: sources.value.filter((s) => s.category === g.key) }))
    .filter((g) => g.items.length),
);

async function poll() {
  if (!selected.value || paused.value) return;
  try {
    const chunk = await api.readLog(selected.value.path, offset.value);
    error.value = null;
    if (offset.value === null) content.value = chunk.content;       // first read: tail
    else if (chunk.content) content.value += chunk.content;         // append new bytes
    offset.value = chunk.offset;
    // keep the DOM bounded (~last 400k chars)
    if (content.value.length > 400_000) content.value = content.value.slice(-400_000);
    if (chunk.content) autoscroll();
  } catch (e: any) {
    error.value = e?.message ?? String(e);
  }
}
function autoscroll() {
  nextTick(() => { const el = body.value; if (el) el.scrollTop = el.scrollHeight; });
}

function select(s: LogSource) {
  selected.value = s;
  content.value = "";
  offset.value = null;
  error.value = null;
  poll().then(autoscroll);
}
function openCustom() {
  const p = customPath.value.trim();
  if (!p) return;
  select({ id: "custom", label: p.split("/").pop() ?? p, category: "custom", path: p });
}

onMounted(async () => {
  try { sources.value = await api.listLogs(); } catch { sources.value = []; }
  if (sources.value.length) select(sources.value[0]);
  timer = window.setInterval(poll, 1500);
});
onBeforeUnmount(() => { if (timer) window.clearInterval(timer); });
</script>

<template>
  <div class="flex-1 flex min-h-0">
    <!-- source list -->
    <aside class="w-64 shrink-0 border-r border-zinc-800 overflow-y-auto p-2">
      <div v-for="g in groups" :key="g.key" class="mb-2">
        <div class="text-[11px] uppercase tracking-wide text-zinc-600 px-2 py-1">{{ g.label }}</div>
        <button v-for="s in g.items" :key="s.id" @click="select(s)"
          class="w-full text-left px-2 py-1.5 rounded text-[12.5px] truncate"
          :class="selected?.id === s.id ? 'bg-zinc-800 text-white' : 'text-zinc-300 hover:bg-zinc-900'"
          :title="s.path">{{ s.label }}</button>
      </div>
      <div v-if="!sources.length" class="px-2 py-2 text-xs text-zinc-600">
        No service logs detected. Add a path below.
      </div>
      <div class="mt-2 px-1">
        <input v-model="customPath" @keydown.enter="openCustom" placeholder="Custom log path…"
          class="w-full bg-zinc-900 border border-zinc-800 rounded px-2 py-1 text-[11px] outline-none focus:border-blue-600" />
      </div>
    </aside>

    <!-- viewer -->
    <div class="flex-1 min-w-0 flex flex-col">
      <div class="h-11 shrink-0 flex items-center gap-2 px-4 border-b border-zinc-800">
        <span class="text-sm text-zinc-200 truncate">{{ selected?.label ?? "Select a log" }}</span>
        <span v-if="selected" class="text-[11px] text-zinc-600 truncate">{{ selected.path }}</span>
        <div v-if="selected" class="ml-auto flex items-center gap-2">
          <span class="text-[11px]" :class="paused ? 'text-amber-400' : 'text-emerald-400'">
            {{ paused ? "paused" : "● live" }}
          </span>
          <button @click="paused = !paused"
            class="text-xs px-2 py-1 rounded border border-zinc-700 hover:bg-zinc-800">{{ paused ? "Resume" : "Pause" }}</button>
          <button @click="content = ''" class="text-xs px-2 py-1 rounded border border-zinc-700 hover:bg-zinc-800">Clear</button>
        </div>
      </div>
      <div v-if="error" class="m-2 px-3 py-2 text-xs text-red-400 border border-red-900 rounded">{{ error }}</div>
      <div ref="body" class="flex-1 overflow-auto p-3 font-mono text-[11.5px] leading-relaxed text-zinc-300 whitespace-pre-wrap bg-zinc-950">{{ content || (selected ? "" : "No log selected.") }}</div>
    </div>
  </div>
</template>
