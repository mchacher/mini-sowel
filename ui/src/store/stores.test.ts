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
import { useZones } from "./useZones";

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

  it("useZones.createZone posts, then reads the tree again; a refusal throws and leaves the tree", async () => {
    const api = mockApi({
      ...homeRoutes({ zones: [zone("root", "Home")] }),
      "POST /api/v1/zones": [
        { status: 201, body: zone("g", "Garage", [], "root") },
        { status: 400, body: { error: "Too many zones (32)" } },
      ],
    });
    useZones.setState({ tree: [zone("root", "Home")] });
    const created = await useZones.getState().createZone({ name: "Garage", parentId: "root" });
    expect(created.id).toBe("g");
    expect(api.count("GET /api/v1/zones")).toBe(1);
    const tree = useZones.getState().tree;
    await expect(useZones.getState().createZone({ name: "More" })).rejects.toThrow(
      "Too many zones (32)",
    );
    expect(api.count("GET /api/v1/zones")).toBe(1);
    expect(useZones.getState().tree).toBe(tree);
  });

  it("useZones.updateZone and deleteZone read the tree again; a refusal throws", async () => {
    const api = mockApi({
      ...homeRoutes({ zones: [zone("root", "Home")] }),
      "PUT /api/v1/zones/g": { body: {} },
      "DELETE /api/v1/zones/g": {
        status: 400,
        body: { error: "Cannot delete zone with 1 equipment. Remove or move them first." },
      },
    });
    await useZones.getState().updateZone("g", { name: "Garage 2" });
    expect(api.count("GET /api/v1/zones")).toBe(1);
    await expect(useZones.getState().deleteZone("g")).rejects.toThrow(/Cannot delete zone/);
    expect(api.count("GET /api/v1/zones")).toBe(1);
  });

  it("useEquipments create, update and delete read the list again; a refusal throws", async () => {
    const api = mockApi({
      ...homeRoutes({ zones: [zone("root", "Home")] }),
      "POST /api/v1/equipments": [
        { status: 201, body: light("l", "Light", "root", false) },
        { status: 400, body: { error: "Too many equipments (64)" } },
      ],
      "PUT /api/v1/equipments/l": { body: {} },
      "DELETE /api/v1/equipments/l": { status: 204 },
    });
    const data = { name: "Light", type: "light_onoff" as const, zoneId: "root", deviceIds: ["r"] };
    await useEquipments.getState().createEquipment(data);
    expect(api.count("GET /api/v1/equipments")).toBe(1);
    await useEquipments.getState().updateEquipment("l", { name: "Lamp" });
    expect(api.count("GET /api/v1/equipments")).toBe(2);
    await useEquipments.getState().deleteEquipment("l");
    expect(api.count("GET /api/v1/equipments")).toBe(3);
    await expect(useEquipments.getState().createEquipment(data)).rejects.toThrow(
      "Too many equipments (64)",
    );
    expect(api.count("GET /api/v1/equipments")).toBe(3);
  });
});
