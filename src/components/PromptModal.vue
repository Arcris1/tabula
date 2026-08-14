<script setup lang="ts">
import { nextTick, onMounted, ref } from "vue";
import BaseModal from "./BaseModal.vue";

const props = defineProps<{
  title: string;
  label?: string;
  initial?: string;
  placeholder?: string;
  confirmLabel?: string;
}>();
const emit = defineEmits<{ submit: [value: string]; cancel: [] }>();

const value = ref(props.initial ?? "");
const input = ref<HTMLInputElement | null>(null);

function submit() {
  const v = value.value.trim();
  if (!v) return;
  emit("submit", v);
}
onMounted(() => nextTick(() => { input.value?.focus(); input.value?.select(); }));
</script>

<template>
  <BaseModal size="sm" labelled-by="prompt-title" @cancel="emit('cancel')">
    <div class="p-4">
      <div id="prompt-title" class="text-sm font-medium mb-2">{{ title }}</div>
      <label v-if="label" class="block text-xs text-zinc-400 mb-1">{{ label }}</label>
      <input ref="input" v-model="value" :placeholder="placeholder" data-autofocus
        @keydown.enter.prevent="submit"
        class="w-full bg-zinc-950 border border-zinc-700 rounded px-2 py-1.5 text-sm outline-none focus:border-blue-500 placeholder:text-zinc-500" />
      <div class="flex justify-end gap-2 mt-4">
        <button @click="emit('cancel')" class="px-3 py-1.5 rounded text-xs text-zinc-300 border border-zinc-700 hover:bg-zinc-800">Cancel</button>
        <button @click="submit" :disabled="!value.trim()"
          class="px-3 py-1.5 rounded text-xs text-white bg-blue-600 hover:bg-blue-500 disabled:opacity-40 disabled:cursor-not-allowed">
          {{ confirmLabel ?? "OK" }}
        </button>
      </div>
    </div>
  </BaseModal>
</template>
