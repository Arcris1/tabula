<script setup lang="ts">
defineProps<{ name: string }>();
const emit = defineEmits<{ newTab: []; override: []; cancel: [] }>();
</script>

<template>
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60" @click.self="emit('cancel')">
    <div class="w-[26rem] rounded border border-zinc-700 bg-zinc-900 p-4">
      <div class="text-sm font-medium mb-1">Open “{{ name }}”</div>
      <div class="text-xs text-zinc-400 mb-4">
        This tab already has a different query. Open the saved query in a new tab,
        or override what's in this tab?
      </div>
      <div class="flex justify-end gap-2">
        <button @click="emit('cancel')" class="px-3 py-1.5 rounded text-xs text-zinc-400 hover:bg-zinc-800">Cancel</button>
        <button @click="emit('override')" class="px-3 py-1.5 rounded text-xs text-amber-300 hover:bg-amber-950/50 border border-amber-900/60">Override this tab</button>
        <button @click="emit('newTab')" class="px-3 py-1.5 rounded text-xs text-white bg-blue-600 hover:bg-blue-500">Open in new tab</button>
      </div>
    </div>
  </div>
</template>
