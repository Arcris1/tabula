<script setup lang="ts">
import { computed, nextTick, onMounted, ref } from "vue";

const props = defineProps<{
  title: string;
  message: string;
  /** the exact name the user must type to enable the destructive button */
  expected: string;
  confirmLabel?: string;
}>();
const emit = defineEmits<{ confirm: []; cancel: [] }>();

const typed = ref("");
const input = ref<HTMLInputElement | null>(null);
const matches = computed(() => typed.value === props.expected);

onMounted(() => nextTick(() => input.value?.focus()));

function submit() {
  if (matches.value) emit("confirm");
}
</script>

<template>
  <div class="danger-overlay" @click.self="emit('cancel')" @keydown.esc="emit('cancel')">
    <div class="danger-card" role="dialog" aria-modal="true">
      <div class="danger-head">
        <span class="danger-icon">⚠</span>
        <h3>{{ title }}</h3>
      </div>
      <p class="danger-msg">{{ message }}</p>
      <label class="danger-label">
        Type <code>{{ expected }}</code> to confirm
        <input
          ref="input"
          v-model="typed"
          type="text"
          autocomplete="off"
          spellcheck="false"
          @keydown.enter="submit"
        />
      </label>
      <div class="danger-actions">
        <button class="btn-cancel" @click="emit('cancel')">Cancel</button>
        <button class="btn-danger" :disabled="!matches" @click="submit">
          {{ confirmLabel ?? "Delete" }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.danger-overlay {
  position: fixed;
  inset: 0;
  z-index: 1000;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.55);
}
.danger-card {
  width: 420px;
  max-width: 90vw;
  background: var(--panel, #1b1d23);
  border: 1px solid var(--border, #2c2f36);
  border-radius: 10px;
  padding: 20px;
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5);
}
.danger-head {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 10px;
}
.danger-icon {
  color: #e5484d;
  font-size: 18px;
}
.danger-head h3 {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
}
.danger-msg {
  margin: 0 0 14px;
  font-size: 13px;
  line-height: 1.5;
  color: var(--text-muted, #9aa0a6);
  white-space: pre-wrap;
}
.danger-label {
  display: block;
  font-size: 12px;
  color: var(--text-muted, #9aa0a6);
}
.danger-label code {
  color: #e5484d;
  font-weight: 600;
}
.danger-label input {
  display: block;
  width: 100%;
  margin-top: 6px;
  padding: 8px 10px;
  background: var(--input-bg, #12141a);
  border: 1px solid var(--border, #2c2f36);
  border-radius: 6px;
  color: var(--text, #e6e6e6);
  font-family: var(--mono, monospace);
  font-size: 13px;
}
.danger-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 18px;
}
.danger-actions button {
  padding: 7px 14px;
  border-radius: 6px;
  font-size: 13px;
  cursor: pointer;
  border: 1px solid transparent;
}
.btn-cancel {
  background: transparent;
  border-color: var(--border, #2c2f36);
  color: var(--text, #e6e6e6);
}
.btn-danger {
  background: #e5484d;
  color: #fff;
}
.btn-danger:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
</style>
