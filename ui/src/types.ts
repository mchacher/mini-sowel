// Subset of Sowel ui/src/types.ts (Sowel commit 5da84dc1, UI 1.87.0), each declaration copied
// unchanged: Zone, ZoneWithChildren, ZoneAggregatedData, Equipment, EquipmentWithDetails,
// EquipmentType, EquipmentFamily, EquipmentStatus, EquipmentStatusReason, DataBindingWithValue,
// OrderBindingWithDetails, BatteryAlert, and what they name (DataCategory, OrderCategory...).
// The file is Sowel's model, which Solite's API answers (decision D3).

// ============================================================
// Data Types (mirrors src/shared/types.ts)
// ============================================================

export type DataType = "boolean" | "number" | "enum" | "text" | "json";

export type DataCategory =
  | "motion"
  | "temperature"
  | "humidity"
  | "pressure"
  | "luminosity"
  | "contact_door"
  | "contact_window"
  | "light_state"
  | "light_brightness"
  | "light_color_temp"
  | "light_color"
  | "shutter_position"
  | "lock_state"
  | "battery"
  | "power"
  | "energy"
  | "voltage"
  | "current"
  | "water_leak"
  | "smoke"
  | "co2"
  | "voc"
  | "noise"
  | "rain"
  | "wind"
  | "action"
  | "gate_state"
  | "cover_state"
  | "runtime_daily"
  | "weather_condition"
  | "uv"
  | "solar_radiation"
  | "setpoint"
  | "temperature_outdoor"
  | "temperature_device"
  | "humidity_outdoor"
  | "media_volume"
  | "media_mute"
  | "media_input"
  | "appliance_state"
  // Spec 152 — solar command channel state feedback (states family, spec 144).
  | "solar_state"
  | "pool_water_temperature"
  | "pool_temperature_setpoint"
  // Spec 120 — display equipment telemetry.
  | "firmware_version"
  | "uptime"
  | "rssi"
  | "language"
  | "display_brightness"
  // Spec 133 — camera equipment. Vendor-agnostic.
  | "camera_snapshot_url"
  | "camera_stream_url"
  | "camera_monitoring"
  | "camera_light_mode"
  | "camera_detection"
  // Spec 156 — UPS. `ups_status` is a closed, severity-ordered enum;
  // `battery_runtime` is the remaining autonomy in seconds; `ups_load` is the
  // output load as a percentage of nominal power (never `power` — spec 156 FR3).
  | "ups_status"
  | "battery_runtime"
  | "ups_load"
  // Spec 177 — a thermostat's operating mode. Closed enum, values in
  // OPERATION_MODE_VALUES (thermostat-contract.ts): auto / heat / cool / dry /
  // fan, plus `off` for units whose mode carries the stop. A pellet stove's
  // programme (`profile`) is NOT a mode: it stays an extra.
  | "operation_mode"
  // Spec 182 — EV charger. `ev_vehicle_state` is a closed enum
  // (EV_VEHICLE_STATE_VALUES: IEC 61851 A/B/C); the charging current the
  // charger is set to (A, not a measurement); energy since the vehicle was
  // plugged in (kWh, resets each session — NOT the additive `energy`).
  | "ev_vehicle_state"
  | "ev_charge_current"
  | "ev_session_energy"
  // Spec 183 — electric vehicle. `ev_battery_level` is the TRACTION battery
  // (%), deliberately not `battery` (a device battery, spec 143).
  // `ev_charging_state` is a closed enum (EV_CHARGING_STATE_VALUES);
  // `ev_reported_at` is when the car itself last reported (ISO string).
  | "ev_battery_level"
  | "ev_range"
  | "ev_plugged"
  | "ev_charging_state"
  | "ev_reported_at"
  | "ev_at_home"
  | "ev_mileage"
  | "ev_charge_limit"
  // Spec 194 — the power the car says it charges at (W).
  | "ev_charge_power"
  | "generic";

