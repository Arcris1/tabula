export type Platform = "mac" | "other";

export function detectPlatform(): Platform {
  const p = typeof navigator !== "undefined" ? navigator.platform + navigator.userAgent : "";
  return /Mac|iPhone|iPad/i.test(p) ? "mac" : "other";
}

/** Normalizes a KeyboardEvent to "ctrl+meta+shift+alt+key" (modifiers sorted, key lowercased). */
export function comboOf(e: KeyboardEvent): string | null {
  const key = e.key;
  if (key === "Meta" || key === "Control" || key === "Shift" || key === "Alt") return null;
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("ctrl");
  if (e.metaKey) parts.push("meta");
  if (e.altKey) parts.push("alt");
  if (e.shiftKey) parts.push("shift");
  let k = key.toLowerCase();
  if (k === " ") k = "space";
  parts.push(k);
  return parts.join("+");
}

/** Rows drive both bindings() and the help modal. combo uses "mod" (⌘/Ctrl). */
export const SHORTCUT_TABLE: { action: string; combo: string; label: string; context: string }[] = [
  { action: "launcher:new", combo: "mod+n", label: "New connection", context: "Launcher" },
  { action: "tabs:new", combo: "mod+t", label: "New query tab", context: "Workspace" },
  { action: "table:new", combo: "mod+shift+t", label: "New table", context: "Workspace" },
  { action: "tabs:close", combo: "mod+w", label: "Close query tab", context: "Workspace" },
  { action: "tabs:data", combo: "mod+1", label: "Data tab", context: "Workspace" },
  { action: "tabs:structure", combo: "mod+2", label: "Structure tab", context: "Workspace" },
  { action: "tabs:index", combo: "mod+3…9", label: "Query tab 1…7", context: "Workspace" },
  { action: "tabs:next", combo: "ctrl+tab", label: "Next tab", context: "Workspace" },
  { action: "tabs:prev", combo: "ctrl+shift+tab", label: "Previous tab", context: "Workspace" },
  { action: "view:refresh", combo: "mod+r", label: "Refresh / re-run", context: "Any tab" },
  { action: "grid:commit", combo: "mod+s", label: "Commit changes (Data) / Save query (editor)", context: "Data / Query" },
  { action: "grid:discard", combo: "mod+alt+z", label: "Discard pending changes", context: "Data" },
  { action: "grid:insertRow", combo: "mod+i", label: "Insert row", context: "Data" },
  { action: "grid:focusFilter", combo: "mod+f", label: "Filter rows", context: "Data" },
  { action: "sidebar:focusFilter", combo: "mod+p", label: "Search tables", context: "Workspace" },
  { action: "history:toggle", combo: "mod+y", label: "Query history", context: "Workspace" },
  { action: "query:run", combo: "mod+enter", label: "Run statement", context: "Query editor" },
  { action: "query:cancel", combo: "mod+.", label: "Cancel running query", context: "Query tab" },
  { action: "workspace:disconnect", combo: "mod+shift+d", label: "Disconnect", context: "Workspace" },
  { action: "window:new", combo: "mod+shift+n", label: "New window", context: "Anywhere" },
  { action: "settings:toggle", combo: "mod+,", label: "Settings", context: "Anywhere" },
  { action: "help:toggle", combo: "mod+shift+/", label: "Keyboard shortcuts (also F1)", context: "Anywhere" },
  { action: "editor:comment", combo: "mod+/", label: "Toggle comment (editor built-in)", context: "Query editor" },
];

function resolve(combo: string, platform: Platform): string {
  const mod = platform === "mac" ? "meta" : "ctrl";
  // normalize order to match comboOf: ctrl, meta, alt, shift, key
  const parts = combo.replace(/\bmod\b/, mod).split("+");
  const key = parts.pop()!;
  const order = ["ctrl", "meta", "alt", "shift"];
  const mods = order.filter((m) => parts.includes(m));
  return [...mods, key].join("+");
}

/** combo string -> action name for the given platform. */
export function bindings(platform: Platform): Record<string, string> {
  const out: Record<string, string> = {};
  const add = (combo: string, action: string) => { out[resolve(combo, platform)] = action; };
  add("mod+n", "launcher:new");
  add("mod+t", "tabs:new");
  add("mod+shift+t", "table:new");
  add("mod+w", "tabs:close");
  add("mod+1", "tabs:data");
  add("mod+2", "tabs:structure");
  for (let n = 3; n <= 9; n++) add(`mod+${n}`, `tabs:index:${n - 3}`);
  add("ctrl+tab", "tabs:next");
  add("ctrl+shift+tab", "tabs:prev");
  add("mod+shift+]", "tabs:next");
  add("mod+shift+[", "tabs:prev");
  add("mod+r", "view:refresh");
  add("f5", "view:refresh");
  add("mod+s", "grid:commit");
  add("mod+alt+z", "grid:discard");
  add("mod+i", "grid:insertRow");
  add("mod+f", "grid:focusFilter");
  add("mod+p", "sidebar:focusFilter");
  add("mod+y", "history:toggle");
  add("mod+.", "query:cancel");
  add("mod+shift+d", "workspace:disconnect");
  add("mod+shift+n", "window:new");
  add("mod+,", "settings:toggle");
  // mod+/ belongs to the editor's toggle-comment; help uses the platform Help combo
  add("mod+shift+/", "help:toggle");
  add("mod+shift+?", "help:toggle"); // shifted layouts deliver "?" as the key
  add("f1", "help:toggle");
  return out;
}

const MAC_GLYPHS: Record<string, string> = { ctrl: "⌃", meta: "⌘", alt: "⌥", shift: "⇧" };
const KEY_LABELS: Record<string, string> = { enter: "↵", tab: "Tab", escape: "Esc", " ": "Space" };

/** "mod+shift+d" -> "⌘⇧D" (mac) / "Ctrl+Shift+D" (other). */
export function displayCombo(combo: string, platform: Platform): string {
  const mod = platform === "mac" ? "meta" : "ctrl";
  const parts = combo.replace(/\bmod\b/, mod).split("+");
  const key = parts.pop()!;
  const keyLabel = KEY_LABELS[key] ?? (key.length === 1 ? key.toUpperCase() : key.toUpperCase());
  if (platform === "mac") {
    return parts.map((m) => MAC_GLYPHS[m] ?? m).join("") + keyLabel;
  }
  const names: Record<string, string> = { ctrl: "Ctrl", meta: "Win", alt: "Alt", shift: "Shift" };
  return [...parts.map((m) => names[m] ?? m), keyLabel].join("+");
}
