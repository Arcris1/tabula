<script setup lang="ts">
import BaseModal from "./BaseModal.vue";
const props = defineProps<{ title: string; message: string; confirmLabel?: string; danger?: boolean }>();
const emit = defineEmits<{ confirm: []; cancel: [] }>();
</script>

<template>
  <BaseModal size="sm" enter-confirms labelled-by="confirm-title"
    @confirm="emit('confirm')" @cancel="emit('cancel')">
    <div class="p-4">
      <div id="confirm-title" class="text-sm font-medium mb-1">{{ title }}</div>
      <div class="text-xs text-zinc-300 mb-4 whitespace-pre-wrap break-words max-h-48 overflow-auto">{{ message }}</div>
      <div class="flex justify-end gap-2">
        <button @click="emit('cancel')" :data-autofocus="props.danger ? '' : undefined"
          class="px-3 py-1.5 rounded text-xs text-zinc-300 border border-zinc-700 hover:bg-zinc-800">Cancel</button>
        <button @click="emit('confirm')" :data-autofocus="props.danger ? undefined : ''"
          class="px-3 py-1.5 rounded text-xs text-white"
          :class="danger ? 'bg-red-600 hover:bg-red-500' : 'bg-blue-600 hover:bg-blue-500'">
          {{ confirmLabel ?? "Confirm" }}
        </button>
      </div>
    </div>
  </BaseModal>
</template>
