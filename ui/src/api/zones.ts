// Ported from Sowel ui/src/api/zones.ts (Sowel commit 5da84dc1, UI 1.87.0). Cuts: everything but `getZones`, `getZoneAggregation`, `createZone` and `deleteZone`.
import type { Zone, ZoneWithChildren, ZoneAggregatedData } from "../types";
import { fetchJSON, API_BASE } from "./client";

// ============================================================
// Zones
// ============================================================

export async function getZones(): Promise<ZoneWithChildren[]> {
  return fetchJSON<ZoneWithChildren[]>(`${API_BASE}/zones`);
}

export async function getZoneAggregation(): Promise<Record<string, ZoneAggregatedData>> {
  return fetchJSON<Record<string, ZoneAggregatedData>>(`${API_BASE}/zones/aggregation`);
}

// Added by Solite (spec 017): the devices page makes a room under the root and undoes it when the
// device cannot be placed in it. Same routes and bodies as Sowel's `createZone` and `deleteZone`.
export async function createZone(data: { name: string }): Promise<Zone> {
  return fetchJSON<Zone>(`${API_BASE}/zones`, { method: "POST", body: JSON.stringify(data) });
}

export async function deleteZone(id: string): Promise<void> {
  return fetchJSON<void>(`${API_BASE}/zones/${id}`, { method: "DELETE" });
}
