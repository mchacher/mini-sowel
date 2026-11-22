// Sowel's `EquipmentCell` (ui/src/components/devices/DeviceList.tsx, Sowel commit 5da84dc1): one
// line per equipment that uses the device, "Room › Equipment"; "—" when none. The room links to
// its page; the equipment's name is plain text (Solite has no equipment page, spec 019, R3).
import { hashForZone } from "../../zones";
import type { DeviceEquipment } from "./deviceEquipments";

export function EquipmentCell({ equipments }: { equipments?: DeviceEquipment[] }) {
  if (!equipments || equipments.length === 0) {
    return <span className="text-micro text-text-tertiary">—</span>;
  }
  return (
    <span className="flex flex-col gap-0.5">
      {equipments.map((eq) => (
        <span key={eq.id} className="text-caption truncate max-w-[220px]">
          {eq.zoneLabel && (
            <>
              <a
                href={hashForZone(eq.zoneId)}
                className="text-text-secondary hover:text-primary hover:underline"
              >
                {eq.zoneLabel}
              </a>
              <span className="text-text-tertiary mx-0.5">›</span>
            </>
          )}
          <span className="text-text">{eq.name}</span>
        </span>
      ))}
    </span>
  );
}
