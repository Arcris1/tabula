<script setup lang="ts">
import BaseModal from "./BaseModal.vue";
defineProps<{ statements: string[]; executable?: boolean }>();
const emit = defineEmits<{ close: []; execute: [] }>();
</script>

<template>
  <BaseModal size="lg" labelled-by="sqlprev-title" @cancel="emit('close')">
    <div class="p-4 flex flex-col max-h-[70vh]">
      <div id="sqlprev-title" class="text-sm font-medium mb-2">SQL to be executed</div>
      <pre class="flex-1 overflow-auto text-[12px] font-mono bg-zinc-950 rounded p-3 whitespace-pre-wrap text-zinc-200">{{ statements.join(";\n") + ";" }}</pre>
      <div class="flex justify-end gap-2 mt-3">
        <button @click="emit('close')" data-autofocus class="px-3 py-1.5 rounded text-xs text-zinc-300 border border-zinc-700 hover:bg-zinc-800">Close</button>
        <button v-if="executable" @click="emit('execute')"
          class="px-3 py-1.5 rounded text-xs bg-blue-600 hover:bg-blue-500 text-white">Execute</button>
      </div>
    </div>
  </BaseModal>
</template>
