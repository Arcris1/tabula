import { describe, it, expect } from "vitest";
import { bindings, comboOf, displayCombo } from "../src/lib/keymap";

function ev(init: Partial<KeyboardEvent> & { key: string }): KeyboardEvent {
  return new KeyboardEvent("keydown", init);
}

describe("comboOf", () => {
  it("normalizes modifiers in stable order", () => {
    expect(comboOf(ev({ key: "d", metaKey: true, shiftKey: true }))).toBe("meta+shift+d");
    expect(comboOf(ev({ key: "Tab", ctrlKey: true }))).toBe("ctrl+tab");
    expect(comboOf(ev({ key: "F5" }))).toBe("f5");
  });
  it("ignores bare modifier presses", () => {
    expect(comboOf(ev({ key: "Shift", shiftKey: true }))).toBeNull();
    expect(comboOf(ev({ key: "Meta", metaKey: true }))).toBeNull();
  });
});

describe("bindings", () => {
  it("resolves mod per platform", () => {
    expect(bindings("mac")["meta+s"]).toBe("grid:commit");
    expect(bindings("other")["ctrl+s"]).toBe("grid:commit");
    expect(bindings("mac")["ctrl+s"]).toBeUndefined();
  });
  it("keeps ctrl+tab on both platforms and maps F5", () => {
    expect(bindings("mac")["ctrl+tab"]).toBe("tabs:next");
    expect(bindings("other")["ctrl+tab"]).toBe("tabs:next");
    expect(bindings("other")["f5"]).toBe("view:refresh");
  });
  it("numeric tabs", () => {
    expect(bindings("mac")["meta+3"]).toBe("tabs:index:0");
    expect(bindings("other")["ctrl+9"]).toBe("tabs:index:6");
  });
});

describe("help binding", () => {
  it("mod+/ is left free for the editor's toggle-comment", () => {
    expect(bindings("mac")["meta+/"]).toBeUndefined();
    expect(bindings("other")["ctrl+/"]).toBeUndefined();
  });
  it("help opens on mod+shift+/ (either key form) and F1", () => {
    expect(bindings("mac")["meta+shift+/"]).toBe("help:toggle");
    expect(bindings("mac")["meta+shift+?"]).toBe("help:toggle");
    expect(bindings("other")["f1"]).toBe("help:toggle");
  });
});

describe("displayCombo", () => {
  it("mac glyphs", () => {
    expect(displayCombo("mod+shift+d", "mac")).toBe("⌘⇧D");
    expect(displayCombo("mod+enter", "mac")).toBe("⌘↵");
  });
  it("windows names", () => {
    expect(displayCombo("mod+shift+d", "other")).toBe("Ctrl+Shift+D");
    expect(displayCombo("ctrl+tab", "other")).toBe("Ctrl+Tab");
  });
});
