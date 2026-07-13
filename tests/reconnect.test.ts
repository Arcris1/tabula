import { describe, it, expect, vi } from "vitest";

// The reconnect helper is internal to api.ts; we test its behaviour through a
// re-implementation-free approach: import the module and drive fetchRows with a
// mocked invoke. Simpler: replicate the classifier + retry contract here by
// importing the exported api and mocking @tauri-apps/api/core.

vi.mock("@tauri-apps/api/core", () => {
  return { invoke: vi.fn() };
});

import { invoke } from "@tauri-apps/api/core";
import { api, type TableInfo } from "../src/lib/api";

const table: TableInfo = { name: "t", schema: null, kind: "table" } as unknown as TableInfo;
const opts = { limit: 10, offset: 0, sort: null, filters: [] } as any;

describe("fetchRows auto-reconnect", () => {
  it("retries once when the connection was closed, then succeeds", async () => {
    const mock = invoke as unknown as ReturnType<typeof vi.fn>;
    mock.mockReset();
    mock
      .mockRejectedValueOnce(new Error("connection closed by server"))
      .mockResolvedValueOnce({ columns: [], rows: [], query: "SELECT 1" });
    const page = await api.fetchRows("c1", table, opts);
    expect(page.query).toBe("SELECT 1");
    expect(mock).toHaveBeenCalledTimes(2);
  });

  it("does not retry on a normal SQL error", async () => {
    const mock = invoke as unknown as ReturnType<typeof vi.fn>;
    mock.mockReset();
    mock.mockRejectedValue(new Error("syntax error near SELECT"));
    await expect(api.fetchRows("c1", table, opts)).rejects.toThrow("syntax error");
    expect(mock).toHaveBeenCalledTimes(1);
  });
});
