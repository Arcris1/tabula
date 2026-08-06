<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { api, type RedisEntry, type RedisValue } from "../lib/api";
import { tryPhpFormat } from "../lib/phpSerialize";
import { useConnectionsStore } from "../stores/connections";
import { useSettingsStore } from "../stores/settings";
import { useWorkspaceStore } from "../stores/workspace";
import ConfirmModal from "./ConfirmModal.vue";
import JsonNode from "./JsonNode";

const props = defineProps<{ entry: RedisEntry | null }>();
const emit = defineEmits<{ changed: []; deleted: [] }>();
const ws = useWorkspaceStore();
const conns = useConnectionsStore();
const settings = useSettingsStore();

/** Production write gate, shared by Save/TTL/Persist (Delete has its own confirm). */
const pendingWrite = ref<null | (() => Promise<void>)>(null);
function guardProd(action: () => Promise<void>) {
  const prod = conns.connections.find((c) => c.id === ws.activeId)?.isProduction;
  if (prod && settings.rails.warnProductionWrites) pendingWrite.value = action;
  else action();
}
function confirmWrite() {
  const a = pendingWrite.value;
  pendingWrite.value = null;
  a?.();
}

const editing = ref(false);
const editText = ref("");
const parseError = ref<string | null>(null);
const ttlInput = ref<number | null>(null);
const confirmDelete = ref(false);

watch(() => props.entry, () => {
  editing.value = false;
  parseError.value = null;
  showRaw.value = false;
  ttlInput.value = props.entry && props.entry.ttlSecs > 0 ? props.entry.ttlSecs : null;
});

function ttlText(ttl: number): string {
  if (ttl === -1) return "no TTL";
  if (ttl === -2) return "missing";
  return `TTL ${ttl}s`;
}

function shownCount(e: RedisEntry): number {
  const v = e.value;
  if (v.type === "list") return v.items.length;
  if (v.type === "zSet") return v.members.length;
  return 0;
}
/** Save would rewrite the key from a partial view — editing must be blocked. */
function isTruncated(e: RedisEntry): boolean {
  return e.totalItems != null && e.totalItems > shownCount(e);
}
function looksJson(s: string): boolean {
  const t = s.trim();
  return (t.startsWith("{") && t.endsWith("}")) || (t.startsWith("[") && t.endsWith("]"));
}
function pretty(s: string): string {
  try {
    return JSON.stringify(JSON.parse(s), null, 2);
  } catch {
    return s;
  }
}

// PHP-serialized values (Laravel sessions/cache) render as a collapsible tree
const showRaw = ref(false);
const phpParsed = computed<unknown | null>(() =>
  props.entry?.value.type === "string" ? tryPhpFormat(props.entry.value.value) : null,
);

function encode(entry: RedisEntry): string {
  const v = entry.value;
  if (v.type === "string") return v.value;
  if (v.type === "list") return v.items.join("\n");
  if (v.type === "set") return v.members.join("\n");
  if (v.type === "hash") return JSON.stringify(Object.fromEntries(v.fields), null, 2);
  return JSON.stringify(Object.fromEntries(v.members), null, 2); // zset
}
function decode(text: string, type: RedisValue["type"]): RedisValue {
  if (type === "string") return { type, value: text };
  if (type === "list") return { type, items: text.split("\n").filter((l) => l.length) };
  if (type === "set") return { type, members: text.split("\n").filter((l) => l.length) };
  if (type === "hash") return { type, fields: Object.entries(JSON.parse(text)).map(([k, v]) => [k, String(v)]) };
  return { type: "zSet", members: Object.entries(JSON.parse(text)).map(([k, v]) => [k, Number(v)]) };
}

function beginEdit() {
  if (!props.entry || isTruncated(props.entry)) return;
  editText.value = encode(props.entry);
  parseError.value = null;
  editing.value = true;
}
function save() {
  if (!props.entry || !ws.activeId) return;
  guardProd(async () => {
    parseError.value = null;
    try {
      const value = decode(editText.value, props.entry!.value.type);
      await api.setRedisValue(ws.activeId!, props.entry!.key, value);
      editing.value = false;
      emit("changed");
    } catch (e: any) {
      parseError.value = e?.message ?? String(e);
    }
  });
}
function applyTtl() {
  if (!props.entry || !ws.activeId) return;
  guardProd(async () => {
    await api.setRedisTtl(ws.activeId!, props.entry!.key, ttlInput.value ?? 0);
    emit("changed");
  });
}
function persist() {
  if (!props.entry || !ws.activeId) return;
  guardProd(async () => {
    await api.setRedisTtl(ws.activeId!, props.entry!.key, 0);
    emit("changed");
  });
}
async function doDelete() {
  if (!props.entry || !ws.activeId) return;
  await api.deleteRedisKey(ws.activeId, props.entry.key);
  confirmDelete.value = false;
  emit("deleted");
}
</script>

