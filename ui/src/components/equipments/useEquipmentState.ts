// Subset of Sowel ui/src/components/equipments/useEquipmentState.ts (Sowel commit 5da84dc1, UI
// 1.87.0): `computeElapsed` and `formatElapsed`, copied unchanged. The hook itself (thermostat,
// gate and shutter state) is not used by Solite's screen.
import { formatDuration } from "../../lib/format";

/** Compute elapsed seconds from an ISO timestamp. */
export function computeElapsed(iso: string | null): number {
  if (!iso) return 0;
  const ts = iso.endsWith("Z") ? iso : `${iso}Z`;
  return Math.max(0, Math.floor((Date.now() - new Date(ts).getTime()) / 1000));
}

/** Elapsed seconds by the shared duration rule: « 45 s », « 2 min 30 s », « 35 min », « 1 h 05 ». */
export function formatElapsed(s: number): string {
  return formatDuration(s);
}
