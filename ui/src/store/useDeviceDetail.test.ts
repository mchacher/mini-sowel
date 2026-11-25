import { afterEach, describe, expect, it, vi } from "vitest";
import { deviceWithData, mockApi, resetStores, type Route } from "../test-utils";
import { useDeviceDetail, type DataUpdatedEvent } from "./useDeviceDetail";

afterEach(() => {
  resetStores();
  vi.unstubAllGlobals();
});

const DEV = "/api/v1/devices/zb";

async function loaded(extra: Record<string, Route> = {}) {
  const api = mockApi({
    [`GET ${DEV}`]: {
      body: deviceWithData("zb", "ZBMINIR2", [["temperature", "temperature"]], {}, [
        { key: "state" },
      ]),
    },
    ...extra,
  });
  await useDeviceDetail.getState().load("zb");
  return api;
}

const event = (over: Partial<DataUpdatedEvent> = {}): DataUpdatedEvent => ({
  deviceId: "zb",
  dataId: "zb-temperature",
  key: "temperature",
  value: 21.5,
  timestamp: "2026-11-25T10:00:00.000Z",
  ...over,
});

describe("useDeviceDetail (spec 030, R7)", () => {
  it("load reads the data and the orders", async () => {
    await loaded();
    const s = useDeviceDetail.getState();
    expect(s.deviceId).toBe("zb");
    expect(s.data.map((d) => d.key)).toEqual(["temperature"]);
    expect(s.orders.map((o) => o.key)).toEqual(["state"]);
    expect(s.loading).toBe(false);
  });

  it("device.data.updated for a held data changes its value and date, with no request", async () => {
    const api = await loaded();
    useDeviceDetail.getState().handleDataUpdated(event());
    const d = useDeviceDetail.getState().data[0];
    expect(d.value).toBe(21.5);
    expect(d.lastUpdated).toBe("2026-11-25T10:00:00.000Z");
    expect(api.count(`GET ${DEV}`)).toBe(1);
  });

  it("device.data.updated for an unknown data reads the device again", async () => {
    const api = await loaded();
    useDeviceDetail.getState().handleDataUpdated(event({ dataId: "zb-new", key: "new" }));
    await vi.waitFor(() => expect(api.count(`GET ${DEV}`)).toBe(2));
  });

  it("a burst of unknown data ids keeps one read in flight, then one more", async () => {
    const api = await loaded();
    const { handleDataUpdated } = useDeviceDetail.getState();
    for (const n of ["a", "b", "c", "d"]) handleDataUpdated(event({ dataId: `zb-${n}`, key: n }));
    await vi.waitFor(() => expect(api.count(`GET ${DEV}`)).toBe(3));
    await new Promise((r) => setTimeout(r, 20));
    expect(api.count(`GET ${DEV}`)).toBe(3); // the load, the read in flight, the one reread
  });

  it("an event for another device changes nothing", async () => {
    const api = await loaded();
    const before = useDeviceDetail.getState().data;
    useDeviceDetail.getState().handleDataUpdated(event({ deviceId: "other" }));
    useDeviceDetail.getState().handleRemoved("other");
    expect(useDeviceDetail.getState().data).toBe(before);
    expect(useDeviceDetail.getState().notFound).toBe(false);
    expect(api.count(`GET ${DEV}`)).toBe(1);
  });

  it("device.removed for the device sets notFound", async () => {
    await loaded();
    useDeviceDetail.getState().handleRemoved("zb");
    expect(useDeviceDetail.getState().notFound).toBe(true);
  });

  it("a 404 is notFound, another failure is an error with its text", async () => {
    mockApi({ [`GET ${DEV}`]: { status: 404, body: { error: "Device not found" } } });
    await useDeviceDetail.getState().load("zb");
    expect(useDeviceDetail.getState().notFound).toBe(true);
    expect(useDeviceDetail.getState().error).toBeNull();

    mockApi({ [`GET ${DEV}`]: { status: 503, body: { error: "radio busy" } } });
    await useDeviceDetail.getState().load("zb");
    expect(useDeviceDetail.getState().notFound).toBe(false);
    expect(useDeviceDetail.getState().error).toContain("could not handle");
  });

  it("an answer for a device left meanwhile is ignored", async () => {
    mockApi({ [`GET ${DEV}`]: { body: deviceWithData("zb", "ZBMINIR2", [["t", "temperature"]]) } });
    const pending = useDeviceDetail.getState().load("zb");
    useDeviceDetail.getState().clear();
    await pending;
    expect(useDeviceDetail.getState().deviceId).toBeNull();
    expect(useDeviceDetail.getState().data).toEqual([]);
  });
});
