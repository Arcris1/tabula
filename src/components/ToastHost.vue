<script setup lang="ts">
import { useToastStore } from "../stores/toast";
const toast = useToastStore();
const styles: Record<string, string> = {
  success: "border-emerald-800/70 bg-emerald-950/80 text-emerald-200",
  error: "border-red-800/70 bg-red-950/85 text-red-200",
  info: "border-zinc-700 bg-zinc-900/95 text-zinc-200",
};
const icons: Record<string, string> = { success: "✓", error: "✕", info: "i" };
</script>

<template>
  <div class="fixed bottom-3 right-3 z-[100] flex flex-col gap-2 items-end pointer-events-none" aria-live="polite">
    <div v-for="t in toast.toasts" :key="t.id" role="status"
      class="pointer-events-auto flex items-start gap-2 max-w-sm rounded-lg border px-3 py-2 text-xs shadow-lg"
      :class="styles[t.kind]">
      <span class="mt-px font-semibold leading-none" aria-hidden="true">{{ icons[t.kind] }}</span>
      <span class="whitespace-pre-wrap break-words flex-1">{{ t.message }}</span>
      <button @click="toast.dismiss(t.id)" class="text-current/60 hover:text-current" aria-label="Dismiss">×</button>
    </div>
  </div>
</template>
