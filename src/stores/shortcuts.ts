import { defineStore } from "pinia";

type Handler = () => void;

export const useShortcutsStore = defineStore("shortcuts", () => {
  const stacks = new Map<string, Handler[]>();

  /** Registers a handler; the LAST registered wins. Returns an unregister fn. */
  function register(action: string, fn: Handler): () => void {
    const stack = stacks.get(action) ?? [];
    stack.push(fn);
    stacks.set(action, stack);
    return () => {
      const s = stacks.get(action);
      if (!s) return;
      const i = s.indexOf(fn);
      if (i >= 0) s.splice(i, 1);
      if (!s.length) stacks.delete(action);
    };
  }

  /** Invokes the most recent handler for the action. Returns whether one ran. */
  function dispatch(action: string): boolean {
    const stack = stacks.get(action);
    if (!stack?.length) return false;
    stack[stack.length - 1]();
    return true;
  }

  return { register, dispatch };
});
