import { computed, reactive, ref } from "vue";
import type { Changeset } from "./api";

interface RowEdits {
  pk: Record<string, unknown>;
  changes: Record<string, { old: unknown; new: unknown }>;
}

export type ChangesetState = ReturnType<typeof createChangeset>;

/**
 * Pending-edit state for one grid (Data tab or an editable query result).
 * Each consumer creates its own instance so edits never collide.
 */
export function createChangeset() {
  const pkColumns = ref<string[]>([]);
  const updates = reactive(new Map<string, RowEdits>());
  const deletes = reactive(new Map<string, Record<string, unknown>>());
  const drafts = ref<Record<string, unknown>[]>([]);

  const editable = computed(() => pkColumns.value.length > 0);
  const count = computed(() => updates.size + deletes.size + drafts.value.length);

  function rowKey(row: unknown[], columns: string[]): string {
    return JSON.stringify(pkColumns.value.map((c) => row[columns.indexOf(c)]));
  }
  function pkOf(row: unknown[], columns: string[]): Record<string, unknown> {
    return Object.fromEntries(pkColumns.value.map((c) => [c, row[columns.indexOf(c)]]));
  }

  function setContext(pk: string[]) {
    pkColumns.value = pk;
    clear();
  }

  function editCell(row: unknown[], columns: string[], column: string, value: unknown) {
    const key = rowKey(row, columns);
    const original = row[columns.indexOf(column)];
    let entry = updates.get(key);
    if (JSON.stringify(value) === JSON.stringify(original)) {
      if (entry) {
        delete entry.changes[column];
        if (!Object.keys(entry.changes).length) updates.delete(key);
      }
      return;
    }
    if (!entry) {
      entry = { pk: pkOf(row, columns), changes: {} };
      updates.set(key, entry);
    }
    entry.changes[column] = { old: original, new: value };
  }

  function cellValue(row: unknown[], columns: string[], column: string) {
    const entry = updates.get(rowKey(row, columns));
    const c = entry?.changes[column];
    return c ? { value: c.new, dirty: true } : { value: row[columns.indexOf(column)], dirty: false };
  }

  function markDelete(row: unknown[], columns: string[]) {
    const key = rowKey(row, columns);
    if (deletes.has(key)) deletes.delete(key);
    else deletes.set(key, pkOf(row, columns));
  }
  function isDeleted(row: unknown[], columns: string[]) {
    return deletes.has(rowKey(row, columns));
  }

  function addDraft(_columns: string[]): Record<string, unknown> {
    const d = reactive({} as Record<string, unknown>);
    drafts.value.push(d);
    return drafts.value[drafts.value.length - 1];
  }
  function removeDraft(i: number) {
    drafts.value.splice(i, 1);
  }

  function toChangeset(): Changeset {
    return {
      updates: [...updates.values()].map((e) => ({
        pk: e.pk,
        changes: Object.entries(e.changes).map(([column, v]) => ({ column, old: v.old, new: v.new })),
      })),
      deletes: [...deletes.values()].map((pk) => ({ pk })),
      inserts: drafts.value.map((d) =>
        Object.fromEntries(Object.entries(d).filter(([, v]) => v !== undefined && v !== "")),
      ),
    };
  }

  function clear() {
    updates.clear();
    deletes.clear();
    drafts.value = [];
  }

  return {
    pkColumns, editable, count, drafts,
    setContext, editCell, cellValue, markDelete, isDeleted,
    addDraft, removeDraft, toChangeset, clear,
  };
}
