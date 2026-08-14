import { defineStore } from "pinia";
import { ref } from "vue";

export type ToastKind = "success" | "error" | "info";
export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
}

let seq = 0;

/** App-wide transient notifications, so success/failure feedback is consistent
 *  instead of each surface inventing its own inline banner. */
export const useToastStore = defineStore("toast", () => {
  const toasts = ref<Toast[]>([]);

  function push(kind: ToastKind, message: string, ttl = 3500) {
    const id = ++seq;
    toasts.value.push({ id, kind, message });
    // errors linger longer so they can be read
    window.setTimeout(() => dismiss(id), kind === "error" ? Math.max(ttl, 6000) : ttl);
  }
  function dismiss(id: number) {
    toasts.value = toasts.value.filter((t) => t.id !== id);
  }
  const success = (m: string) => push("success", m);
  const error = (m: string) => push("error", m);
  const info = (m: string) => push("info", m);

  return { toasts, push, dismiss, success, error, info };
});
