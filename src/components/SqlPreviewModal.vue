<script setup lang="ts">
defineProps<{ statements: string[]; executable?: boolean }>();
const emit = defineEmits<{ close: []; execute: [] }>();
</script>

<template>
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60" @click.self="emit('close')">
    <div class="w-[40rem] max-h-[70vh] rounded border border-zinc-700 bg-zinc-900 p-4 flex flex-col">
      <div class="text-sm font-medium mb-2">SQL to be executed</div>
      <pre class="flex-1 overflow-auto text-[12px] font-mono bg-zinc-950 rounded p-3 whitespace-pre-wrap">{{ statements.join(";\n") + ";" }}</pre>
      <div class="flex justify-end gap-2 mt-3">
        <button @click="emit('close')" class="px-3 py-1.5 rounded text-xs text-zinc-400 hover:bg-zinc-800">Close</button>
        <button v-if="executable" @click="emit('execute')"
          class="px-3 py-1.5 rounded text-xs bg-blue-600 hover:bg-blue-500 text-white">Execute</button>
      </div>
    </div>
  </div>
</template>
