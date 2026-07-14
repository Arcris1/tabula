<script setup lang="ts">
import { ref } from "vue";
import { useServicesStore } from "../stores/services";
import ServicesView from "./ServicesView.vue";
import DatabasesView from "./DatabasesView.vue";

const emit = defineEmits<{ exit: [] }>();
const services = useServicesStore();

type Section = "dashboard" | "services" | "databases" | "logs" | "mail" | "settings";
const section = ref<Section>("services");

const nav: { id: Section; label: string; icon: string; ready?: boolean }[] = [
  { id: "dashboard", label: "Dashboard", icon: "▦" },
  { id: "services", label: "Services", icon: "≋", ready: true },
  { id: "databases", label: "Databases", icon: "▤", ready: true },
  { id: "logs", label: "Logs", icon: "≡" },
  { id: "mail", label: "Mail", icon: "✉" },
  { id: "settings", label: "Settings", icon: "⚙" },
];
const titles: Record<Section, string> = {
  dashboard: "Dashboard", services: "Services", databases: "Databases",
  logs: "Logs", mail: "Mail", settings: "Settings",
};
</script>

<template>
  <div class="h-full flex">
    <!-- sidebar -->
    <aside class="w-52 shrink-0 border-r border-zinc-800 flex flex-col p-3">
      <div class="px-2 py-2">
        <div class="text-lg font-semibold tracking-tight text-zinc-100">Tabula</div>
        <div class="text-[11px] text-zinc-600 leading-tight">Your local stack,<br />one place.</div>
      </div>
      <nav class="mt-3 flex flex-col gap-0.5">
        <button v-for="n in nav" :key="n.id" @click="n.ready && (section = n.id)"
          class="flex items-center gap-2.5 px-2.5 py-1.5 rounded text-sm text-left"
          :class="[
            section === n.id ? 'bg-zinc-800 text-white' : 'text-zinc-400 hover:bg-zinc-900',
            n.ready ? '' : 'opacity-40 cursor-default',
          ]">
          <span class="w-4 text-center text-zinc-500">{{ n.icon }}</span>
          {{ n.label }}
          <span v-if="!n.ready" class="ml-auto text-[9px] uppercase text-zinc-600">soon</span>
        </button>
      </nav>
      <button @click="emit('exit')"
        class="mt-auto flex items-center gap-2 px-2.5 py-1.5 rounded text-xs text-zinc-400 hover:bg-zinc-900 border border-zinc-800">
        ← Database client
      </button>
    </aside>

    <!-- main -->
    <div class="flex-1 min-w-0 flex flex-col">
      <header class="h-14 shrink-0 flex items-center gap-3 px-5 border-b border-zinc-800">
        <h1 class="text-lg font-semibold text-zinc-100">{{ titles[section] }}</h1>
        <div v-if="section === 'services'" class="ml-auto flex items-center gap-2">
          <button @click="services.load()" :disabled="services.loading"
            class="text-xs px-2.5 py-1.5 rounded border border-zinc-700 hover:bg-zinc-800 disabled:opacity-40">Refresh</button>
          <button @click="services.startAll()"
            class="text-xs px-3 py-1.5 rounded bg-emerald-600 hover:bg-emerald-500 text-white">Start all</button>
          <button @click="services.stopAll()"
            class="text-xs px-3 py-1.5 rounded border border-zinc-700 hover:bg-zinc-800">Stop all</button>
        </div>
      </header>

      <ServicesView v-if="section === 'services'" />
      <DatabasesView v-else-if="section === 'databases'" />
      <div v-else class="flex-1 flex items-center justify-center text-zinc-600 text-sm">
        {{ titles[section] }} — coming soon
      </div>
    </div>
  </div>
</template>
