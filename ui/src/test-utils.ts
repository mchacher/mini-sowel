// Helpers of the tests: a WebSocket that the test drives, a `fetch` that answers from a table,
// and builders of the zones and equipments of Sowel's API.
import { act } from "@testing-library/preact";
import { expect, vi } from "vitest";
import { disconnect, useLive } from "./store/useLive";
import { setAccessToken } from "./api/client";
import { useAuth } from "./store/useAuth";
import { useEquipments } from "./store/useEquipments";
import { useZoneAggregation } from "./store/useZoneAggregation";
import { useZones } from "./store/useZones";
import { useRecipes } from "./store/useRecipes";
import { useDevices } from "./store/useDevices";
import { useDeviceDetail } from "./store/useDeviceDetail";
import { useHomeSettings } from "./store/useHomeSettings";
import { useTimezone } from "./store/useTimezone";
import { resetPairingTimers, usePairing } from "./store/usePairing";
import type {
  DataBindingWithValue,
  DataCategory,
  Device,
  DataType,
  DeviceData,
  DeviceOrder,
  EquipmentWithDetails,
  OrderBindingWithDetails,
  OrderCategory,
  RecipeInfo,
  RecipeInstance,
  ZoneAggregatedData,
  ZoneWithChildren,
} from "./types";

/** A WebSocket driven by the test: `FakeWebSocket.instances` lists the sockets the page opened. */
export class FakeWebSocket {
  static instances: FakeWebSocket[] = [];
  static OPEN = 1;
  static CONNECTING = 0;
  static CLOSED = 3;

  readyState = 0;
  sent: string[] = [];
  onopen: (() => void) | null = null;
  onmessage: ((message: { data: string }) => void) | null = null;
  onclose: ((event: { code: number }) => void) | null = null;
  onerror: (() => void) | null = null;

  url: string;
  protocol?: string;

  constructor(url: string, protocol?: string) {
    this.url = url;
    this.protocol = protocol;
    FakeWebSocket.instances.push(this);
  }

  send(data: string): void {
    this.sent.push(data);
  }

  close(): void {
    this.readyState = 3;
  }

  /** The server accepts the connection. */
  open(): void {
    this.readyState = 1;
    this.onopen?.();
  }

  /** The server sends a text frame. */
  receive(data: unknown): void {
    this.onmessage?.({ data: typeof data === "string" ? data : JSON.stringify(data) });
  }

  /** The connection ends (the server stops, or refuses it with a 503). */
  drop(code = 1006): void {
    this.readyState = 3;
    this.onclose?.({ code });
  }

  static get last(): FakeWebSocket {
    return FakeWebSocket.instances[FakeWebSocket.instances.length - 1];
  }

  static reset(): void {
    FakeWebSocket.instances = [];
  }
}

export function installFakeWebSocket(): void {
  FakeWebSocket.reset();
  vi.stubGlobal("WebSocket", FakeWebSocket);
}

type Answer = { status?: number; body?: unknown; headers?: Record<string, string> };
export type Route =
  Answer | ((request: { method: string; body: unknown }) => Answer | Promise<Answer>);

/** A `fetch` that answers from a table keyed by `"GET /api/v1/zones"`; every call is recorded. */
export function mockApi(routes: Record<string, Route | Route[]>) {
  const calls: { key: string; body: unknown }[] = [];
  const headersOf: Record<string, string>[] = []; // parallel to `calls`
  const fetchMock = vi.fn(async (url: string, init?: RequestInit) => {
    const method = (init?.method ?? "GET").toUpperCase();
    const key = `${method} ${url}`;
    // A string is JSON; an upload (a File) is recorded as it is.
    const body = !init?.body
      ? undefined
      : typeof init.body === "string"
        ? JSON.parse(init.body)
        : init.body;
    calls.push({ key, body });
    headersOf.push((init?.headers ?? {}) as Record<string, string>);
    const entry = routes[key];
    // A list answers its first item once, then the next, the last one for good.
    const route: Route = Array.isArray(entry)
      ? ((entry.length > 1 ? entry.shift() : entry[0]) ?? { status: 404 })
      : (entry ?? { status: 404 });
    const answer = typeof route === "function" ? await route({ method, body }) : route;
    const status = answer.status ?? 200;
    return new Response(status === 204 ? null : JSON.stringify(answer.body ?? {}), {
      status,
      headers: { "Content-Type": "application/json", ...answer.headers },
    });
  });
  vi.stubGlobal("fetch", fetchMock);
  return {
    fetchMock,
    calls,
    headersOf,
    count: (key: string) => calls.filter((c) => c.key === key).length,
  };
}

