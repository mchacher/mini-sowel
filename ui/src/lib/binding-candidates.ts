// Ported from Sowel's `src/shared/binding-candidates.ts` and `thermostat-contract.ts` (Sowel
// commit 5da84dc1), for decision D3's seven types (spec 019). Cut: every type but `light_onoff`,
// `light_dimmable`, `switch` and `shutter`; the metering attach of a single-channel switch; the
// `shutter_*` key convention (no Solite device has it: only Sowel's category fallback stays); the
// door, VMC and solar rules. Structural input types, as in Sowel.
import type { DataCategory, DataType, EquipmentType, OrderCategory } from "../types";

/** Structural subset of a device data this module needs. */
export interface CandidateData {
  key: string;
  category: DataCategory;
  type?: DataType;
  enumValues?: string[] | null;
}

/** Structural subset of a device order this module needs. */
export interface CandidateOrder {
  key: string;
  type: DataType;
  category?: OrderCategory | null;
  enumValues?: string[] | null;
}

/** One functional channel of a device: the data and orders that make one equipment's bindings. */
export interface BindingCandidate {
  id: string;
  label: string;
  dataKeys: string[];
  orderKeys: string[];
}

/** The types whose compatibility and binding follow the candidates (Sowel's list, cut to D3's). */
export const CANDIDATE_BASED_TYPES: ReadonlySet<EquipmentType> = new Set<EquipmentType>([
  "light_onoff",
  "light_dimmable",
  "switch",
  "shutter",
]);

const ONOFF_TOKENS = new Set(["ON", "OFF", "TOGGLE"]);
const POWER_TOGGLE_CATEGORIES = new Set<OrderCategory>(["light_toggle", "toggle_power"]);

function isOnOffEnum(order: CandidateOrder): boolean {
  if (order.type !== "enum") return false;
  if (!order.enumValues || order.enumValues.length === 0) return false;
  return order.enumValues.every((v) => typeof v === "string" && ONOFF_TOKENS.has(v.toUpperCase()));
}

/**
 * True for an on/off command channel: an ON/OFF enum order, or a boolean order of category
 * `light_toggle` or `toggle_power` (Zigbee2MQTT's relay `state`). A boolean order of another
 * category (a configuration toggle) is not one.
 */
export function isOnOffOrder(order: CandidateOrder): boolean {
  if (isOnOffEnum(order)) return true;
  return (
    order.type === "boolean" && !!order.category && POWER_TOGGLE_CATEGORIES.has(order.category)
  );
}

/** The candidates of a device for an equipment type (empty for a type that is not candidate-based). */
export function computeBindingCandidates(
  equipmentType: EquipmentType,
  deviceData: readonly CandidateData[],
  deviceOrders: readonly CandidateOrder[],
): BindingCandidate[] {
  switch (equipmentType) {
    case "switch":
    case "light_onoff": {
      // One candidate per on/off command channel, with the data of the same key.
      const candidates: BindingCandidate[] = [];
      for (const o of deviceOrders) {
        if (!isOnOffOrder(o)) continue;
        const matchingData = deviceData.find((d) => d.key === o.key);
        candidates.push({
          id: o.key,
          label: o.key,
          dataKeys: matchingData ? [matchingData.key] : [],
          orderKeys: [o.key],
        });
      }
      return candidates;
    }
    case "light_dimmable": {
      // An on/off order, with the brightness orders and the data of the same keys attached.
      const candidates: BindingCandidate[] = [];
      const brightnessOrders = deviceOrders.filter((o) => o.key.includes("brightness"));
      for (const o of deviceOrders) {
        if (!isOnOffOrder(o)) continue;
        const dataKeys: string[] = [];
        const orderKeys = [o.key];
        const matchingData = deviceData.find((d) => d.key === o.key);
        if (matchingData) dataKeys.push(matchingData.key);
        for (const b of brightnessOrders) {
          orderKeys.push(b.key);
          const bd = deviceData.find((d) => d.key === b.key);
          if (bd) dataKeys.push(bd.key);
        }
        candidates.push({ id: o.key, label: o.key, dataKeys, orderKeys });
      }
      return candidates;
    }
    case "shutter": {
      // Sowel's category fallback: one channel, whatever the keys are called.
      const dataKeys = deviceData
        .filter((d) => d.category === "shutter_position")
        .map((d) => d.key);
      const orderKeys = deviceOrders
        .filter((o) => o.category === "set_shutter_position" || o.category === "shutter_move")
        .map((o) => o.key);
      if (orderKeys.length === 0) return [];
      return [{ id: "shutter1", label: "Shutter", dataKeys, orderKeys }];
    }
    default:
      return [];
  }
}

/** A thermostat's boolean run state is bound under this alias (Sowel's `THERMOSTAT_STATE_ALIAS`). */
export const THERMOSTAT_STATE_ALIAS = "state";

/** Sowel's thermostat identity: a `setpoint` data or a `set_setpoint` order. */
export function isThermostatDevice(
  data: readonly { category?: string | null }[],
  orders: readonly { category?: string | null }[],
): boolean {
  return (
    data.some((d) => d.category === "setpoint") || orders.some((o) => o.category === "set_setpoint")
  );
}
