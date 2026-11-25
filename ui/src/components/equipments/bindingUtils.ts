// Subset of Sowel ui/src/components/equipments/bindingUtils.ts (Sowel commit 5da84dc1, UI 1.87.0):
// `findOrderByCategory`, `findMainOnOffOrder` and `relayStateOn`, copied unchanged with the
// minimal binding types they take; and, for the eight types that have Sowel's binding rules (specs
// 019 and 027), the binding plan: `RELEVANT_DATA`, `RELEVANT_ORDERS`, `BIND_ALL_ORDER_TYPES`,
// `STANDARD_ALIASES`, `ORDER_CATEGORY_ALIASES`, `DATA_CATEGORY_ALIASES`, `TYPE_CATEGORY_ALIASES`
// (the entries of those types), `resolveAlias`, `isRelevantData`, `isRelevantOrder`, `uniqueAlias` and
// `computeBindingPlan` (candidate path for lights, switches and shutters, legacy path for the
// rest). Changes: no VMC, door, contract, historize or skip options; the plan keeps at most 16 data
// and 16 order bindings and aliases of at most 32 characters (the API's bounds); `applyBindingPlan`
// stops at the first refusal and throws it (Sowel's carries on). Spec 030, the equipment page:
// `computeMissingBindings` (Sowel's name; the plan of the bound devices minus what is bound, aliases
// unique against the bound ones, the 16 per kind counting the bound ones), `fetchDevices` and
// `removeAllBindings` (which stops at the first refusal, as `applyBindingPlan`). The rest of
// Sowel's file (the EV contracts) is not used by Solite's screens.
import type {
  DataBindingWithValue,
  DataCategory,
  EquipmentType,
  OrderBindingWithDetails,
  OrderCategory,
} from "../../types";
import { getDevice, type DeviceWithData } from "../../api/devices";
import {
  addDataBinding,
  addOrderBinding,
  removeDataBinding,
  removeOrderBinding,
} from "../../api/equipments";
import {
  CANDIDATE_BASED_TYPES,
  THERMOSTAT_STATE_ALIAS,
  computeBindingCandidates,
} from "../../lib/binding-candidates";

interface MinimalOrderBinding {
  alias: string;
  category?: OrderCategory;
}

interface MinimalDataBinding {
  alias: string;
  category?: DataCategory;
}

/**
 * Find an order binding by category, with optional alias and regex fallbacks.
 *
 * Resolution order:
 *   1. First binding whose `category` is in `categories`.
 *   2. First binding whose `alias` is in `aliasFallbacks` (legacy
 *      compatibility for bindings that pre-date category typing).
 *   3. First binding whose `alias` matches any of `aliasPatterns` (e.g.
 *      `^shutter\d*_state$` for indexed channels).
 *
 * Always pass at least one category. The fallbacks are for migration —
 * once every plugin emits a category and every binding carries one,
 * fallbacks can be dropped.
 *
 * @example
 * const move = findOrderByCategory(
 *   eq.orderBindings,
 *   ["pool_cover_move", "shutter_move"],
 *   ["state"],
 *   [/^shutter\d*_state$/],
 * );
 * if (move) await onExecuteOrder(move.alias, "OPEN");
 */
export function findOrderByCategory<T extends MinimalOrderBinding>(
  bindings: readonly T[],
  categories: readonly OrderCategory[],
  aliasFallbacks?: readonly string[],
  aliasPatterns?: readonly RegExp[],
): T | undefined {
  const byCategory = bindings.find(
    (b) => b.category !== undefined && categories.includes(b.category),
  );
  if (byCategory) return byCategory;
  if (aliasFallbacks && aliasFallbacks.length > 0) {
    const byAlias = bindings.find((b) => aliasFallbacks.includes(b.alias));
    if (byAlias) return byAlias;
  }
  if (aliasPatterns && aliasPatterns.length > 0) {
    return bindings.find((b) => aliasPatterns.some((re) => re.test(b.alias)));
  }
  return undefined;
}

/**
 * Spec 152 — resolve the MAIN on/off command of a light/switch/water_heater,
 * mirroring exactly what `LightControl` binds to (category-first, then the loose
 * boolean/`state`-enum fallback), while never matching the dedicated solar
 * channel. Callers use it as the render gate for the main toggle so the gate and
 * the control resolve identically (no "gate hides a toggle LightControl would
 * still render" regression for legacy uncategorized bindings).
 */
