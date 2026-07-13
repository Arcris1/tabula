import { defineStore } from "pinia";
import { reactive, watch } from "vue";
import type { RailSettings } from "../lib/railChecks";

const KEY = "dbclient.settings";

const DEFAULTS: RailSettings = {
  warnNoWhere: true,
  warnDropTruncate: true,
  warnProductionWrites: true,
};

export const useSettingsStore = defineStore("settings", () => {
  const rails = reactive<RailSettings>({ ...DEFAULTS });

  try {
    const raw = localStorage.getItem(KEY);
    if (raw) Object.assign(rails, { ...DEFAULTS, ...JSON.parse(raw) });
  } catch {
    /* corrupted settings fall back to defaults */
  }

  watch(rails, (v) => {
    try {
      localStorage.setItem(KEY, JSON.stringify(v));
    } catch {
      /* storage unavailable */
    }
  });

  return { rails };
});
