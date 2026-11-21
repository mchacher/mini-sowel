// Ported from Sowel ui/src/api/zones.ts (Sowel commit 5da84dc1, UI 1.87.0). Cuts: everything but `getZones` and `getZoneAggregation`.
import type { ZoneWithChildren, ZoneAggregatedData } from "../types";
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
