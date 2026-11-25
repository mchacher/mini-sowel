import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  FakeWebSocket,
  authTokens,
  storeSession,
  aggregate,
  homeRoutes,
  installFakeWebSocket,
  device,
  deviceWithData,
  mockApi,
  resetStores,
  zone,
} from "../test-utils";
import { connect, reconnectDelay, useLive } from "./useLive";
import { useAuth } from "./useAuth";
import { useZoneAggregation } from "./useZoneAggregation";
import { useDevices } from "./useDevices";
import { useDeviceDetail } from "./useDeviceDetail";
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
    expect(FakeWebSocket.last.protocol).toEqual(["bearer.none"]);
    expect(useLive.getState().status).toBe("connecting");
  });

  it("subscribes to zones, equipments and devices when the socket opens, and the stores fetch", () => {
    connect();
    FakeWebSocket.last.open();
    expect(useLive.getState().status).toBe("open");
    expect(FakeWebSocket.last.sent).toEqual([
      '{"type":"subscribe","topics":["zones","equipments","devices","recipes"]}',
    ]);
    expect(api.count("GET /api/v1/zones")).toBe(1);
    expect(api.count("GET /api/v1/equipments")).toBe(1);
    expect(api.count("GET /api/v1/zones/aggregation")).toBe(1);
    expect(api.count("GET /api/v1/recipes")).toBe(1);
    expect(api.count("GET /api/v1/recipe-instances")).toBe(1);
  });

  it("a recipe.instance.* event reads the instances again, once for a burst", async () => {
    connect();
    FakeWebSocket.last.open();
    await vi.advanceTimersByTimeAsync(1500);
    expect(api.count("GET /api/v1/recipe-instances")).toBe(1); // the open's own read
    FakeWebSocket.last.receive([
      { type: "recipe.instance.state.changed" },
      { type: "recipe.instance.started" },
    ]);
    FakeWebSocket.last.receive({ type: "recipe.instance.error" });
    await vi.advanceTimersByTimeAsync(0);
    expect(api.count("GET /api/v1/recipe-instances")).toBe(2); // one, not three
    await vi.advanceTimersByTimeAsync(1000);
    expect(api.count("GET /api/v1/recipe-instances")).toBe(3); // the trailing one of the window
    await vi.advanceTimersByTimeAsync(1000);
    expect(api.count("GET /api/v1/recipe-instances")).toBe(3);
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

  it("a reconnection reads the open device page again, with no spinner", async () => {
    api = mockApi({
      ...homeRoutes({ zones: [zone("root", "Home")] }),
      "GET /api/v1/devices/zb": { body: deviceWithData("zb", "ZBMINIR2", [["t", "temperature"]]) },
    });
    await useDeviceDetail.getState().load("zb");
    expect(api.count("GET /api/v1/devices/zb")).toBe(1);
    connect();
    FakeWebSocket.last.open();
    expect(useDeviceDetail.getState().loading).toBe(false);
    await vi.advanceTimersByTimeAsync(0);
    expect(api.count("GET /api/v1/devices/zb")).toBe(2);
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

  it("shows connecting for each attempt and closed for the wait between two (spec 037)", async () => {
    connect();
    FakeWebSocket.last.open();
    FakeWebSocket.last.drop();
    expect(useLive.getState().status).toBe("closed");
    await vi.advanceTimersByTimeAsync(1000); // the timer fires: a new socket
    expect(FakeWebSocket.instances.length).toBe(2);
    expect(useLive.getState().status).toBe("connecting");
    FakeWebSocket.last.drop(); // refused
    expect(useLive.getState().status).toBe("closed");
    await vi.advanceTimersByTimeAsync(2000);
    expect(useLive.getState().status).toBe("connecting");
    FakeWebSocket.last.open();
    expect(useLive.getState().status).toBe("open");
  });

  it("reads the home settings and the time zone with the other stores, on every open (spec 037)", () => {
    connect();
    FakeWebSocket.last.open();
    expect(api.count("GET /api/v1/settings")).toBe(1);
    expect(api.count("GET /api/v1/system/timezone")).toBe(1);
    FakeWebSocket.last.drop();
    vi.advanceTimersByTime(1000);
    FakeWebSocket.last.open();
    expect(api.count("GET /api/v1/settings")).toBe(2);
    expect(api.count("GET /api/v1/system/timezone")).toBe(2);
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

describe("live store, token (spec 029)", () => {
  it("offers bearer.none then bearer.<access token>", () => {
    storeSession("access-0", "refresh-0");
    connect();
    expect(FakeWebSocket.last.protocol).toEqual(["bearer.none", "bearer.access-0"]);
  });

  it("a close 4001 asks for a refresh, then reconnects with the new token", async () => {
    storeSession("access-0", "refresh-0");
    api = mockApi({
      ...homeRoutes({ zones: [zone("root", "Home")] }),
      "POST /api/v1/auth/refresh": { body: authTokens("access-1", "refresh-1") },
    });
    connect();
    FakeWebSocket.last.open();
    FakeWebSocket.last.drop(4001);
    await vi.advanceTimersByTimeAsync(0);
    expect(api.count("POST /api/v1/auth/refresh")).toBe(1);
    expect(FakeWebSocket.instances.length).toBe(2); // at once, not after the backoff
    expect(FakeWebSocket.last.protocol).toEqual(["bearer.none", "bearer.access-1"]);
  });

  it("a close 4001 with a refused refresh ends the session and does not reconnect", async () => {
    storeSession("access-0", "refresh-0");
    useAuth.setState({ isAuthenticated: true, loading: false, setupRequired: false });
    api = mockApi({ "POST /api/v1/auth/refresh": { status: 401 } });
    connect();
    FakeWebSocket.last.open();
    FakeWebSocket.last.drop(4001);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(useAuth.getState().isAuthenticated).toBe(false);
    expect(FakeWebSocket.instances.length).toBe(1);
  });

  it("a second 4001 before any open backs off instead of looping", async () => {
    storeSession("access-0", "refresh-0");
    api = mockApi({ "POST /api/v1/auth/refresh": { body: authTokens("access-1", "refresh-1") } });
    connect();
    FakeWebSocket.last.drop(4001);
    await vi.advanceTimersByTimeAsync(0);
    expect(FakeWebSocket.instances.length).toBe(2);
    FakeWebSocket.last.drop(4001);
    await vi.advanceTimersByTimeAsync(0);
    expect(api.count("POST /api/v1/auth/refresh")).toBe(1);
    expect(FakeWebSocket.instances.length).toBe(2);
    await vi.advanceTimersByTimeAsync(1000);
    expect(FakeWebSocket.instances.length).toBe(3);
  });
});
