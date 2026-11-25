// Solite's own file: the live connection to the host's `/ws` (spec 013), written for the
// topics the screen needs. Sowel's ui/src/store/useWebSocket.ts is a monolith of 645 lines that
// imports 11 stores; this one keeps its behaviour (subprotocol, batched events, reconnection with
// backoff and jitter, refetch on every open, reconnection when the page comes back) and nothing else.
import { create } from "zustand";
import type { Device, DeviceStatus, EquipmentStatus, Zone, ZoneAggregatedData } from "../types";
import { useDeviceDetail } from "./useDeviceDetail";
import { useDevices } from "./useDevices";
import { usePairing } from "./usePairing";
import { useEquipments } from "./useEquipments";
import { useZoneAggregation } from "./useZoneAggregation";
import { useZones } from "./useZones";
import { useRecipes } from "./useRecipes";
import { coalesceCalls } from "../lib/coalesce-calls";
import { getAccessToken } from "../api/client";
import { useAuth } from "./useAuth";

/** What the screen is subscribed to. */
const TOPICS = ["zones", "equipments", "devices", "recipes"];
/**
 * The subprotocols offered (spec 029, R5): `bearer.none`, the one the box echoes, then
 * `bearer.<access token>`, which the host reads for the token.
 */
function subprotocols(): string[] {
  const token = getAccessToken();
  return token ? ["bearer.none", `bearer.${token}`] : ["bearer.none"];
}
/** The close code of a missing or refused token (Sowel's). */
const CLOSE_UNAUTHORIZED = 4001;
const MAX_RECONNECT_DELAY = 30_000;

export type LiveStatus = "connecting" | "open" | "closed";

interface LiveState {
  status: LiveStatus;
}

/** The events of Sowel's `EngineEvent` (ui/src/types.ts) that the screen applies. */
type LiveEvent =
  | { type: "zone.created" | "zone.updated"; zone: Zone }
  | { type: "zone.removed"; zoneId: string }
  | { type: "zone.data.changed"; zoneId: string; aggregatedData: ZoneAggregatedData }
  | { type: "equipment.created" | "equipment.updated" | "equipment.removed" }
  | { type: "equipment.data.changed"; equipmentId: string; alias: string; value: unknown }
  | { type: "equipment.status.changed"; equipmentId: string; newStatus: EquipmentStatus }
  | { type: "device.discovered"; device: Device }
  | {
      type: "device.data.updated";
      deviceId: string;
      dataId: string;
      key: string;
      value: unknown;
      timestamp: string;
    }
  | { type: "device.removed"; deviceId: string }
  | { type: "device.status_changed"; deviceId: string; status: DeviceStatus }
  | {
      type:
        | "recipe.instance.created"
        | "recipe.instance.removed"
        | "recipe.instance.started"
        | "recipe.instance.stopped"
        | "recipe.instance.state.changed"
        | "recipe.instance.error";
    };

export const useLive = create<LiveState>(() => ({ status: "connecting" }));

let socket: WebSocket | null = null;
let timer: ReturnType<typeof setTimeout> | null = null;
let attempts = 0;
let hooksInstalled = false;
/** A 4001 was answered by a refresh and no socket has opened since: the next one backs off. */
let refreshedOnClose = false;

function setStatus(status: LiveStatus): void {
  useLive.setState({ status });
}

/** `min(1000 × 2^n, 30000) + random(0..500)` ms, as Sowel's client. */
export function reconnectDelay(attempt: number): number {
  return Math.min(1000 * Math.pow(2, attempt), MAX_RECONNECT_DELAY) + Math.random() * 500;
}

function socketUrl(): string {
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${protocol}//${window.location.host}/ws`;
}

/** Recipe events come in bursts (a state change per tick): one read per 1 s window (spec 024, R3). */
const refetchInstances = coalesceCalls(() => {
  void useRecipes.getState().fetchInstances();
}, 1000);

/** The stores read again: events may have been lost while the socket was closed (spec 013 closes slow clients). */
function refetchAll(): void {
  void useZones.getState().fetchZones();
  void useEquipments.getState().fetchEquipments();
  void useZoneAggregation.getState().fetchAggregation();
  void useDevices.getState().fetchDevices();
  void usePairing.getState().refresh();
  void useRecipes.getState().fetchRecipes();
  void useRecipes.getState().fetchInstances();
  void useDeviceDetail.getState().reload();
}

