<script setup lang="ts">
import { onMounted, ref } from "vue";
import { api, type PortStatus } from "../lib/api";

// common local dev ports and what they usually are
const KNOWN: { port: number; label: string }[] = [
  { port: 80, label: "HTTP" },
  { port: 443, label: "HTTPS" },
  { port: 8080, label: "Nginx/Apache (brew)" },
  { port: 3306, label: "MySQL / MariaDB" },
  { port: 5432, label: "PostgreSQL" },
  { port: 6379, label: "Redis" },
  { port: 1433, label: "SQL Server" },
  { port: 9000, label: "PHP-FPM" },
  { port: 1025, label: "Mailpit SMTP" },
  { port: 8025, label: "Mailpit web" },
  { port: 3000, label: "Node dev server" },
  { port: 5173, label: "Vite dev server" },
];

const ports = ref<PortStatus[]>([]);
const loading = ref(false);
async function scan() {
  loading.value = true;
  try {
    ports.value = await api.checkPorts(KNOWN.map((k) => k.port));
  } finally {
    loading.value = false;
  }
}
function label(port: number) {
  return KNOWN.find((k) => k.port === port)?.label ?? "";
}
onMounted(scan);
</script>

<template>
  <div class="flex-1 overflow-y-auto p-5">
    <div class="max-w-2xl">
      <div class="flex items-center mb-3">
        <h2 class="text-sm font-medium text-zinc-200">Ports in use</h2>
        <button @click="scan" :disabled="loading"
          class="ml-auto text-xs px-2.5 py-1.5 rounded border border-zinc-700 hover:bg-zinc-800 disabled:opacity-40">Rescan</button>
      </div>
      <p class="text-xs text-zinc-500 mb-3">Which common local dev ports are currently listening.</p>
      <ul class="rounded-lg border border-zinc-800 divide-y divide-zinc-800/70">
        <li v-for="p in ports" :key="p.port" class="flex items-center gap-3 px-3 py-2">
          <span class="font-mono text-sm text-zinc-300 w-14">{{ p.port }}</span>
          <span class="text-sm text-zinc-400 flex-1 truncate">{{ label(p.port) }}</span>
          <span v-if="p.open" class="text-[11px] px-2 py-0.5 rounded-full bg-emerald-950/60 text-emerald-400 border border-emerald-900">● in use</span>
          <span v-else class="text-[11px] px-2 py-0.5 rounded-full bg-zinc-800 text-zinc-500 border border-zinc-700">free</span>
        </li>
      </ul>
    </div>
  </div>
</template>
