# Shortcuts + Multi-Window — Design Spec

**Date:** 2026-07-07 · **Status:** Approved (design approved in session)

## 1. Cross-platform keyboard shortcuts

`mod` = ⌘ on macOS, Ctrl on Windows/Linux (detected once at startup). Bindings
map to EXISTING actions only. Combos with a mod modifier fire even when an
input/editor is focused; modals handle Esc themselves.

| Binding (mod = ⌘/Ctrl) | Context | Action |
|---|---|---|
| mod+N | Launcher | New connection form |
| mod+T | Workspace (SQL) | New query tab |
| mod+W | Workspace (SQL) | Close active query tab (never the window) |
| mod+1 / mod+2 | Workspace (SQL) | Data / Structure tab |
| mod+3…mod+9 | Workspace (SQL) | Query tabs in order |
| Ctrl+Tab / Ctrl+Shift+Tab, mod+Shift+] / mod+Shift+[ | Workspace (SQL) | Next / previous tab |
| mod+R, F5 | Data / Structure / Query | Reload grid / refresh structure / re-run last query |
| mod+S | Data tab | Commit pending changes (production guard kept) |
| mod+Alt+Z | Data tab | Discard pending changes |
| mod+I | Data tab (editable) | Insert draft row |
| mod+F | Data tab | Focus first column-filter input |
| mod+P | Workspace (SQL) | Focus sidebar table search |
| mod+Y | Workspace (SQL) | Toggle history panel |
| mod+Enter | Query editor | Run statement (exists) |
| mod+. | Query tab | Cancel running query |
| mod+Shift+D | Workspace | Disconnect |
| mod+Shift+N | Anywhere | New window |
| Esc | Modals / cell edit | Close / cancel (per-component) |
| mod+/ | Anywhere | Shortcut cheat sheet (platform-appropriate glyphs) |

Architecture: `src/lib/keymap.ts` (pure: platform detection, `comboOf(event)`
normalization, per-platform `bindings()` table, `displayCombo()`), Pinia
`shortcuts` store (register/unregister named handlers, `dispatch(action)`),
one global keydown listener in App.vue. Per-tab actions are namespaced
(`query:cancel:<tabId>`); App resolves context (mainTab) before dispatching.
`ShortcutHelpModal.vue` renders the table.

## 2. Multi-window

- `mod+Shift+N` / toolbar button → Rust command `open_new_window`
  (WebviewWindowBuilder, unique label, same URL). Works on Windows later.
- Each window has independent Vue/Pinia state (own launcher, connection, tabs).
- **Backend isolation:** connection-scoped registries (pools, tunnels, running
  queries) are keyed by `{window_label}:{connection_id}`. All connection
  commands receive `tauri::Window` and use the scoped key. Two windows on the
  same saved connection get independent pools; disconnect in one never affects
  the other.
- `delete_connection` sweeps `*:{id}` across windows (`remove_by_connection`);
  window Destroyed event sweeps `{label}:*` (`remove_by_window`).
- Window title = `dbclient — <connection name>` while connected; reset on
  disconnect.
- Query events already emit to the owning window only.

## 3. Testing

Vitest: keymap normalization on both platforms, bindings, displayCombo,
shortcuts store register/dispatch. Rust: registry scoped-key tests (two labels
same connection: remove one window's entry, the other survives;
remove_by_connection clears both). Existing suites stay green.
