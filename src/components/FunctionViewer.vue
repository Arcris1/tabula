<script setup lang="ts">
import { onBeforeUnmount, onMounted } from "vue";

defineProps<{ name: string; definition: string; loading: boolean }>();
const emit = defineEmits<{ close: [] }>();

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") emit("close");
}
onMounted(() => window.addEventListener("keydown", onKey));
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60" @click.self="emit('close')">
    <div class="w-[46rem] max-h-[80vh] rounded border border-zinc-700 bg-zinc-900 p-4 flex flex-col">
      <div class="flex items-center mb-2">
        <span class="text-sm font-medium">ƒ {{ name }}</span>
        <button @click="emit('close')" class="ml-auto text-zinc-500 hover:text-zinc-300">×</button>
      </div>
      <div v-if="loading" class="text-xs text-zinc-500 p-4">Loading…</div>
      <pre v-else-if="definition" class="flex-1 overflow-auto text-[12px] font-mono bg-zinc-950 rounded p-3 whitespace-pre-wrap">{{ definition }}</pre>
      <div v-else class="text-xs text-zinc-600 p-4">No source available for this function.</div>
    </div>
  </div>
</template>
