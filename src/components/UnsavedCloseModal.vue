<script setup lang="ts">
import { nextTick, onMounted, ref } from "vue";
import BaseModal from "./BaseModal.vue";

const props = defineProps<{ initialName: string }>();
const emit = defineEmits<{ save: [name: string]; discard: []; cancel: [] }>();

const name = ref(props.initialName);
const input = ref<HTMLInputElement | null>(null);

function save() {
  const v = name.value.trim();
  if (!v) return;
  emit("save", v);
}
onMounted(() => nextTick(() => { input.value?.focus(); input.value?.select(); }));
</script>

<template>
  <BaseModal size="sm" labelled-by="unsaved-title" @cancel="emit('cancel')">
    <div class="p-4">
      <div id="unsaved-title" class="text-sm font-medium mb-1">Unsaved query</div>
      <div class="text-xs text-zinc-300 mb-3">
        This query hasn't been saved. Save it before closing, or close without saving?
      </div>
      <label class="block text-xs text-zinc-400 mb-1">Name</label>
      <input ref="input" v-model="name" placeholder="e.g. stuck leads last 24h" data-autofocus
        @keydown.enter.prevent="save"
        class="w-full bg-zinc-950 border border-zinc-700 rounded px-2 py-1.5 text-sm outline-none focus:border-blue-500 placeholder:text-zinc-500" />
      <div class="flex justify-end gap-2 mt-4">
        <button @click="emit('cancel')" class="px-3 py-1.5 rounded text-xs text-zinc-300 border border-zinc-700 hover:bg-zinc-800">Cancel</button>
        <button @click="emit('discard')" class="px-3 py-1.5 rounded text-xs text-red-400 hover:bg-red-950/60">Don't save</button>
        <button @click="save" :disabled="!name.trim()"
          class="px-3 py-1.5 rounded text-xs text-white bg-blue-600 hover:bg-blue-500 disabled:opacity-40 disabled:cursor-not-allowed">Save &amp; close</button>
      </div>
    </div>
  </BaseModal>
</template>
