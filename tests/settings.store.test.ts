import { describe, it, expect, beforeEach } from "vitest";
import { setActivePinia, createPinia } from "pinia";
import { nextTick } from "vue";
import { useSettingsStore } from "../src/stores/settings";

describe("settings store", () => {
  beforeEach(() => {
    localStorage.clear();
    setActivePinia(createPinia());
  });

  it("defaults all guards on", () => {
    const s = useSettingsStore();
    expect(s.rails).toEqual({ warnNoWhere: true, warnDropTruncate: true, warnProductionWrites: true });
  });

  it("persists changes and reloads them", async () => {
    const s = useSettingsStore();
    s.rails.warnDropTruncate = false;
    await nextTick();
    expect(JSON.parse(localStorage.getItem("dbclient.settings")!).warnDropTruncate).toBe(false);

    setActivePinia(createPinia());
    const s2 = useSettingsStore();
    expect(s2.rails.warnDropTruncate).toBe(false);
    expect(s2.rails.warnNoWhere).toBe(true);
  });

  it("survives corrupted storage", () => {
    localStorage.setItem("dbclient.settings", "{not json");
    setActivePinia(createPinia());
    const s = useSettingsStore();
    expect(s.rails.warnNoWhere).toBe(true);
  });
});