function apply(event: LiveEvent): void {
  switch (event.type) {
    case "zone.created":
      useZones.getState().handleZoneCreated(event.zone);
      break;
    case "zone.updated":
      useZones.getState().handleZoneUpdated(event.zone);
      break;
    case "zone.removed":
      useZones.getState().handleZoneRemoved(event.zoneId);
      break;
    case "zone.data.changed":
      useZoneAggregation.getState().handleZoneDataChanged(event.zoneId, event.aggregatedData);
      break;
    case "equipment.created":
      useEquipments.getState().handleEquipmentCreated();
      break;
    case "equipment.updated":
      useEquipments.getState().handleEquipmentUpdated();
      break;
    case "equipment.removed":
      useEquipments.getState().handleEquipmentRemoved();
      break;
    case "equipment.data.changed":
      useEquipments
        .getState()
        .handleEquipmentDataChanged(event.equipmentId, event.alias, event.value);
      break;
    case "equipment.status.changed":
      useEquipments.getState().handleEquipmentStatusChanged(event.equipmentId, event.newStatus);
      break;
    case "device.discovered":
      useDevices.getState().addDevice(event.device);
      usePairing.getState().markNew(event.device.id);
      break;
    case "device.removed":
      useDevices.getState().removeDevice(event.deviceId);
      useDeviceDetail.getState().handleRemoved(event.deviceId);
      break;
    case "device.data.updated":
      useDeviceDetail.getState().handleDataUpdated(event);
      break;
    case "device.status_changed":
      useDevices.getState().updateDeviceStatus(event.deviceId, event.status);
      break;
    case "recipe.instance.created":
    case "recipe.instance.removed":
    case "recipe.instance.started":
    case "recipe.instance.stopped":
    case "recipe.instance.state.changed":
    case "recipe.instance.error":
      refetchInstances();
      break;
  }
}

function isEvent(value: unknown): value is LiveEvent {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as { type?: unknown }).type === "string"
  );
}

/** An array of events or a single one; anything else is ignored. */
function onMessage(data: unknown): void {
  try {
    const parsed: unknown = JSON.parse(String(data));
    for (const event of Array.isArray(parsed) ? parsed : [parsed]) {
      if (isEvent(event)) apply(event);
    }
  } catch {
    // Ignore malformed messages.
  }
}

/** Reconnects at once when the page comes back or the network does, if the socket is gone. */
function wake(): void {
  if (socket) return;
  connect();
}

function installWakeHooks(): void {
  if (hooksInstalled) return;
  hooksInstalled = true;
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") wake();
  });
  window.addEventListener("online", wake);
}

/** Opens the socket (a no-op while one is open or opening). The stores fetch when it opens. */
export function connect(): void {
  installWakeHooks();
  if (socket) return;
  if (timer) {
    clearTimeout(timer);
    timer = null;
  }
  const current = new WebSocket(socketUrl(), subprotocols());
  socket = current;
  current.onopen = () => {
    attempts = 0;
    refreshedOnClose = false;
    setStatus("open");
    current.send(JSON.stringify({ type: "subscribe", topics: TOPICS }));
    refetchAll();
  };
  current.onmessage = (message) => onMessage(message.data);
  current.onclose = (event) => {
    if (socket === current) socket = null;
    setStatus("closed");
    if (event?.code === CLOSE_UNAUTHORIZED && !refreshedOnClose) {
      // The token is expired or refused: a new one, then the socket again. A refused refresh
      // shows the login screen (the auth store), and the home closes this connection.
      refreshedOnClose = true;
      void useAuth
        .getState()
        .refreshSession()
        .then((ok) => {
          if (ok && !socket && !timer) connect();
        });
      return;
    }
    timer = setTimeout(() => {
      timer = null;
      attempts++;
      connect();
    }, reconnectDelay(attempts));
  };
  current.onerror = () => {
    // `onclose` follows.
  };
}

/** Closes the socket and cancels the reconnection (tests; the page itself never leaves). */
export function disconnect(): void {
  if (timer) {
    clearTimeout(timer);
    timer = null;
  }
  const current = socket;
  socket = null;
  attempts = 0;
  refreshedOnClose = false;
  if (current) {
    current.onclose = null;
    current.close();
  }
  setStatus("connecting");
}
