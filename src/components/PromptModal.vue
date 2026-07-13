<script setup lang="ts">
import { nextTick, onMounted, ref } from "vue";

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
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60" @click.self="emit('cancel')">
    <div class="w-96 rounded border border-zinc-700 bg-zinc-900 p-4">
      <div class="text-sm font-medium mb-2">{{ title }}</div>
      <label v-if="label" class="block text-xs text-zinc-400 mb-1">{{ label }}</label>
      <input ref="input" v-model="value" :placeholder="placeholder"
        @keydown.enter.prevent="submit" @keydown.esc.prevent="emit('cancel')"
        class="w-full bg-zinc-950 border border-zinc-700 rounded px-2 py-1.5 text-sm outline-none focus:border-blue-600" />
      <div class="flex justify-end gap-2 mt-4">
        <button @click="emit('cancel')" class="px-3 py-1.5 rounded text-xs text-zinc-400 hover:bg-zinc-800">Cancel</button>
        <button @click="submit" :disabled="!value.trim()"
          class="px-3 py-1.5 rounded text-xs text-white bg-blue-600 hover:bg-blue-500 disabled:opacity-40">
          {{ confirmLabel ?? "OK" }}
        </button>
      </div>
    </div>
  </div>
</template>