export type OrderCategory =
  | "light_toggle"
  | "set_brightness"
  | "set_color_temp"
  | "set_color"
  | "shutter_move"
  | "set_shutter_position"
  | "toggle_power"
  | "set_setpoint"
  // Spec 177 — set a thermostat's operating mode (values: OPERATION_MODE_VALUES).
  | "set_operation_mode"
  | "gate_trigger"
  | "valve_toggle"
  | "toggle_mute"
  | "set_input"
  | "pool_pump_toggle"
  | "pool_cover_move"
  | "pool_cover_position"
  | "set_pool_temperature_setpoint"
  // Spec 120 — display equipment.
  | "set_language"
  | "set_display_brightness"
  // Spec 122 — display wake action. No value: the firmware restores its
  // own last user-chosen brightness. Used by presence-driven recipes so
  // the recipe does not need to know the user's preferred level.
  | "display_wake"
  // Spec 133 — camera equipment.
  | "set_camera_monitoring"
  | "set_camera_light_mode"
  | "trigger_camera_siren"
  // Spec 152 — solar force command channel (dedicated on/off, distinct from the
  // main on/off; driven by the surplus arbiter recipe).
  | "solar_toggle"
  // Spec 182 — set an EV charger's charging current (A, bounded by the
  // order's min/max). Not `set_setpoint`: that is the thermostat identity.
  | "set_ev_charge_current"
  // Spec 183 — electric vehicle: wake it and start a charge (momentary, no
  // value), and set the charge limit the car applies by itself (%).
  | "ev_wake"
  | "ev_charge_start"
  // Spec 184 — read the car's latest report now (momentary, never wakes it).
  | "ev_refresh"
  | "set_ev_charge_limit";

/** An active low-battery alert (spec 143). */
export interface BatteryAlert {
  deviceDataId: string;
  deviceId: string;
  deviceName: string;
  /** Raw low value: "12" for a percentage, "true" for a battery_low flag. */
  value: string;
  raisedAt: string;
  lastNotifiedAt: string;
  /** Names of the equipments bound to this device (spec 143/#472); empty when
   *  unbound. Lets the banner show the equipment, not just the device. */
  equipmentNames?: string[];
  /** Zone of the first bound equipment, null when unbound. */
  zoneId?: string | null;
}

// ============================================================
// Zone
// ============================================================

export interface Zone {
  id: string;
  name: string;
  parentId: string | null;
  icon?: string;
  description?: string;
  displayOrder: number;
  createdAt: string;
  updatedAt: string;
}

export interface ZoneWithChildren extends Zone {
  children: ZoneWithChildren[];
}

export interface ZoneAggregatedData {
  temperature: number | null;
  humidity: number | null;
  luminosity: number | null;
  motion: boolean;
  motionSensors: number;
  motionSince: string | null;
  openDoors: number;
  openWindows: number;
  waterLeak: boolean;
  smoke: boolean;
  lightsOn: number;
  lightsTotal: number;
  shuttersOpen: number;
  shuttersTotal: number;
  averageShutterPosition: number | null;
  /** Spec 190 — `door` equipments not proven closed, and their total. */
  doorsOpen: number;
  doorsTotal: number;
  waterValvesOpen: number;
  waterValvesTotal: number;
  waterFlowTotal: number | null;
  /** Spec 170 — live sum, in whole watts, of the consumption submeters in this
   *  zone and its descendants. `null` when there was nothing current to sum,
   *  which is not the same as a measured 0 W. */
  powerTotal: number | null;
  sunrise: string | null;
  sunset: string | null;
  isDaylight: boolean | null;
  /** Spec 120 — count of `display` equipments in this zone (+ descendants)
   *  whose EquipmentStatus === "online" vs total. */
  displaysOnline: number;
  displaysTotal: number;
  /** Per-DataCategory count of equipments excluded from aggregation because
   *  status === "offline" (spec 116). Kept for API clients; the UI marks
   *  the affected pill from `unavailableEquipmentNamesByCategory`. */
  unavailableEquipmentsByCategory: Partial<Record<DataCategory, number>>;
  /** #1134 — names of those equipments, sorted, one entry per equipment (same keys). */
  unavailableEquipmentNamesByCategory: Partial<Record<DataCategory, string[]>>;
}

// ============================================================
// Equipment
// ============================================================

