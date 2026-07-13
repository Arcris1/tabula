<script setup lang="ts">
defineProps<{ title: string; message: string; confirmLabel?: string; danger?: boolean }>();
const emit = defineEmits<{ confirm: []; cancel: [] }>();
</script>

<template>
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60" @click.self="emit('cancel')">
    <div class="w-96 rounded border border-zinc-700 bg-zinc-900 p-4">
      <div class="text-sm font-medium mb-1">{{ title }}</div>
      <div class="text-xs text-zinc-400 mb-4 whitespace-pre-wrap break-words max-h-48 overflow-auto">{{ message }}</div>
      <div class="flex justify-end gap-2">
        <button @click="emit('cancel')" class="px-3 py-1.5 rounded text-xs text-zinc-400 hover:bg-zinc-800">Cancel</button>
        <button @click="emit('confirm')" class="px-3 py-1.5 rounded text-xs text-white"
          :class="danger ? 'bg-red-600 hover:bg-red-500' : 'bg-blue-600 hover:bg-blue-500'">
          {{ confirmLabel ?? "Confirm" }}
        </button>
      </div>
    </div>
  </div>
</template>