export function findMainOnOffOrder<T extends MinimalOrderBinding & { type?: string }>(
  bindings: readonly T[],
): T | undefined {
  return (
    findOrderByCategory(bindings, ["light_toggle", "toggle_power"], ["state"]) ??
    bindings.find(
      (b) =>
        b.alias !== "solar" &&
        b.category !== "solar_toggle" &&
        (b.type === "boolean" || (b.alias === "state" && b.type === "enum")),
    )
  );
}

/**
 * Whether a relay-style equipment (light, heater, plug, valve, pump) is on: the
 * first binding with alias `state` or category `light_state`, `true` or « ON »
 * in any case. The controls and the row's tile (spec 208) read it here, so they
 * never disagree. False when there is no such binding.
 */
export function relayStateOn(
  bindings: readonly (MinimalDataBinding & { value?: unknown })[],
): boolean {
  const state = bindings.find((b) => b.alias === "state" || b.category === "light_state");
  if (!state) return false;
  return state.value === true || String(state.value).toUpperCase() === "ON";
}

// ============================================================
// Binding plan (spec 019)
// ============================================================

/** The API's bound on the data bindings, and on the order bindings, of one equipment. */
export const MAX_BINDINGS = 16;
/** The API's bound on an alias (`Invalid alias` past it). */
export const MAX_ALIAS = 32;

/** Maps equipment types to relevant data categories for auto-binding. */
const RELEVANT_DATA: Record<string, string[]> = {
  light_onoff: ["light_state"],
  light_dimmable: ["light_state", "light_brightness"],
  light_color: ["light_state", "light_brightness", "light_color", "light_color_temp"],
  shutter: ["shutter_position"],
  switch: ["light_state"],
  sensor: [
    "temperature",
    "humidity",
    "pressure",
    "luminosity",
    "co2",
    "voc",
    "noise",
    "motion",
    "contact_door",
    "contact_window",
    "water_leak",
    "smoke",
    "battery",
  ],
  button: ["action", "battery"],
  thermostat: [
    "temperature",
    "temperature_outdoor",
    "setpoint",
    "power",
    "operation_mode",
    "generic",
  ],
};

/** Maps equipment types to relevant order keys for auto-binding. */
const RELEVANT_ORDERS: Record<string, string[]> = {
  light_onoff: ["state", "on", "R1", "R2", "R3", "R4"],
  light_dimmable: ["state", "on", "brightness", "R1", "R2", "R3", "R4"],
  light_color: ["state", "on", "brightness", "color", "color_temp", "R1", "R2", "R3", "R4"],
  shutter: ["position", "state", "target_position"],
  switch: ["state", "on", "R1", "R2", "R3", "R4"],
  button: [],
  sensor: [],
};

/** Types that bind EVERY order their device exposes (Sowel's spec 177). */
const BIND_ALL_ORDER_TYPES: ReadonlySet<EquipmentType> = new Set<EquipmentType>(["thermostat"]);

/** Per-type key aliases (legacy relays and a thermostat's vendor keys). */
const STANDARD_ALIASES: Record<string, Record<string, string>> = {
  thermostat: {
    targetTemperature: "setpoint",
    insideTemperature: "temperature",
  },
  light_onoff: { R1: "state", R2: "state", R3: "state", R4: "state" },
  light_dimmable: { R1: "state", R2: "state", R3: "state", R4: "state" },
  light_color: { R1: "state", R2: "state", R3: "state", R4: "state" },
  switch: { R1: "state", R2: "state", R3: "state", R4: "state" },
};

/** Order category to alias, plugin-agnostic. */
export const ORDER_CATEGORY_ALIASES: Record<string, string> = {
  light_toggle: "state",
  toggle_power: "state",
  shutter_move: "state",
  set_shutter_position: "position",
  set_brightness: "brightness",
  set_color_temp: "color_temp",
  set_color: "color",
  set_setpoint: "setpoint",
};

/** Data category to alias, plugin-agnostic. */
export const DATA_CATEGORY_ALIASES: Record<string, string> = {
  light_state: "state",
  shutter_position: "position",
  light_brightness: "brightness",
  light_color_temp: "color_temp",
  light_color: "color",
  setpoint: "setpoint",
  battery: "battery",
};