export type EquipmentType =
  | "light_onoff"
  | "light_dimmable"
  | "light_color"
  | "shutter"
  | "awning"
  // Spec 190 — a motorised door driven by OPEN / CLOSE / STOP (sibling of shutter).
  | "door"
  | "switch"
  | "sensor"
  | "button"
  | "thermostat"
  | "weather"
  | "weather_forecast"
  | "gate"
  | "heater"
  | "water_heater"
  | "energy_meter"
  | "main_energy_meter"
  | "energy_production_meter"
  | "solar_panel"
  | "media_player"
  | "appliance"
  | "water_valve"
  | "pool_pump"
  | "pool_cover"
  | "pool_heat_pump"
  // Spec 120 — Sowel-supervised display (energy display, e-paper, ...).
  | "display"
  // Spec 133 — surveillance camera (vendor-agnostic).
  | "camera"
  // Spec 153 — mechanical ventilation (VMC), 2-speed: OFF / V1 / V2.
  | "vmc"
  // Spec 156 — uninterruptible power supply, read-only.
  | "ups"
  // Spec 182 — EV charger; its contract is src/shared/ev-charger-contract.ts.
  | "ev_charger"
  // Spec 183 — electric vehicle (BEV and PHEV); contract in
  // src/shared/electric-vehicle-contract.ts.
  | "electric_vehicle";

/** Spec 208 — what an equipment acts on or measures. Only a `switch` stores
 *  one (`Equipment.family`); every other type has a fixed family, or none. */
export type EquipmentFamily = "lighting" | "openings" | "climate" | "water" | "energy" | "sensors";

/** Spec 160 — one coplanar group of panels. */
export interface SolarPlane {
  tiltDeg: number;
  azimuthDeg: number;
  peakWc: number;
}

export interface SolarProfile {
  planes: SolarPlane[];
  /**
   * ISO date. The array has been in this configuration since (spec 161).
   *
   * Only ever used to bound a backfill over existing history: fitted across a
   * capacity change the gain describes neither array. The forward path never
   * reads it.
   */
  since?: string;
}

export interface Equipment {
  id: string;
  name: string;
  zoneId: string;
  type: EquipmentType;
  icon?: string;
  description?: string;
  enabled: boolean;
  createdAt: string;
  updatedAt: string;
  /** Flexible-load declaration (spec 140). Present only when arbitration is
   *  enabled for this equipment. */
  energyProfile?: EnergyLoadProfile;
  /** Spec 146 — opt-in confirmation before actuating on the mobile dashboard
   *  (gate equipments only in v1). */
  requireConfirmation?: boolean;
  /** Spec 188 O10 — follow orders to their outcome. Absent / `null` = the
   *  type's default (on for gate and door, off otherwise). */
  trackOrderOutcome?: boolean | null;
  /** Spec 173 — id of the meter that already counts this equipment's
   *  consumption. The by-usage breakdown renders that parent net of its direct
   *  children. `null`/absent = counted nowhere else. */
  meteringParentId?: string | null;
  /** Spec 179 — this meter is fed by a separate supply: its consumption never
   *  flows through the main meter. Rendered apart from every reconciliation
   *  (by-usage partition, live donut, `other`, cost). Default false. */
  separateSupply?: boolean;
  /** Spec 195 — negative power on this meter is local production (a panel
   *  wired behind it), not consumption. Default false. */
  localProduction?: boolean;
  /** Spec 208 — a switch's family; absent / null = the plugs. */
  family?: EquipmentFamily | null;
  /** Spec 191 — a plug-in hybrid shows no cost per 100 km. */
  plugInHybrid?: boolean;
  /** Spec 193 — an EV charger's usual vehicle: breaks a tie when several
   *  cars could have charged during a session. Absent = none. */
  usualVehicleId?: string;
  /** Spec 154 — invert shutter-family command direction (shutter_move
   *  OPEN<->CLOSE, set_shutter_position -> 100-value). Command-only; ignored for
   *  non-shutter-family types. */
  invertDirection?: boolean;
  /** Spec 174 phase 2 — the timed command this equipment offers, absent when it
   *  offers none. Configuration; the window running is `timedAction`. */
  timedCommand?: TimedCommand | null;
  /** Spec 160 — declared array geometry. */
  solarProfile?: SolarProfile;
}

// ============================================================
// Energy capacity arbiter (spec 140)
// ============================================================

export type EnergyLoadClass = "comfort" | "deferrable";

export interface EnergyLoadProfile {
  class: EnergyLoadClass;
  nominalPowerW: number;
  minOnS: number;
  minOffS: number;
  /** Grid import (W) this load accepts to run on a partial surplus (#550). */
  toleratedImportW?: number;
  /** Shutdown inertia (s) — how long the load keeps drawing after a revoke
   *  before its export returns; widens the arbiter's revoke-not-honored grace
   *  for this load only (#631). Absent = global releaseHoldS. */
  releaseDelayS?: number;
  /** Core-maintained measured estimate; read-only in the UI. */
  learned?: { watts: number; atIso: string; runs: number };
}

