<script setup lang="ts">
import { computed, onMounted } from "vue";
import { api } from "../lib/api";
import { useServicesStore } from "../stores/services";

const store = useServicesStore();
onMounted(() => { if (!store.services.length) store.load(); });

const mailpit = computed(() => store.services.find((s) => s.kind === "mailpit"));
const WEB = 8025;
const SMTP = 1025;

function openInbox() {
  api.openExternal(`http://localhost:${WEB}`).catch(() => {});
}
</script>

<template>
  <div class="flex-1 overflow-y-auto p-5">
    <div v-if="!mailpit || !mailpit.installed" class="max-w-lg text-sm text-zinc-400">
      <p class="mb-2">Mailpit isn't installed.</p>
      <p class="text-xs text-zinc-500">Install it with <code class="text-zinc-300">brew install mailpit</code>, then it'll appear here — a local SMTP server that catches outgoing mail so it never leaves your machine.</p>
    </div>

    <template v-else>
      <div class="rounded-lg border border-zinc-800 bg-zinc-900/40 p-5 max-w-2xl">
        <div class="flex items-center gap-3 mb-4">
          <div class="w-9 h-9 rounded-md bg-emerald-600 flex items-center justify-center text-white">✉</div>
          <div>
            <div class="text-base font-medium text-zinc-100">Mailpit</div>
            <div class="text-xs text-zinc-500">Local mail catcher — SMTP server + web inbox</div>
          </div>
          <span v-if="mailpit.running" class="ml-auto text-[11px] px-2 py-0.5 rounded-full bg-emerald-950/60 text-emerald-400 border border-emerald-900">● Running</span>
          <span v-else class="ml-auto text-[11px] px-2 py-0.5 rounded-full bg-zinc-800 text-zinc-500 border border-zinc-700">Stopped</span>
        </div>

        <dl class="grid grid-cols-2 gap-3 text-sm mb-5">
          <div class="rounded border border-zinc-800 px-3 py-2">
            <dt class="text-[11px] uppercase tracking-wide text-zinc-600">SMTP (point your app here)</dt>
            <dd class="font-mono text-zinc-200 mt-0.5">127.0.0.1 : {{ SMTP }}</dd>
          </div>
          <div class="rounded border border-zinc-800 px-3 py-2">
            <dt class="text-[11px] uppercase tracking-wide text-zinc-600">Web inbox</dt>
            <dd class="font-mono text-zinc-200 mt-0.5">localhost : {{ WEB }}</dd>
          </div>
        </dl>

        <div class="flex items-center gap-2">
          <button @click="openInbox" :disabled="!mailpit.running"
            class="text-sm px-3 py-1.5 rounded bg-blue-600 hover:bg-blue-500 text-white disabled:opacity-40">Open inbox ↗</button>
          <template v-if="mailpit.manageable">
            <button v-if="!mailpit.running" @click="store.act(mailpit.id, 'start')" :disabled="store.busy.has(mailpit.id)"
              class="text-sm px-3 py-1.5 rounded bg-emerald-600 hover:bg-emerald-500 text-white disabled:opacity-40">Start</button>
            <button v-else @click="store.act(mailpit.id, 'stop')" :disabled="store.busy.has(mailpit.id)"
              class="text-sm px-3 py-1.5 rounded border border-red-900/70 text-red-400 hover:bg-red-950/60 disabled:opacity-40">Stop</button>
          </template>
          <span v-if="mailpit.version" class="ml-auto text-[11px] font-mono text-emerald-400/90">{{ mailpit.version }}</span>
        </div>
      </div>
    </template>
  </div>
</template>
