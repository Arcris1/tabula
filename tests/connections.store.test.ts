import { describe, it, expect, vi, beforeEach } from "vitest";
import { setActivePinia, createPinia } from "pinia";

vi.mock("../src/lib/api", () => ({
  api: {
    listConnections: vi.fn().mockResolvedValue([
      { id: "a", name: "local mysql", engine: "mysql", color: "blue", isProduction: false },
    ]),
    saveConnection: vi.fn().mockResolvedValue(undefined),
    deleteConnection: vi.fn().mockResolvedValue(undefined),
    connect: vi.fn().mockResolvedValue("sql"),
    disconnect: vi.fn().mockResolvedValue(undefined),
    testConnection: vi.fn().mockResolvedValue(undefined),
  },
}));

import { api } from "../src/lib/api";
import { useConnectionsStore } from "../src/stores/connections";

describe("connections store", () => {
  beforeEach(() => setActivePinia(createPinia()));

  it("loads connections", async () => {
    const store = useConnectionsStore();
    await store.load();
    expect(store.connections).toHaveLength(1);
    expect(store.connections[0].name).toBe("local mysql");
  });

  it("save reloads the list", async () => {
    const store = useConnectionsStore();
    await store.save(
      { id: "b", name: "x", engine: "sqlite", color: "gray", isProduction: false },
      undefined,
    );
    expect(api.saveConnection).toHaveBeenCalled();
    expect(api.listConnections).toHaveBeenCalled();
  });

  it("connect returns paradigm", async () => {
    const store = useConnectionsStore();
    expect(await store.connect("a")).toBe("sql");
  });
});