/** Per-type category to alias overrides, applied before the global maps. */
const TYPE_CATEGORY_ALIASES: Partial<Record<EquipmentType, Record<string, string>>> = {
  thermostat: {
    toggle_power: "power",
    temperature: "temperature",
    temperature_outdoor: "outsideTemperature",
    operation_mode: "operationMode",
    set_operation_mode: "operationMode",
  },
};

/**
 * Resolve a device key to the alias for the given type. Priority: a thermostat's boolean power
 * reading; the category (per type, then global); the per-type key map; the key itself.
 */
export function resolveAlias(
  key: string,
  equipmentType: string,
  categoryMap?: Record<string, string>,
  category?: string,
  valueType?: string,
): string {
  if (equipmentType === "thermostat" && category === "power" && valueType === "boolean") {
    return THERMOSTAT_STATE_ALIAS;
  }
  if (category) {
    const perType = TYPE_CATEGORY_ALIASES[equipmentType as EquipmentType]?.[category];
    if (perType) return perType;
    if (categoryMap && categoryMap[category]) return categoryMap[category];
  }
  return STANDARD_ALIASES[equipmentType]?.[key] ?? key;
}

export function isRelevantData(category: string, equipmentType: string): boolean {
  return RELEVANT_DATA[equipmentType]?.includes(category) ?? false;
}

export function isRelevantOrder(key: string, equipmentType: string): boolean {
  if (BIND_ALL_ORDER_TYPES.has(equipmentType as EquipmentType)) return true;
  return RELEVANT_ORDERS[equipmentType]?.includes(key) ?? false;
}

/** One binding the form would create. */
export interface PlannedBinding {
  kind: "data" | "order";
  deviceId: string;
  /** `device_data.id` for data, `device_order.id` for orders. */
  sourceId: string;
  /** The key as the device publishes it. */
  key: string;
  /** The alias it is bound under, already made unique. */
  alias: string;
  category?: string;
}

/** Return a unique alias: "battery", "battery_2", "battery_3", etc. */
function uniqueAlias(base: string, used: Set<string>): string {
  if (!used.has(base)) return base;
  let n = 2;
  while (used.has(`${base}_${n}`)) n++;
  return `${base}_${n}`;
}

/** What an equipment already has, when the plan is for the rest (the missing bindings). */
interface PlanOptions {
  usedDataAliases: readonly string[];
  usedOrderAliases: readonly string[];
  /** `device_data.id` and `device_order.id` already bound: not planned again. */
  skipSourceIds: ReadonlySet<string>;
}

/** The bindings of these devices, in the order given, before the 16 and 16 cut. */
function planAll(
  devices: readonly DeviceWithData[],
  equipmentType: string,
  options?: PlanOptions,
): PlannedBinding[] {
  const usedData = new Set<string>(options?.usedDataAliases);
  const usedOrders = new Set<string>(options?.usedOrderAliases);
  const plan: PlannedBinding[] = [];

  const push = (
    kind: "data" | "order",
    deviceId: string,
    item: { id: string; key: string; category?: string },
    base: string,
  ) => {
    if (options?.skipSourceIds.has(item.id)) return;
    const used = kind === "data" ? usedData : usedOrders;
    const alias = uniqueAlias(base, used);
    // An alias the API would refuse is left out, and does not take the name.
    if (alias.length > MAX_ALIAS) return;
    used.add(alias);
    plan.push({
      kind,
      deviceId,
      sourceId: item.id,
      key: item.key,
      alias,
      ...(item.category !== undefined ? { category: item.category } : {}),
    });
  };

  for (const device of devices) {
    const data = device.data ?? [];
    const orders = device.orders ?? [];
    if (CANDIDATE_BASED_TYPES.has(equipmentType as EquipmentType)) {
      const candidates = computeBindingCandidates(equipmentType as EquipmentType, data, orders);
      // No matching channel: the device gives nothing; several: the first one (Solite's devices have one).
      const chosen = candidates[0];
      if (!chosen) continue;
      const allowedData = new Set(chosen.dataKeys);
      const allowedOrders = new Set(chosen.orderKeys);
      for (const d of data) {
        if (!allowedData.has(d.key)) continue;
        push(
          "data",
          device.id,
          d,
          resolveAlias(d.key, equipmentType, DATA_CATEGORY_ALIASES, d.category, d.type),
        );
      }
      for (const o of orders) {
        if (!allowedOrders.has(o.key)) continue;
        push(
          "order",
          device.id,
          o,
          resolveAlias(o.key, equipmentType, ORDER_CATEGORY_ALIASES, o.category),
        );
      }
      continue;
    }
    for (const d of data) {
      if (!isRelevantData(d.category, equipmentType)) continue;
      push(
        "data",
        device.id,
        d,
        resolveAlias(d.key, equipmentType, DATA_CATEGORY_ALIASES, d.category, d.type),
      );
    }
    for (const o of orders) {
      if (!isRelevantOrder(o.key, equipmentType)) continue;
      push(
        "order",
        device.id,
        o,
        resolveAlias(o.key, equipmentType, ORDER_CATEGORY_ALIASES, o.category),
      );
    }
  }
  return plan;
}

