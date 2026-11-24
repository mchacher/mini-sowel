// Solite's replacement for Sowel's ui/src/components/home/CompactEquipmentCard.tsx (841 lines,
// about 35 modules: every control of Sowel). One row of an equipment, with Sowel's row classes:
// a `light_onoff` has Sowel's LightControl (compact), a `sensor` Sowel's SensorValues, any other
// type its name, its icon and its status badge. A disabled equipment is dimmed, without control.
import { useEffect, useRef, useState } from "react";
import { Box, Lightbulb } from "lucide-react";
import { ApiError } from "../../api/client";
import { apiErrorText } from "../../lib/api-error";
import type { EquipmentWithDetails } from "../../types";
import { tileClasses, familyOf } from "../../lib/equipment-family";
import { EquipmentStatusBadge } from "../equipments/EquipmentStatusBadge";
import { LightControl } from "../equipments/LightControl";
import { SensorValues } from "../equipments/SensorValues";
import { relayStateOn } from "../equipments/bindingUtils";
import { getAllBatteryBindings, getSensorBindings, getSensorIcon } from "../equipments/sensorUtils";

/** How long the text of a refused order stays under the equipment, in milliseconds. */
export const ORDER_ERROR_MS = 5000;

function icon(equipment: EquipmentWithDetails): React.ReactNode {
  if (
    equipment.type === "light_onoff" ||
    equipment.type === "light_dimmable" ||
    equipment.type === "light_color"
  ) {
    return <Lightbulb size={16} strokeWidth={1.5} />;
  }
  if (equipment.type === "sensor") return getSensorIcon(equipment.dataBindings);
  return <Box size={16} strokeWidth={1.5} />;
}

export function EquipmentRow({
  equipment,
  onExecuteOrder,
}: {
  equipment: EquipmentWithDetails;
  onExecuteOrder: (equipmentId: string, alias: string, value: unknown) => Promise<void>;
}) {
  const [error, setError] = useState<string | null>(null);
  const clear = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(
    () => () => {
      if (clear.current) clearTimeout(clear.current);
    },
    [],
  );

  // A refused order, for 5 seconds: the server's own text for a 502 or a 503, the translated text
  // of Sowel's helper when no answer came (a network error).
  const execute = async (alias: string, value: unknown): Promise<void> => {
    try {
      await onExecuteOrder(equipment.id, alias, value);
    } catch (err) {
      setError(
        err instanceof ApiError && err.status !== 0 && err.message
          ? err.message
          : apiErrorText(err),
      );
      if (clear.current) clearTimeout(clear.current);
      clear.current = setTimeout(() => setError(null), ORDER_ERROR_MS);
    }
  };

  const running = relayStateOn(equipment.dataBindings);
  const control = equipment.enabled
    ? equipment.type === "light_onoff" && (
        <LightControl equipment={equipment} onExecuteOrder={execute} compact divider={false} />
      )
    : null;
  const values =
    equipment.type === "sensor" ? (
      <SensorValues
        sensorBindings={getSensorBindings(equipment.dataBindings)}
        batteryBindings={getAllBatteryBindings(equipment.dataBindings)}
        layout="wrap"
      />
    ) : null;

  return (
    <div className={equipment.enabled ? "" : "opacity-50"}>
      <div className="flex items-center gap-[0.85rem] px-[1.1rem] py-3 min-w-0">
        <span
          aria-hidden="true"
          className={`flex-shrink-0 inline-flex items-center justify-center size-8 rounded-sm ${tileClasses(familyOf(equipment), running)}`}
        >
          {icon(equipment)}
        </span>
        <div className="flex items-center gap-2 min-w-0 flex-1">
          <span className="text-body font-medium text-text truncate">{equipment.name}</span>
          <EquipmentStatusBadge status={equipment.status} reason={equipment.statusReason} />
        </div>
        {values && <div className="flex items-center gap-3 min-w-0">{values}</div>}
        {control}
      </div>
      {error && (
        <p role="alert" className="px-[1.1rem] pb-2 text-caption text-error">
          {error}
        </p>
      )}
    </div>
  );
}
