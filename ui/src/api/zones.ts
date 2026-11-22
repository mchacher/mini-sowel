// Ported from Sowel ui/src/api/zones.ts (Sowel commit 5da84dc1, UI 1.87.0). Cuts: everything but `getZones`, `getZoneAggregation`, `createZone`, `updateZone` and `deleteZone`.
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

// `createZone` and `deleteZone` were added by Solite for the devices page (spec 017: a room under
// the root, undone when the device cannot be placed in it); `createZone` takes Sowel's full body
// and `updateZone` is added with the Topology page (spec 018). Same routes and bodies as Sowel's.
export interface ZoneInput {
  name: string;
  parentId?: string | null;
  description?: string;
}

export async function createZone(data: ZoneInput): Promise<Zone> {
  return fetchJSON<Zone>(`${API_BASE}/zones`, { method: "POST", body: JSON.stringify(data) });
}

export async function updateZone(id: string, data: Partial<ZoneInput>): Promise<Zone> {
  return fetchJSON<Zone>(`${API_BASE}/zones/${id}`, { method: "PUT", body: JSON.stringify(data) });
}

export async function deleteZone(id: string): Promise<void> {
  return fetchJSON<void>(`${API_BASE}/zones/${id}`, { method: "DELETE" });
}
