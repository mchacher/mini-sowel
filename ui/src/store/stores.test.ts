import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  FakeWebSocket,
  aggregate,
  homeRoutes,
  installFakeWebSocket,
  light,
  device,
  fixtures,
  mockApi,
  resetStores,
  zone,
} from "../test-utils";
import { connect } from "./useLive";
import { computeBindingPlan } from "../components/equipments/bindingUtils";
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

  it("useDevices.updateDeviceName sends the name only, and puts the old one back when refused", async () => {
    const api = mockApi({
      "PUT /api/v1/devices/a": [{ body: {} }, { status: 404, body: { error: "Device not found" } }],
    });
    useDevices.getState().addDevice(device("a", "A"));
    await useDevices.getState().updateDeviceName("a", "B");
    expect(api.calls.map((c) => c.body)).toEqual([{ name: "B" }]);
    await expect(useDevices.getState().updateDeviceName("a", "C")).rejects.toThrow(
      "Device not found",
    );
    expect(useDevices.getState().devices.a.name).toBe("B");
    expect("updateDeviceZone" in useDevices.getState()).toBe(false);
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
    const data = { name: "Light", type: "light_onoff" as const, zoneId: "root" };
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

  it("useEquipments.createWithBindings sends the plan after the equipment, then reads the list", async () => {
    const api = mockApi({
      ...homeRoutes({ zones: [zone("root", "Home")] }),
      "POST /api/v1/equipments": { status: 201, body: light("l", "Light", "root", false) },
      "POST /api/v1/equipments/l/data-bindings": { status: 201, body: {} },
      "POST /api/v1/equipments/l/order-bindings": { status: 201, body: {} },
    });
    const plan = computeBindingPlan([fixtures.zbminir2()], "light_onoff");
    await useEquipments
      .getState()
      .createWithBindings({ name: "Light", type: "light_onoff", zoneId: "root" }, plan);
    expect(api.calls.map((c) => c.key)).toEqual([
      "POST /api/v1/equipments",
      "POST /api/v1/equipments/l/data-bindings",
      "POST /api/v1/equipments/l/order-bindings",
      "GET /api/v1/equipments",
    ]);
  });

  it("useEquipments.createWithBindings deletes the equipment when a binding is refused", async () => {
    const api = mockApi({
      ...homeRoutes({ zones: [zone("root", "Home")] }),
      "POST /api/v1/equipments": { status: 201, body: light("l", "Light", "root", false) },
      "POST /api/v1/equipments/l/data-bindings": {
        status: 404,
        body: { error: "DeviceData not found: zb-state" },
      },
      "DELETE /api/v1/equipments/l": { status: 204 },
    });
    const plan = computeBindingPlan([fixtures.zbminir2()], "light_onoff");
    await expect(
      useEquipments
        .getState()
        .createWithBindings({ name: "Light", type: "light_onoff", zoneId: "root" }, plan),
    ).rejects.toThrow("DeviceData not found: zb-state");
    expect(api.count("DELETE /api/v1/equipments/l")).toBe(1);
    expect(api.count("POST /api/v1/equipments/l/order-bindings")).toBe(0);
  });

  it("useEquipments.createWithBindings throws the binding's text when the delete is refused too", async () => {
    const api = mockApi({
      ...homeRoutes({ zones: [zone("root", "Home")] }),
      "POST /api/v1/equipments": { status: 201, body: light("l", "Light", "root", false) },
      "POST /api/v1/equipments/l/data-bindings": { status: 503, body: { error: "Busy" } },
      "DELETE /api/v1/equipments/l": { status: 500, body: { error: "disk error" } },
    });
    const plan = computeBindingPlan([fixtures.zbminir2()], "light_onoff");
    await expect(
      useEquipments
        .getState()
        .createWithBindings({ name: "Light", type: "light_onoff", zoneId: "root" }, plan),
    ).rejects.toThrow("Busy");
    expect(api.count("GET /api/v1/equipments")).toBe(1);
  });
});