export function zone(
  id: string,
  name: string,
  children: ZoneWithChildren[] = [],
  parentId: string | null = null,
): ZoneWithChildren {
  return {
    id,
    name,
    parentId,
    displayOrder: 0,
    createdAt: "2026-11-21T10:00:00.000Z",
    updatedAt: "2026-11-21T10:00:00.000Z",
    children,
  };
}

export function aggregate(partial: Partial<ZoneAggregatedData> = {}): ZoneAggregatedData {
  return {
    temperature: null,
    humidity: null,
    luminosity: null,
    motion: false,
    motionSensors: 0,
    motionSince: null,
    openDoors: 0,
    openWindows: 0,
    waterLeak: false,
    smoke: false,
    lightsOn: 0,
    lightsTotal: 0,
    shuttersOpen: 0,
    shuttersTotal: 0,
    averageShutterPosition: null,
    doorsOpen: 0,
    doorsTotal: 0,
    waterValvesOpen: 0,
    waterValvesTotal: 0,
    waterFlowTotal: null,
    powerTotal: null,
    sunrise: null,
    sunset: null,
    isDaylight: null,
    displaysOnline: 0,
    displaysTotal: 0,
    unavailableEquipmentsByCategory: {},
    unavailableEquipmentNamesByCategory: {},
    ...partial,
  };
}

export function dataBinding(
  alias: string,
  category: DataBindingWithValue["category"],
  value: unknown,
  extra: Partial<DataBindingWithValue> = {},
): DataBindingWithValue {
  return {
    id: `db-${alias}`,
    equipmentId: "e",
    deviceDataId: `dd-${alias}`,
    alias,
    deviceId: "d",
    deviceName: "Device",
    key: alias,
    type: typeof value === "boolean" ? "boolean" : "number",
    category,
    value,
    lastUpdated: null,
    lastChanged: null,
    stale: false,
    ...extra,
  };
}

export function equipment(
  id: string,
  name: string,
  type: EquipmentWithDetails["type"],
  zoneId: string,
  extra: Partial<EquipmentWithDetails> = {},
): EquipmentWithDetails {
  return {
    id,
    name,
    type,
    zoneId,
    enabled: true,
    createdAt: "2026-11-21T10:00:00.000Z",
    updatedAt: "2026-11-21T10:00:00.000Z",
    dataBindings: [],
    orderBindings: [],
    status: "online",
    ...extra,
  };
}

/** A `light_onoff` as Solite's API gives it: a `state` data binding and a `state` order. */
export function light(
  id: string,
  name: string,
  zoneId: string,
  on: boolean,
  extra: Partial<EquipmentWithDetails> = {},
): EquipmentWithDetails {
  const order: OrderBindingWithDetails = {
    id: `ob-${id}`,
    equipmentId: id,
    deviceOrderId: "do",
    alias: "state",
    deviceId: "d",
    deviceName: "Relay",
    key: "state",
    type: "enum",
    category: "light_toggle",
  };
  return equipment(id, name, "light_onoff", zoneId, {
    dataBindings: [dataBinding("state", "light_state", on, { equipmentId: id })],
    orderBindings: [order],
    ...extra,
  });
}

export function sensor(
  id: string,
  name: string,
  zoneId: string,
  extra: Partial<EquipmentWithDetails> = {},
): EquipmentWithDetails {
  return equipment(id, name, "sensor", zoneId, {
    dataBindings: [
      dataBinding("temperature", "temperature", 12.6, { equipmentId: id, unit: "°C" }),
      dataBinding("humidity", "humidity", 59, { equipmentId: id, unit: "%" }),
    ],
    ...extra,
  });
}