<template>
  <section class="flex-1 min-w-0 flex flex-col">
    <div v-if="!entry" class="flex-1 flex items-center justify-center text-zinc-600 text-xs">Select a key</div>
    <template v-else>
      <header class="h-8 shrink-0 flex items-center gap-2 px-3 border-b border-zinc-800 text-xs">
        <span class="font-mono truncate">{{ entry.key }}</span>
        <span class="text-zinc-500 uppercase text-[10px] border border-zinc-800 rounded px-1">{{ entry.value.type }}</span>
        <span v-if="isTruncated(entry)"
          class="text-[10px] text-amber-400 border border-amber-900 rounded px-1"
          title="Only the first items are shown; editing is disabled because saving would delete the rest.">
          showing {{ shownCount(entry) }} of {{ entry.totalItems }}
        </span>
        <span class="ml-auto text-zinc-500">{{ ttlText(entry.ttlSecs) }}</span>
        <button v-if="!editing" @click="beginEdit" :disabled="isTruncated(entry)"
          :title="isTruncated(entry) ? 'Editing disabled: value is truncated — saving would lose data' : ''"
          class="px-2 py-0.5 rounded border border-zinc-700 hover:bg-zinc-800 disabled:opacity-40 disabled:cursor-not-allowed">Edit</button>
        <button @click="confirmDelete = true"
          class="px-2 py-0.5 rounded border border-red-900 text-red-400 hover:bg-red-950">Delete</button>
      </header>

      <div class="h-8 shrink-0 flex items-center gap-2 px-3 border-b border-zinc-800 text-[11px] text-zinc-500">
        <span>TTL</span>
        <input v-model.number="ttlInput" type="number" placeholder="seconds"
          class="w-24 bg-zinc-900 border border-zinc-800 rounded px-1 py-0.5 outline-none focus:border-zinc-600" />
        <button @click="applyTtl" class="px-2 py-0.5 rounded border border-zinc-700 hover:bg-zinc-800">Set</button>
        <button @click="persist" class="px-2 py-0.5 rounded border border-zinc-700 hover:bg-zinc-800">Persist</button>
      </div>

      <div v-if="editing" class="flex-1 flex flex-col p-3 gap-2">
        <div class="text-[11px] text-zinc-500">
          {{ entry.value.type === "string" ? "raw text"
            : entry.value.type === "hash" ? "JSON object {\"field\": \"value\"}"
            : entry.value.type === "zSet" ? "JSON object {\"member\": score}"
            : "one member per line" }}
        </div>
        <textarea v-model="editText"
          class="flex-1 bg-zinc-950 border border-zinc-800 rounded p-2 font-mono text-[12px] outline-none focus:border-zinc-600 resize-none" />
        <div v-if="parseError" class="text-xs text-red-400">{{ parseError }}</div>
        <div class="flex gap-2 justify-end">
          <button @click="editing = false" class="px-3 py-1.5 rounded text-xs text-zinc-400 hover:bg-zinc-800">Cancel</button>
          <button @click="save" class="px-3 py-1.5 rounded text-xs bg-blue-600 hover:bg-blue-500 text-white">Save</button>
        </div>
      </div>

      <div v-else class="flex-1 overflow-auto p-3 font-mono text-[12px]">
        <template v-if="entry.value.type === 'string'">
          <div v-if="phpParsed !== null" class="flex items-center gap-2 mb-2 font-sans">
            <span class="text-[10px] uppercase tracking-wide text-indigo-300 border border-indigo-900 rounded px-1"
              title="PHP serialize() format — a Laravel session/cache payload">php serialized</span>
            <button @click="showRaw = !showRaw"
              class="text-[11px] px-2 py-0.5 rounded border border-zinc-700 text-zinc-400 hover:bg-zinc-800">
              {{ showRaw ? "Formatted" : "Raw" }}
            </button>
          </div>
          <JsonNode v-if="phpParsed !== null && !showRaw" :value="phpParsed" :depth="0" />
          <pre v-else class="whitespace-pre-wrap">{{
            looksJson(entry.value.value) ? pretty(entry.value.value) : entry.value.value
          }}</pre>
        </template>
        <table v-else-if="entry.value.type === 'hash'" class="border-collapse">
          <tr v-for="[f, v] in entry.value.fields" :key="f">
            <td class="pr-4 py-0.5 text-zinc-400 border-b border-zinc-900 align-top">{{ f }}</td>
            <td class="py-0.5 border-b border-zinc-900 whitespace-pre-wrap">{{ v }}</td>
          </tr>
        </table>
        <ol v-else-if="entry.value.type === 'list'" class="list-decimal list-inside">
          <li v-for="(item, i) in entry.value.items" :key="i" class="py-0.5 border-b border-zinc-900">{{ item }}</li>
        </ol>
        <ul v-else-if="entry.value.type === 'set'">
          <li v-for="m in entry.value.members" :key="m" class="py-0.5 border-b border-zinc-900">{{ m }}</li>
        </ul>
        <table v-else-if="entry.value.type === 'zSet'" class="border-collapse">
          <tr v-for="[m, score] in entry.value.members" :key="m">
            <td class="pr-4 py-0.5 text-zinc-400 border-b border-zinc-900">{{ score }}</td>
            <td class="py-0.5 border-b border-zinc-900">{{ m }}</td>
          </tr>
        </table>
      </div>

      <ConfirmModal v-if="confirmDelete" title="Delete key"
        :message="`Permanently delete ${entry.key}?`" confirm-label="Delete" danger
        @confirm="doDelete" @cancel="confirmDelete = false" />
      <ConfirmModal v-if="pendingWrite" title="Write on PRODUCTION"
        :message="`Modify ${entry.key} on a production database?`" confirm-label="Apply" danger
        @confirm="confirmWrite" @cancel="pendingWrite = null" />
    </template>
  </section>
</template>
