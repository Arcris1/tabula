<script setup lang="ts">
import { onBeforeUnmount, onMounted } from "vue";
import { detectPlatform, displayCombo, SHORTCUT_TABLE } from "../lib/keymap";

const emit = defineEmits<{ close: [] }>();
const platform = detectPlatform();

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") emit("close");
}
onMounted(() => window.addEventListener("keydown", onKey));
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60" @click.self="emit('close')">
    <div class="w-[30rem] max-h-[75vh] rounded border border-zinc-700 bg-zinc-900 p-4 flex flex-col">
      <div class="text-sm font-medium mb-3">Keyboard shortcuts</div>
      <div class="flex-1 overflow-auto">
        <table class="w-full text-xs">
          <tbody>
            <tr v-for="row in SHORTCUT_TABLE" :key="row.action" class="border-t border-zinc-800/60">
              <td class="py-1.5 pr-3 text-zinc-300">{{ row.label }}</td>
              <td class="py-1.5 pr-3 text-zinc-600">{{ row.context }}</td>
              <td class="py-1.5 text-right">
                <kbd class="px-1.5 py-0.5 rounded bg-zinc-800 border border-zinc-700 font-mono text-[11px]">
                  {{ row.combo.includes("…") ? row.combo.replace("mod", platform === "mac" ? "⌘" : "Ctrl") : displayCombo(row.combo, platform) }}
                </kbd>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <div class="flex justify-end mt-3">
        <button @click="emit('close')" class="px-3 py-1.5 rounded text-xs text-zinc-400 hover:bg-zinc-800">Close</button>
      </div>
    </div>
  </div>
</template>
