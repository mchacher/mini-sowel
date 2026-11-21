import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockApi, resetStores } from "../test-utils";
import { useZones } from "../store/useZones";
import { fetchJSON } from "./client";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  resetStores();
});

describe("client", () => {
  it("retries a GET answered 503 once, after 1 s, and gives the data", async () => {
    const api = mockApi({
      "GET /api/v1/zones": [{ status: 503, body: { error: "Busy" } }, { body: [{ id: "z" }] }],
    });
    const pending = fetchJSON("/api/v1/zones");
    await vi.advanceTimersByTimeAsync(999);
    expect(api.count("GET /api/v1/zones")).toBe(1);
    await vi.advanceTimersByTimeAsync(1);
    await expect(pending).resolves.toEqual([{ id: "z" }]);
    expect(api.count("GET /api/v1/zones")).toBe(2);
  });

  it("does not retry twice, nor a POST", async () => {
    const api = mockApi({
      "GET /api/v1/zones": { status: 503, body: { error: "Busy" } },
      "POST /api/v1/zones": { status: 503, body: { error: "Busy" } },
    });
    const get = fetchJSON("/api/v1/zones").catch((e: Error) => e.message);
    await vi.advanceTimersByTimeAsync(1000);
    expect(await get).toBe("Busy");
    expect(api.count("GET /api/v1/zones")).toBe(2);
    await expect(fetchJSON("/api/v1/zones", { method: "POST", body: "{}" })).rejects.toThrow(
      "Busy",
    );
    expect(api.count("POST /api/v1/zones")).toBe(1);
  });

  it("gives a store the text of a failed fetch when nothing answers", async () => {
    vi.stubGlobal("fetch", () => Promise.reject(new TypeError("Failed to fetch")));
    await useZones.getState().fetchZones();
    expect(useZones.getState().error).toBe("Cannot reach mini-Sowel");
    expect(useZones.getState().loading).toBe(false);
  });
});
