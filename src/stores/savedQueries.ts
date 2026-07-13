import { defineStore } from "pinia";
import { ref, watch } from "vue";

const KEY = "dbclient.savedQueries";

export interface SavedQuery {
  id: string;
  name: string;
  sql: string;
  createdAtMs: number;
}

export const useSavedQueriesStore = defineStore("savedQueries", () => {
  const queries = ref<SavedQuery[]>([]);

  try {
    const raw = localStorage.getItem(KEY);
    if (raw) queries.value = JSON.parse(raw);
  } catch {
    /* corrupted -> empty */
  }

  watch(queries, (v) => {
    try {
      localStorage.setItem(KEY, JSON.stringify(v));
    } catch {
      /* storage unavailable */
    }
  }, { deep: true });

  function save(name: string, sql: string) {
    // upsert: overwrite an existing saved query with the same name instead of
    // creating a duplicate
    const existing = queries.value.find((q) => q.name === name);
    if (existing) existing.sql = sql;
    else queries.value.unshift({ id: crypto.randomUUID(), name, sql, createdAtMs: Date.now() });
  }
  function remove(id: string) {
    queries.value = queries.value.filter((q) => q.id !== id);
  }
  /** Whether some saved query has exactly this SQL (ignoring surrounding whitespace). */
  function isSaved(sql: string): boolean {
    const s = sql.trim();
    return s.length > 0 && queries.value.some((q) => q.sql.trim() === s);
  }
  function rename(id: string, name: string) {
    const q = queries.value.find((x) => x.id === id);
    if (q) q.name = name;
  }

  return { queries, save, remove, rename, isSaved };
});