/** How many data and orders these devices would give, before the cut at 16 each. */
export function countPlanned(
  devices: readonly DeviceWithData[],
  equipmentType: string,
): { data: number; orders: number } {
  const all = planAll(devices, equipmentType);
  const data = all.filter((b) => b.kind === "data").length;
  return { data, orders: all.length - data };
}

/**
 * The bindings to create for these devices (given in the order they were ticked): what the type
 * uses, under Sowel's aliases, data first, at most 16 of each kind. Pure.
 */
export function computeBindingPlan(
  devices: readonly DeviceWithData[],
  equipmentType: string,
): PlannedBinding[] {
  const all = planAll(devices, equipmentType);
  return [
    ...all.filter((b) => b.kind === "data").slice(0, MAX_BINDINGS),
    ...all.filter((b) => b.kind === "order").slice(0, MAX_BINDINGS),
  ];
}

/**
 * Write a plan, one request after the previous one answered, data before orders. The first
 * refusal stops it and is thrown (the caller deletes the equipment again).
 */
export async function applyBindingPlan(
  equipmentId: string,
  plan: readonly PlannedBinding[],
): Promise<void> {
  for (const item of plan) {
    if (item.kind === "data") {
      await addDataBinding(equipmentId, { deviceDataId: item.sourceId, alias: item.alias });
    } else {
      await addOrderBinding(equipmentId, { deviceOrderId: item.sourceId, alias: item.alias });
    }
  }
}

/**
 * What the plan of the creation form would add today to an equipment: the plan of its bound
 * devices (in the order they are first bound), minus the items already bound, with aliases unique
 * against those it already uses, and at most 16 of each kind counting the bound ones. Pure.
 */
export function computeMissingBindings(
  devices: readonly DeviceWithData[],
  equipmentType: string,
  dataBindings: readonly DataBindingWithValue[],
  orderBindings: readonly OrderBindingWithDetails[],
): PlannedBinding[] {
  const all = planAll(devices, equipmentType, {
    usedDataAliases: dataBindings.map((b) => b.alias),
    usedOrderAliases: orderBindings.map((b) => b.alias),
    skipSourceIds: new Set([
      ...dataBindings.map((b) => b.deviceDataId),
      ...orderBindings.map((b) => b.deviceOrderId),
    ]),
  });
  return [
    ...all
      .filter((b) => b.kind === "data")
      .slice(0, Math.max(0, MAX_BINDINGS - dataBindings.length)),
    ...all
      .filter((b) => b.kind === "order")
      .slice(0, Math.max(0, MAX_BINDINGS - orderBindings.length)),
  ];
}

/** The devices behind these ids with their data and orders, one read after the other; one that cannot be read is skipped. */
export async function fetchDevices(deviceIds: readonly string[]): Promise<DeviceWithData[]> {
  const devices: DeviceWithData[] = [];
  for (const id of deviceIds) {
    try {
      devices.push(await getDevice(id));
    } catch {
      // Skipped, as Sowel's: its items are not offered.
    }
  }
  return devices;
}

/**
 * Removes every binding of an equipment, data first, one request after the previous one answered.
 * The first refusal stops it and is thrown, so no binding is written over one that could not be
 * removed (Sowel's carries on).
 */
export async function removeAllBindings(
  equipmentId: string,
  dataBindings: readonly { id: string }[],
  orderBindings: readonly { id: string }[],
): Promise<void> {
  for (const b of dataBindings) await removeDataBinding(equipmentId, b.id);
  for (const b of orderBindings) await removeOrderBinding(equipmentId, b.id);
}
