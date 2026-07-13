<script setup lang="ts">
import { onBeforeUnmount, onMounted } from "vue";
import JsonNode from "./JsonNode";

const props = defineProps<{ value: unknown; title?: string }>();
const emit = defineEmits<{ close: [] }>();

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") emit("close");
}
onMounted(() => window.addEventListener("keydown", onKey));
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));

function copyAll() {
  navigator.clipboard?.writeText(JSON.stringify(props.value, null, 2));
}
</script>

<template>
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60" @click.self="emit('close')">
    <div class="w-[40rem] max-h-[75vh] rounded border border-zinc-700 bg-zinc-900 flex flex-col">
      <div class="flex items-center gap-2 px-3 py-2 border-b border-zinc-800 text-xs">
        <span class="font-medium">{{ title ?? "JSON" }}</span>
        <button @click="copyAll" class="ml-auto text-zinc-500 hover:text-zinc-300">Copy</button>
        <button @click="emit('close')" class="text-zinc-500 hover:text-zinc-300">×</button>
      </div>
      <div class="flex-1 overflow-auto p-2">
        <JsonNode :value="value" :depth="0" />
      </div>
    </div>
  </div>
</template>
