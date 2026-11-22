// Sowel's `deviceEquipmentMap` (the `useMemo` of ui/src/pages/DevicesPage.tsx, Sowel commit
// 5da84dc1): which equipments use each device, found through their data bindings (an equipment
// with only order bindings maps none of its devices), with the room's path label (spec 019, R3).
import type { EquipmentWithDetails, ZoneWithChildren } from "../../types";
import { flattenZonesWithPath } from "../../lib/zone-path";

export interface DeviceEquipment {
  id: string;
  name: string;
  zoneId: string;
  /** The room's label (`flattenZonesWithPath`), absent when the zone is unknown. */
  zoneLabel?: string;
}

export function deviceEquipmentMap(
  equipments: readonly EquipmentWithDetails[],
  tree: ZoneWithChildren[],
): Map<string, DeviceEquipment[]> {
  const labels = new Map(flattenZonesWithPath(tree).map((z) => [z.id, z.label]));
  const map = new Map<string, DeviceEquipment[]>();
  for (const eq of equipments) {
    const deviceIds = new Set<string>();
    for (const b of eq.dataBindings ?? []) {
      if (b.deviceId) deviceIds.add(b.deviceId);
    }
    const zoneLabel = eq.zoneId ? labels.get(eq.zoneId) : undefined;
    for (const deviceId of deviceIds) {
      const list = map.get(deviceId) ?? [];
      list.push({ id: eq.id, name: eq.name, zoneId: eq.zoneId, zoneLabel });
      map.set(deviceId, list);
    }
  }
  return map;
}