/** Every store and the live connection back to their start state, between two tests. */
export function resetStores(): void {
  disconnect();
  useZones.setState({ tree: [], loading: false, error: null });
  useEquipments.setState({ equipments: [], loading: false, error: null, loaded: false });
  useZoneAggregation.setState({ data: {}, loading: false, error: null });
  useLive.setState({ status: "connecting" });
  resetPairingTimers();
  useHomeSettings.setState({ name: null, hasLocation: null });
  useTimezone.setState({ tz: "UTC", source: "unknown", offsetHours: 0, loaded: false });
  localStorage.clear();
  setAccessToken(null);
  useAuth.setState({
    user: null,
    isAuthenticated: false,
    setupRequired: null,
    loading: true,
    unreachable: false,
  });
  useDevices.setState({ devices: {}, loading: false, error: null });
  useDeviceDetail.getState().clear();
  useRecipes.setState({ recipes: [], instances: [], loading: false, error: null });
  usePairing.setState({ open: false, remaining: 0, error: null, newIds: [], pageOpen: false });
}

/** A device as `GET /api/v1/devices` gives it, without its data. */
export function device(id: string, name: string, extra: Partial<Device> = {}): Device {
  return {
    id,
    integrationId: "solite",
    sourceDeviceId: `0x${id}`,
    name,
    manufacturer: "SONOFF",
    model: "ZBMINIR2",
    zoneId: null,
    source: "solite",
    status: "online",
    lastSeen: null,
    createdAt: "2026-11-21T10:00:00.000Z",
    updatedAt: "2026-11-21T10:00:00.000Z",
    ...extra,
  };
}

/** A session of the host, as `POST /auth/login` and `/auth/refresh` give it. */
export function authTokens(access = "access-1", refresh = "refresh-1") {
  return {
    accessToken: access,
    refreshToken: refresh,
    expiresIn: 900,
    user: {
      id: "u1",
      username: "admin",
      displayName: "Admin",
      role: "admin" as const,
      preferences: { language: "en" as const },
      enabled: true,
      lastLoginAt: null,
      createdAt: "2026-11-25T10:00:00.000Z",
    },
  };
}

/** The browser holds a session: the stored tokens, as after a login. */
export function storeSession(access = "access-0", refresh = "refresh-0"): void {
  localStorage.setItem("sowel_access_token", access);
  localStorage.setItem("sowel_refresh_token", refresh);
  setAccessToken(access);
}

/** The GETs the stores make when the socket opens. */
export function homeRoutes(home: {
  zones: ZoneWithChildren[];
  equipments?: EquipmentWithDetails[];
  aggregation?: Record<string, ZoneAggregatedData>;
  devices?: Device[];
  settings?: Record<string, string>;
  timezone?: { tz: string; source: string; offsetHours: number };
}): Record<string, Route> {
  return {
    "GET /api/v1/auth/status": { body: { setupRequired: false } },
    "POST /api/v1/auth/refresh": { body: authTokens() },
    "GET /api/v1/zones": { body: home.zones },
    "GET /api/v1/equipments": { body: home.equipments ?? [] },
    "GET /api/v1/zones/aggregation": { body: home.aggregation ?? {} },
    "GET /api/v1/devices": { body: home.devices ?? [] },
    "GET /api/v1/settings": { body: home.settings ?? {} },
    "GET /api/v1/system/timezone": {
      body: home.timezone ?? { tz: "UTC", source: "fallback", offsetHours: 0 },
    },
    "GET /api/v1/integrations/solite/permit-join": {
      body: { permitJoin: false, remaining: 0 },
    },
  };
}

/** Moves the fake timers forward inside `act`, so that the page renders what the timers changed. */
export async function advance(ms: number): Promise<void> {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
}

/** An order of a device, as the tests write it. */
export interface OrderSpec {
  key: string;
  category?: OrderCategory;
  type?: DataType;
  enumValues?: string[];
}

/**
 * A device as `GET /api/v1/devices` gives it, with its data given as `[key, category, type?]`
 * (type `number` by default) and its orders.
 */
export function deviceWithData(
  id: string,
  name: string,
  data: [string, DataCategory, DataType?][],
  extra: Partial<Device> = {},
  orders: OrderSpec[] = [],
): Device & { data: DeviceData[]; orders: DeviceOrder[] } {
  return {
    ...device(id, name, extra),
    data: data.map(([key, category, type]) => ({
      id: `${id}-${key}`,
      deviceId: id,
      key,
      type: type ?? "number",
      category,
      value: 1,
      lastUpdated: null,
    })),
    orders: orders.map((o) => ({
      id: `${id}-o-${o.key}`,
      deviceId: id,
      key: o.key,
      type: o.type ?? "boolean",
      ...(o.category ? { category: o.category } : {}),
      ...(o.enumValues ? { enumValues: o.enumValues } : {}),
    })),
  };
}

