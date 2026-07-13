import { defineStore } from "pinia";
import { ref, watch } from "vue";

const KEY = "dbclient.panelSizes";

/** User-adjustable panel sizes (px), persisted across sessions. */
export const usePanelsStore = defineStore("panels", () => {
  const sidebarW = ref(224); // schema sidebar width  (w-56)
  const sidePanelW = ref(288); // saved/history panel width (w-72)
  const inspectorW = ref(288); // row inspector width  (w-72)
  const sqlPanelH = ref(160); // data-grid SQL panel height (h-40)
  const editorH = ref(192); // query editor height  (h-48)

  const refs = { sidebarW, sidePanelW, inspectorW, sqlPanelH, editorH };

  try {
    const raw = localStorage.getItem(KEY);
    if (raw) {
      const o = JSON.parse(raw);
      for (const [k, r] of Object.entries(refs)) {
        if (typeof o[k] === "number" && o[k] > 0) r.value = o[k];
      }
    }
  } catch {
    /* corrupted -> defaults */
  }

  watch(
    () => Object.fromEntries(Object.entries(refs).map(([k, r]) => [k, r.value])),
    (v) => {
      try {
        localStorage.setItem(KEY, JSON.stringify(v));
      } catch {
        /* storage unavailable */
      }
    },
    { deep: true },
  );

  return { sidebarW, sidePanelW, inspectorW, sqlPanelH, editorH };
});
