import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  FakeWebSocket,
  aggregate,
  homeRoutes,
  installFakeWebSocket,
  device,
  mockApi,
  resetStores,
  zone,
} from "../test-utils";
import { connect, reconnectDelay, useLive } from "./useLive";
import { useZoneAggregation } from "./useZoneAggregation";
import { useDevices } from "./useDevices";
import { usePairing } from "./usePairing";

let api: ReturnType<typeof mockApi>;

beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(Math, "random").mockReturnValue(0);
  installFakeWebSocket();
  api = mockApi(homeRoutes({ zones: [zone("root", "Home")] }));
});

afterEach(() => {
  resetStores();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

function setVisibility(state: "visible" | "hidden"): void {
  Object.defineProperty(document, "visibilityState", { value: state, configurable: true });
  document.dispatchEvent(new Event("visibilitychange"));
}

describe("live store", () => {
  it("connects to /ws with the subprotocol bearer.none", () => {
    connect();
    expect(FakeWebSocket.last.url).toBe(`ws://${window.location.host}/ws`);
    expect(FakeWebSocket.last.protocol).toBe("bearer.none");
    expect(useLive.getState().status).toBe("connecting");
  });

  it("subscribes to zones, equipments and devices when the socket opens, and the stores fetch", () => {
    connect();
    FakeWebSocket.last.open();
    expect(useLive.getState().status).toBe("open");
    expect(FakeWebSocket.last.sent).toEqual([
      '{"type":"subscribe","topics":["zones","equipments","devices"]}',
    ]);
    expect(api.count("GET /api/v1/zones")).toBe(1);
    expect(api.count("GET /api/v1/equipments")).toBe(1);
    expect(api.count("GET /api/v1/zones/aggregation")).toBe(1);
  });

  it("applies an array of events and a single event, and ignores what is not JSON", () => {
    connect();
    FakeWebSocket.last.open();
    const event = (zoneId: string, temperature: number) => ({
      type: "zone.data.changed",
      zoneId,
      aggregatedData: aggregate({ temperature }),
    });
    FakeWebSocket.last.receive([event("a", 1), event("b", 2)]);
    FakeWebSocket.last.receive(event("c", 3));
    FakeWebSocket.last.receive("this is not JSON");
    FakeWebSocket.last.receive('{"type":42}');
    FakeWebSocket.last.receive("[1, null, {}]");
    const data = useZoneAggregation.getState().data;
    expect(Object.keys(data).sort()).toEqual(["a", "b", "c"]);
    expect(data.b.temperature).toBe(2);
  });

  it("zone and equipment events read the tree and the equipments again (spec 018)", async () => {
    connect();
    FakeWebSocket.last.open();
    await vi.advanceTimersByTimeAsync(1500);
    const zones = api.count("GET /api/v1/zones");
    const equipments = api.count("GET /api/v1/equipments");
    FakeWebSocket.last.receive({ type: "zone.created", zone: zone("g", "Garage", [], "root") });
    FakeWebSocket.last.receive({ type: "zone.updated", zone: zone("g", "Garage 2", [], "root") });
    FakeWebSocket.last.receive({ type: "zone.removed", zoneId: "g" });
    await vi.advanceTimersByTimeAsync(0);
    expect(api.count("GET /api/v1/zones")).toBe(zones + 3);
    FakeWebSocket.last.receive({ type: "equipment.created" });
    FakeWebSocket.last.receive({ type: "equipment.updated" });
    FakeWebSocket.last.receive({ type: "equipment.removed" });
    await vi.advanceTimersByTimeAsync(1500);
    expect(api.count("GET /api/v1/equipments")).toBeGreaterThan(equipments);
  });

  it("reconnects after 1 s, 2 s, 4 s... capped at 30 s, and every open refetches", async () => {
    connect();
    FakeWebSocket.last.open();
    FakeWebSocket.last.drop();
    expect(useLive.getState().status).toBe("closed");
    for (const wait of [1000, 2000, 4000, 8000, 16000, 30000, 30000]) {
      const before = FakeWebSocket.instances.length;
      await vi.advanceTimersByTimeAsync(wait - 1);
      expect(FakeWebSocket.instances.length).toBe(before);
      await vi.advanceTimersByTimeAsync(1);
      expect(FakeWebSocket.instances.length).toBe(before + 1);
      FakeWebSocket.last.drop(); // refused: the next wait doubles
    }
    // An open resets the wait to 1 s and refetches.
    await vi.advanceTimersByTimeAsync(30000);
    FakeWebSocket.last.open();
    expect(useLive.getState().status).toBe("open");
    expect(api.count("GET /api/v1/zones")).toBe(2);
    FakeWebSocket.last.drop();
    const before = FakeWebSocket.instances.length;
    await vi.advanceTimersByTimeAsync(1000);
    expect(FakeWebSocket.instances.length).toBe(before + 1);
  });

  it("adds the jitter of Sowel's client to each wait", async () => {
    vi.spyOn(Math, "random").mockReturnValue(0.5);
    expect(reconnectDelay(0)).toBe(1250);
    expect(reconnectDelay(1)).toBe(2250);
    expect(reconnectDelay(10)).toBe(30250);
    connect();
    FakeWebSocket.last.drop();
    await vi.advanceTimersByTimeAsync(1249);
    expect(FakeWebSocket.instances.length).toBe(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(FakeWebSocket.instances.length).toBe(2);
    FakeWebSocket.last.drop();
    await vi.advanceTimersByTimeAsync(2249);
    expect(FakeWebSocket.instances.length).toBe(2);
    await vi.advanceTimersByTimeAsync(1);
    expect(FakeWebSocket.instances.length).toBe(3);
  });

  it("reconnects at once when the page becomes visible or the network is back, after a close", () => {
    connect();
    FakeWebSocket.last.open();
    FakeWebSocket.last.drop();
    expect(FakeWebSocket.instances.length).toBe(1);
    setVisibility("visible");
    expect(FakeWebSocket.instances.length).toBe(2);
    FakeWebSocket.last.drop();
    window.dispatchEvent(new Event("online"));
    expect(FakeWebSocket.instances.length).toBe(3);
  });

  it("does nothing when the page becomes visible while the socket is open or opening", () => {
    connect();
    setVisibility("visible");
    expect(FakeWebSocket.instances.length).toBe(1);
    FakeWebSocket.last.open();
    setVisibility("hidden");
    setVisibility("visible");
    window.dispatchEvent(new Event("online"));
    expect(FakeWebSocket.instances.length).toBe(1);
  });

  it("applies device.discovered, device.status_changed and device.removed, and marks the new one", () => {
    connect();
    FakeWebSocket.last.open();
    usePairing.getState().setPageOpen(true);
    FakeWebSocket.last.receive({ type: "device.discovered", device: device("a", "A") });
    expect(Object.keys(useDevices.getState().devices)).toEqual(["a"]);
    expect(usePairing.getState().newIds).toEqual(["a"]);
    FakeWebSocket.last.receive({
      type: "device.status_changed",
      deviceId: "a",
      deviceName: "A",
      status: "offline",
    });
    expect(useDevices.getState().devices.a.status).toBe("offline");
    expect(usePairing.getState().newIds).toEqual(["a"]);
    FakeWebSocket.last.receive({ type: "device.removed", deviceId: "a", deviceName: "A" });
    expect(useDevices.getState().devices).toEqual({});
  });

  it("marks no device new while the Discovery screen is not open", () => {
    connect();
    FakeWebSocket.last.open();
    FakeWebSocket.last.receive({ type: "device.discovered", device: device("a", "A") });
    expect(Object.keys(useDevices.getState().devices)).toEqual(["a"]);
    expect(usePairing.getState().newIds).toEqual([]);
    usePairing.getState().setPageOpen(true);
    usePairing.getState().setPageOpen(false);
    FakeWebSocket.last.receive({ type: "device.discovered", device: device("b", "B") });
    expect(usePairing.getState().newIds).toEqual([]);
  });

  it("reads the devices and the pairing state again on every open", () => {
    connect();
    FakeWebSocket.last.open();
    expect(api.count("GET /api/v1/devices")).toBe(1);
    expect(api.count("GET /api/v1/integrations/solite/permit-join")).toBe(1);
    FakeWebSocket.last.drop();
    vi.advanceTimersByTime(1000);
    FakeWebSocket.last.open();
    expect(api.count("GET /api/v1/devices")).toBe(2);
    expect(api.count("GET /api/v1/integrations/solite/permit-join")).toBe(2);
  });
});
