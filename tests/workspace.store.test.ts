import { describe, it, expect, vi, beforeEach } from "vitest";
import { setActivePinia, createPinia } from "pinia";

vi.mock("../src/lib/api", () => ({
  api: {
    listTables: vi.fn().mockResolvedValue([]),
    listFunctions: vi.fn().mockResolvedValue([]),
    disconnect: vi.fn(),
    reconnect: vi.fn().mockResolvedValue("sql"),
  },
}));

import { api } from "../src/lib/api";
import { useWorkspaceStore } from "../src/stores/workspace";

describe("workspace store close()", () => {
  beforeEach(() => setActivePinia(createPinia()));

  it("returns to the launcher even if the backend disconnect rejects", async () => {
    (api.disconnect as any).mockRejectedValueOnce(new Error("connection closed"));
    const ws = useWorkspaceStore();
    await ws.addConnection("conn-1", "kv");
    expect(ws.isOpen).toBe(true);

    await ws.close();
    // isOpen must be false so the ConnectionLauncher renders
    expect(ws.isOpen).toBe(false);
    expect(ws.activeId).toBe(null);
    expect(ws.paradigm).toBe(null);
  });

  it("clears workspace state on a normal disconnect", async () => {
    (api.disconnect as any).mockResolvedValueOnce(undefined);
    const ws = useWorkspaceStore();
    await ws.addConnection("conn-2", "sql");
    await ws.close();
    expect(ws.isOpen).toBe(false);
    expect(ws.activeId).toBe(null);
  });

  it("reconnect rebuilds the backend connection while preserving state", async () => {
    (api.reconnect as any).mockResolvedValue("sql");
    const ws = useWorkspaceStore();
    await ws.addConnection("c1", "sql");
    ws.selectTable({ schema: null, name: "keep_me", kind: "table" } as any);

    await ws.reconnect("c1");
    expect(api.reconnect).toHaveBeenCalledWith("c1");
    // workspace state survives the reconnect
    expect(ws.selectedTable?.name).toBe("keep_me");
    expect(ws.isOpen).toBe(true);
  });

  it("keeps per-connection state and switches active when one is closed", async () => {
    const ws = useWorkspaceStore();
    await ws.addConnection("a", "kv");
    await ws.addConnection("b", "kv");
    expect(ws.conns.length).toBe(2);
    expect(ws.activeId).toBe("b");

    ws.selectTable({ schema: null, name: "t_b", kind: "table" } as any);
    ws.setActive("a");
    ws.selectTable({ schema: null, name: "t_a", kind: "table" } as any);
    expect(ws.selectedTable?.name).toBe("t_a");
    ws.setActive("b");
    expect(ws.selectedTable?.name).toBe("t_b"); // state preserved per connection

    await ws.closeConnection("b");
    expect(ws.activeId).toBe("a"); // switched to the neighbour
    expect(ws.conns.length).toBe(1);
  });
});
