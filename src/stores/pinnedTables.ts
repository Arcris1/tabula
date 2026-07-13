import { defineStore } from "pinia";
import { ref, watch } from "vue";

const KEY = "dbclient.pinnedTables";

/** Pinned table keys per connection id, persisted across sessions. */
export const usePinnedTablesStore = defineStore("pinnedTables", () => {
  const byConn = ref<Record<string, string[]>>({});

  try {
    const raw = localStorage.getItem(KEY);
    if (raw) byConn.value = JSON.parse(raw);
  } catch {
    /* corrupted -> empty */
  }

  watch(byConn, (v) => {
    try {
      localStorage.setItem(KEY, JSON.stringify(v));
    } catch {
      /* storage unavailable */
    }
  }, { deep: true });

  function list(connId: string): string[] {
    return byConn.value[connId] ?? [];
  }
  function isPinned(connId: string, tableKey: string): boolean {
    return list(connId).includes(tableKey);
  }
  function toggle(connId: string, tableKey: string) {
    const cur = list(connId);
    byConn.value[connId] = cur.includes(tableKey)
      ? cur.filter((k) => k !== tableKey)
      : [...cur, tableKey];
  }

  return { byConn, list, isPinned, toggle };
});
