<script setup lang="ts">
import { computed, ref } from "vue";
import BaseModal from "./BaseModal.vue";

const props = defineProps<{
  title: string;
  message: string;
  /** the exact name the user must type to enable the destructive button */
  expected: string;
  confirmLabel?: string;
}>();
const emit = defineEmits<{ confirm: []; cancel: [] }>();

const typed = ref("");
const matches = computed(() => typed.value === props.expected);
function submit() { if (matches.value) emit("confirm"); }
</script>

<template>
  <BaseModal size="sm" :backdrop-cancels="true" labelled-by="danger-title"
    @cancel="emit('cancel')">
    <div class="p-5">
      <div class="flex items-center gap-2 mb-2">
        <span class="text-red-500 text-base leading-none" aria-hidden="true">⚠</span>
        <h3 id="danger-title" class="text-sm font-semibold">{{ title }}</h3>
      </div>
      <p class="text-xs text-zinc-300 leading-relaxed mb-4 whitespace-pre-wrap">{{ message }}</p>
      <label class="block text-xs text-zinc-400">
        Type <code class="text-red-400 font-semibold">{{ expected }}</code> to confirm
        <input v-model="typed" data-autofocus type="text" autocomplete="off" spellcheck="false"
          @keydown.enter="submit"
          class="mt-1.5 w-full rounded border border-zinc-700 bg-zinc-950 px-2.5 py-2 font-mono text-[13px] text-zinc-100 outline-none focus:border-red-600" />
      </label>
      <div class="flex justify-end gap-2 mt-4">
        <button @click="emit('cancel')"
          class="px-3 py-1.5 rounded text-xs text-zinc-300 border border-zinc-700 hover:bg-zinc-800">Cancel</button>
        <button :disabled="!matches" @click="submit"
          class="px-3 py-1.5 rounded text-xs text-white bg-red-600 hover:bg-red-500 disabled:opacity-40 disabled:cursor-not-allowed">
          {{ confirmLabel ?? "Delete" }}
        </button>
      </div>
    </div>
  </BaseModal>
</template>
