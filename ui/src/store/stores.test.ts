import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  FakeWebSocket,
  aggregate,
  homeRoutes,
  installFakeWebSocket,
  light,
  device,
  mockApi,
  resetStores,
  zone,
} from "../test-utils";
import { connect } from "./useLive";
import { useEquipments } from "./useEquipments";
import { useDevices } from "./useDevices";
import { useZoneAggregation } from "./useZoneAggregation";

beforeEach(() => {
  vi.useFakeTimers();
  installFakeWebSocket();
});

afterEach(async () => {
  // Lets the 1 s window of the coalesced refetch close.
  await vi.advanceTimersByTimeAsync(2000);
  resetStores();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("stores", () => {
  it("zone.data.changed replaces the aggregate of that zone only", async () => {
    mockApi(
      homeRoutes({
        zones: [zone("root", "Home")],
        aggregation: { a: aggregate({ temperature: 10 }), b: aggregate({ temperature: 20 }) },
      }),
    );
    connect();
    FakeWebSocket.last.open();
    await vi.advanceTimersByTimeAsync(0);
    FakeWebSocket.last.receive({
      type: "zone.data.changed",
      zoneId: "a",
      aggregatedData: aggregate({ temperature: 11.5 }),
    });
    const data = useZoneAggregation.getState().data;
    expect(data.a.temperature).toBe(11.5);
    expect(data.b.temperature).toBe(20);
  });

  it("equipment.data.changed updates the value of the binding with that alias", async () => {
    mockApi(
      homeRoutes({ zones: [zone("root", "Home")], equipments: [light("l", "Light", "z", false)] }),
    );
    connect();
    FakeWebSocket.last.open();
    await vi.advanceTimersByTimeAsync(0);
    const before = useEquipments.getState().equipments[0].dataBindings[0];
    expect(before.value).toBe(false);
    FakeWebSocket.last.receive([
      { type: "equipment.data.changed", equipmentId: "l", alias: "state", value: true },
    ]);
    const after = useEquipments.getState().equipments[0].dataBindings[0];
    expect(after.value).toBe(true);
    expect(after.lastUpdated).not.toBeNull();
  });

  it("equipment.created twice in a burst makes one refetch at once, the rest coalesced", async () => {
    const api = mockApi(homeRoutes({ zones: [zone("root", "Home")] }));
    connect();
    FakeWebSocket.last.open();
    await vi.advanceTimersByTimeAsync(0);
    expect(api.count("GET /api/v1/equipments")).toBe(1); // the open's own fetch
    await vi.advanceTimersByTimeAsync(1500);
    FakeWebSocket.last.receive([{ type: "equipment.created" }, { type: "equipment.created" }]);
    FakeWebSocket.last.receive({ type: "equipment.updated" });
    await vi.advanceTimersByTimeAsync(0);
    expect(api.count("GET /api/v1/equipments")).toBe(2); // one, not three
    await vi.advanceTimersByTimeAsync(999);
    expect(api.count("GET /api/v1/equipments")).toBe(2);
    await vi.advanceTimersByTimeAsync(1);
    expect(api.count("GET /api/v1/equipments")).toBe(3); // the trailing one of the window
  });

  it("useDevices follows add, remove and status in pairing order", () => {
    const { addDevice, removeDevice, updateDeviceStatus } = useDevices.getState();
    addDevice(device("a", "A"));
    addDevice(device("b", "B"));
    expect(Object.keys(useDevices.getState().devices)).toEqual(["a", "b"]);
    updateDeviceStatus("b", "offline");
    expect(useDevices.getState().devices.b.status).toBe("offline");
    removeDevice("a");
    expect(Object.keys(useDevices.getState().devices)).toEqual(["b"]);
  });

  it("useDevices.updateDeviceZone puts the old zone back and throws when refused", async () => {
    mockApi({ "PUT /api/v1/devices/a": { status: 404, body: { error: "Device not found" } } });
    useDevices.getState().addDevice(device("a", "A", { zoneId: "old" }));
    await expect(useDevices.getState().updateDeviceZone("a", "new")).rejects.toThrow(
      "Device not found",
    );
    expect(useDevices.getState().devices.a.zoneId).toBe("old");
  });

  it("useDevices.updateDeviceZone sends the zone, and null for no room", async () => {
    const api = mockApi({ "PUT /api/v1/devices/a": { body: {} } });
    useDevices.getState().addDevice(device("a", "A"));
    await useDevices.getState().updateDeviceZone("a", "z");
    await useDevices.getState().updateDeviceZone("a", null);
    expect(api.calls.map((c) => c.body)).toEqual([{ zoneId: "z" }, { zoneId: null }]);
    expect(useDevices.getState().devices.a.zoneId).toBeNull();
  });
});