/** Derived availability of an equipment (spec 116). Computed server-side
 *  from devices.status + streaming bindings freshness. Never persisted. */
export type EquipmentStatus = "online" | "degraded" | "offline";

export interface EquipmentStatusReason {
  offlineDevices: string[];
  staleBindings: string[];
  offlineSince: string | null;
}

export interface DataBinding {
  id: string;
  equipmentId: string;
  deviceDataId: string;
  alias: string;
  historize?: number | null;
}

export interface OrderBinding {
  id: string;
  equipmentId: string;
  deviceOrderId: string;
  alias: string;
}

export interface DataBindingWithValue extends DataBinding {
  deviceId: string;
  deviceName: string;
  key: string;
  type: DataType;
  category: DataCategory;
  value: unknown;
  unit?: string;
  enumValues?: string[];
  lastUpdated: string | null;
  lastChanged: string | null;
  historize?: number | null;
  /** True iff category is streaming AND lastUpdated > category timeout (spec 116). */
  stale: boolean;
  /**
   * How old this reading may be and still be drawn as live, derived from the
   * source's own cadence (spec 175). Power bindings only; absent means the
   * engine did not resolve it and the surface falls back to the learning
   * window, never that there is no budget.
   */
  freshnessBudgetMs?: number;
}

export interface OrderBindingWithDetails extends OrderBinding {
  deviceId: string;
  deviceName: string;
  key: string;
  type: DataType;
  category?: OrderCategory;

  min?: number;
  max?: number;
  enumValues?: string[];
  unit?: string;
}

/** A computed data value not backed by a device binding (e.g. energy cumuls). */
export interface ComputedDataEntry {
  alias: string;
  value: unknown;
  unit?: string;
  category?: DataCategory;
  lastUpdated: string | null;
}

/** Spec 174 phase 2 — what a timed control arms. `value` may equal `revertValue`
 *  (an impulse gate is opened and closed by one command) and either may be null. */
export interface TimedCommand {
  alias: string;
  value: unknown;
  revertValue: unknown;
  durationMs: number;
  /** Spec 178 — the ladder of window lengths a press walks up, shortest first.
   *  Absent = one length, extended in place. `durationMs` mirrors its first
   *  entry, so a surface showing "what the first press does" needs no change. */
  durationStepsMs?: number[];
}

/** Spec 174 — the window currently running on an equipment. */
export interface TimedAction {
  alias: string;
  value: unknown;
  revertValue: unknown;
  /** ISO-8601 instant the revert is owed at. */
  expiresAt: string;
  armedAt: string;
  armedBy?: string;
  /** Spec 178 — the rung this window is on. 0 without a ladder. */
  stepIndex: number;
  /** Spec 178 — how long the next press would make the window, or null when
   *  the next press gives the deadline up. Resolved by the engine. */
  nextDurationMs: number | null;
}

export interface EquipmentWithDetails extends Equipment {
  dataBindings: DataBindingWithValue[];
  orderBindings: OrderBindingWithDetails[];
  computedData?: ComputedDataEntry[];
  /** Derived availability (spec 116). Always present. */
  status: EquipmentStatus;
  /** Populated only when status !== "online". */
  statusReason?: EquipmentStatusReason;
  /** Spec 174 — present only while a timed window is running. */
  timedAction?: TimedAction;
}

// ============================================================
// Device (mirrors src/shared/types.ts; Sowel ui/src/types.ts, same commit)
// ============================================================

/** Open union as in Sowel (issue #937): the box's devices say "solite" or the source of the scenario. */
export type DeviceSource = string;

export type DeviceStatus = "online" | "offline" | "unknown";

/** How a device is powered, as declared by its integration (spec 143). */
export type PowerSource = "battery" | "mains" | "dc" | "unknown";

export interface Device {
  id: string;
  integrationId: string;
  sourceDeviceId: string;
  name: string;
  manufacturer?: string;
  model?: string;
  ieeeAddress?: string;
  zoneId: string | null;
  source: DeviceSource;
  status: DeviceStatus;
  powerSource?: PowerSource;
  lastSeen: string | null;
  rawExpose?: unknown;
  createdAt: string;
  updatedAt: string;
}

/** The state of the pairing window (spec 017, `GET /integrations/solite/permit-join`). */
export interface PermitJoinState {
  permitJoin: boolean;
  remaining: number;
  error?: string;
}
