import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  FakeWebSocket,
  aggregate,
  homeRoutes,
  installFakeWebSocket,
  light,
  mockApi,
  resetStores,
  zone,
} from "../test-utils";
import { connect } from "./useLive";
import { useEquipments } from "./useEquipments";
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
});
