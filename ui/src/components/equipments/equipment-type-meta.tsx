// Ported from Sowel ui/src/components/equipments/equipment-type-meta.tsx (Sowel commit 5da84dc1, UI
// 1.87.0). Cut (specs 018, 027): everything but the eight types that have Sowel's binding rules
// (`light_onoff`, `light_dimmable`, `light_color`, `shutter`, `sensor`, `switch`, `thermostat`,
// `button`); the categories, icons and labels of those eight are Sowel's; Sowel's shutter icon
// (`ShutterClosedIcon`, a custom SVG) is Lucide's `Blinds`. The API accepts Sowel's 23 other
// types: the form does not offer them, and shows one as written.
import {
  Blinds,
  CircleDot,
  Gauge,
  Lightbulb,
  Palette,
  SunDim,
  Thermometer,
  ToggleLeft,
} from "lucide-react";
import type { DataCategory, EquipmentType } from "../../types";
import type { DeviceWithData } from "../../api/devices";
import {
  CANDIDATE_BASED_TYPES,
  computeBindingCandidates,
  isThermostatDevice,
} from "../../lib/binding-candidates";

/** The equipment types the form offers: the eight that have Sowel's binding rules. */
export const SOLITE_EQUIPMENT_TYPES: EquipmentType[] = [
  "light_onoff",
  "light_dimmable",
  "light_color",
  "shutter",
  "sensor",
  "switch",
  "thermostat",
  "button",
];

export const EQUIPMENT_TYPE_CATEGORIES: Partial<Record<EquipmentType, DataCategory[]>> = {
  light_onoff: ["light_state"],
  light_dimmable: ["light_state", "light_brightness"],
  light_color: ["light_state", "light_brightness", "light_color"],
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
  ],
  button: ["action"],
  // Sowel matches a thermostat by its contract (spec 177 in Sowel): see `isCompatible`.
  thermostat: ["setpoint"],
};

export const TYPE_ICONS: Partial<Record<EquipmentType, React.ReactNode>> = {
  light_onoff: <Lightbulb size={18} strokeWidth={1.5} />,
  light_dimmable: <SunDim size={18} strokeWidth={1.5} />,
  light_color: <Palette size={18} strokeWidth={1.5} />,
  shutter: <Blinds size={18} strokeWidth={1.5} />,
  switch: <ToggleLeft size={18} strokeWidth={1.5} />,
  sensor: <Gauge size={18} strokeWidth={1.5} />,
  button: <CircleDot size={18} strokeWidth={1.5} />,
  thermostat: <Thermometer size={18} strokeWidth={1.5} />,
};

export const TYPE_LABELS: Partial<Record<EquipmentType, string>> = Object.fromEntries(
  SOLITE_EQUIPMENT_TYPES.map((type) => [type, `equipments.type.${type}`]),
);

/**
 * Whether a device fits an equipment type, as Sowel's `DeviceSelector` decides it for the eight
 * types the form offers (spec 019): lights, switches and shutters by their binding candidates, a thermostat by its
 * contract (a `setpoint` data or a `set_setpoint` order), a sensor or a button by a data of the
 * type's categories.
 */
export function isCompatible(type: EquipmentType, device: DeviceWithData): boolean {
  const data = device.data ?? [];
  const orders = device.orders ?? [];
  if (CANDIDATE_BASED_TYPES.has(type))
    return computeBindingCandidates(type, data, orders).length > 0;
  if (type === "thermostat") return isThermostatDevice(data, orders);
  const categories = EQUIPMENT_TYPE_CATEGORIES[type];
  if (!categories || categories.length === 0) return true;
  return data.some((d) => categories.includes(d.category));
}
