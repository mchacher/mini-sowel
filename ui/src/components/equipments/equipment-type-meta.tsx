// Ported from Sowel ui/src/components/equipments/equipment-type-meta.tsx (Sowel commit 5da84dc1, UI
// 1.87.0). Cut (spec 018): everything but decision D3's seven types (`light_onoff`,
// `light_dimmable`, `shutter`, `sensor`, `switch`, `thermostat`, `button`); the categories, icons
// and labels of those seven are Sowel's; Sowel's shutter icon (`ShutterClosedIcon`, a custom SVG)
// is Lucide's `Blinds`.
import { Blinds, CircleDot, Gauge, Lightbulb, SunDim, Thermometer, ToggleLeft } from "lucide-react";
import type { DataCategory, EquipmentType } from "../../types";

/** The equipment types Solite makes (decision D3). */
export const SOLITE_EQUIPMENT_TYPES: EquipmentType[] = [
  "light_onoff",
  "light_dimmable",
  "shutter",
  "sensor",
  "switch",
  "thermostat",
  "button",
];

export const EQUIPMENT_TYPE_CATEGORIES: Partial<Record<EquipmentType, DataCategory[]>> = {
  light_onoff: ["light_state"],
  light_dimmable: ["light_state", "light_brightness"],
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
  // Sowel matches a thermostat by its contract (spec 177 in Sowel); Solite matches the `setpoint`
  // category, which no Solite device has yet: the list is empty and "All devices" remains.
  thermostat: ["setpoint"],
};

export const TYPE_ICONS: Partial<Record<EquipmentType, React.ReactNode>> = {
  light_onoff: <Lightbulb size={18} strokeWidth={1.5} />,
  light_dimmable: <SunDim size={18} strokeWidth={1.5} />,
  shutter: <Blinds size={18} strokeWidth={1.5} />,
  switch: <ToggleLeft size={18} strokeWidth={1.5} />,
  sensor: <Gauge size={18} strokeWidth={1.5} />,
  button: <CircleDot size={18} strokeWidth={1.5} />,
  thermostat: <Thermometer size={18} strokeWidth={1.5} />,
};

export const TYPE_LABELS: Partial<Record<EquipmentType, string>> = Object.fromEntries(
  SOLITE_EQUIPMENT_TYPES.map((type) => [type, `equipments.type.${type}`]),
);
