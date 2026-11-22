import { afterEach, describe, expect, it, vi } from "vitest";
import { fixtures, mockApi } from "../test-utils";
import { getDevice } from "./devices";

afterEach(() => vi.unstubAllGlobals());

describe("devices api", () => {
  it("getDevice reads one device with its data and orders", async () => {
    const api = mockApi({ "GET /api/v1/devices/zb": { body: fixtures.zbminir2() } });
    const device = await getDevice("zb");
    expect(device.orders?.[0].key).toBe("state");
    expect(api.calls.map((c) => c.key)).toEqual(["GET /api/v1/devices/zb"]);
  });
});