/** Solite's motion-light recipe as `GET /api/v1/recipes` gives it (slots of Sowel's recipe). */
export function motionLightRecipe(): RecipeInfo {
  return {
    id: "motion-light",
    name: "Motion Light",
    description: "Turns the lights on with motion and off after a delay.",
    icon: "Lightbulb",
    i18n: {
      fr: {
        name: "Lumière automatique",
        description: "Allume avec le mouvement.",
        slots: { lights: { name: "Lumières", description: "Les lumières à piloter" } },
      },
    },
    slots: [
      { id: "zone", name: "Zone", description: "The zone", type: "zone", required: true },
      {
        id: "lights",
        name: "Lights",
        description: "Lights to drive",
        type: "equipment",
        list: true,
        required: true,
        constraints: { equipmentType: "light_onoff" },
      },
      {
        id: "timeout",
        name: "Timeout",
        description: "Delay before off",
        type: "duration",
        required: false,
        defaultValue: "10m",
      },
      {
        id: "luxThreshold",
        name: "Lux threshold",
        description: "Below this, the light may turn on",
        type: "number",
        required: false,
      },
      {
        id: "buttons",
        name: "Buttons",
        description: "Buttons that override",
        type: "equipment",
        list: true,
        required: false,
        constraints: { equipmentType: "button" },
      },
    ],
  };
}

/** An instance of a recipe as `GET /api/v1/recipe-instances` gives it. */
export function recipeInstance(
  id: string,
  params: Record<string, unknown>,
  extra: Partial<RecipeInstance> = {},
): RecipeInstance {
  return {
    id,
    recipeId: "motion-light",
    params,
    enabled: true,
    createdAt: "2026-11-21T10:00:00.000Z",
    ...extra,
  };
}

/** Solite's real device shapes (`catalog.rs`), as the interface reads them. */
export const fixtures = {
  /** The relay: data `state` (light_state), order `state` (light_toggle). */
  zbminir2: (id = "zb") =>
    deviceWithData(id, "ZBMINIR2", [["state", "light_state", "boolean"]], {}, [
      { key: "state", category: "light_toggle" },
    ]),
  th01: (id = "th") =>
    deviceWithData(id, "TH01", [
      ["temperature", "temperature"],
      ["humidity", "humidity"],
      ["battery", "battery"],
      ["voltage", "voltage"],
    ]),
  ms01: (id = "ms") =>
    deviceWithData(id, "MS01", [
      ["occupancy", "motion", "boolean"],
      ["battery", "battery"],
      ["voltage", "voltage"],
    ]),
  /** A dimmable bulb. */
  bulb: (id = "bulb") =>
    deviceWithData(
      id,
      "Bulb",
      [
        ["state", "light_state", "boolean"],
        ["brightness", "light_brightness"],
      ],
      {},
      [
        { key: "state", category: "light_toggle" },
        { key: "brightness", category: "set_brightness", type: "number" },
      ],
    ),
  /** A colour bulb (synthetic: the catalog has no colour cluster): dimmable, with a colour temperature. */
  colorBulb: (id = "color") =>
    deviceWithData(
      id,
      "Colour bulb",
      [
        ["state", "light_state", "boolean"],
        ["brightness", "light_brightness"],
        ["color_temp", "light_color_temp"],
      ],
      {},
      [
        { key: "state", category: "light_toggle" },
        { key: "brightness", category: "set_brightness", type: "number" },
        { key: "color_temp", category: "set_color_temp", type: "number" },
      ],
    ),
  /** A device with only `voltage`: nothing any type uses. */
  voltageOnly: (id = "volt") => deviceWithData(id, "Voltage only", [["voltage", "voltage"]]),
};

/** The home mounts its socket once the session is checked: waits for it, then the server accepts. */
export async function openSocket(): Promise<void> {
  await vi.waitFor(() => expect(FakeWebSocket.instances.length).toBeGreaterThan(0));
  act(() => FakeWebSocket.last.open());
}
