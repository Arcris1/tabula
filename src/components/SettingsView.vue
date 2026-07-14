<script setup lang="ts">
import { computed, onMounted } from "vue";
import { useServicesStore } from "../stores/services";

const store = useServicesStore();
onMounted(() => { if (!store.services.length) store.load(); });

const platform = computed(() => {
  const p = navigator.platform + navigator.userAgent;
  if (/Mac/i.test(p)) return "macOS";
  if (/Win/i.test(p)) return "Windows";
  return "Linux";
});
const managed = computed(() => store.services.filter((s) => s.manageable).length);
const manager = computed(() => {
  const m = store.services.find((s) => s.manageable)?.manager;
  return m === "brew" ? "Homebrew (brew services)"
    : m === "winservice" ? "Windows Services" : "—";
});
</script>

<template>
  <div class="flex-1 overflow-y-auto p-5">
    <div class="max-w-lg">
      <h2 class="text-sm font-medium text-zinc-200 mb-3">Environment</h2>
      <dl class="rounded-lg border border-zinc-800 divide-y divide-zinc-800/70 text-sm">
        <div class="flex px-3 py-2.5"><dt class="text-zinc-500 w-40">Platform</dt><dd class="text-zinc-200">{{ platform }}</dd></div>
        <div class="flex px-3 py-2.5"><dt class="text-zinc-500 w-40">Service manager</dt><dd class="text-zinc-200">{{ manager }}</dd></div>
        <div class="flex px-3 py-2.5"><dt class="text-zinc-500 w-40">Managed services</dt><dd class="text-zinc-200">{{ managed }}</dd></div>
      </dl>

      <div class="mt-6 pt-4 border-t border-zinc-800 text-[11px] text-zinc-600 flex items-center justify-between">
        <span>Tabula — local stack</span>
        <span>Created by Arcris Silang</span>
      </div>
    </div>
  </div>
</template>
