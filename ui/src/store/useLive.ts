// mini-Sowel's own file: the live connection to the host's `/ws` (spec 013), written for the
// two topics the screen needs. Sowel's ui/src/store/useWebSocket.ts is a monolith of 645 lines that
// imports 11 stores; this one keeps its behaviour (subprotocol, batched events, reconnection with
// backoff and jitter, refetch on every open, reconnection when the page comes back) and nothing else.
import { create } from "zustand";
import type { EquipmentStatus, Zone, ZoneAggregatedData } from "../types";
import { useEquipments } from "./useEquipments";
import { useZoneAggregation } from "./useZoneAggregation";
import { useZones } from "./useZones";

/** What the screen is subscribed to. */
const TOPICS = ["zones", "equipments"];
/** The host takes no token (decision D13); the subprotocol is still sent, as Sowel's client does. */
const SUBPROTOCOL = "bearer.none";
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
  | { type: "equipment.status.changed"; equipmentId: string; newStatus: EquipmentStatus };

export const useLive = create<LiveState>(() => ({ status: "connecting" }));

let socket: WebSocket | null = null;
let timer: ReturnType<typeof setTimeout> | null = null;
let attempts = 0;
let hooksInstalled = false;

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

/** The stores read again: events may have been lost while the socket was closed (spec 013 closes slow clients). */
function refetchAll(): void {
  void useZones.getState().fetchZones();
  void useEquipments.getState().fetchEquipments();
  void useZoneAggregation.getState().fetchAggregation();
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
  const current = new WebSocket(socketUrl(), SUBPROTOCOL);
  socket = current;
  current.onopen = () => {
    attempts = 0;
    setStatus("open");
    current.send(JSON.stringify({ type: "subscribe", topics: TOPICS }));
    refetchAll();
  };
  current.onmessage = (message) => onMessage(message.data);
  current.onclose = () => {
    if (socket === current) socket = null;
    setStatus("closed");
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
  if (current) {
    current.onclose = null;
    current.close();
  }
  setStatus("connecting");
}
