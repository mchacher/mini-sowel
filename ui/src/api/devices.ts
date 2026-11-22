// Ported from Sowel ui/src/api/auth.ts (Sowel commit 5da84dc1, UI 1.87.0), the device functions
// `getDevices`, `updateDevice` and `deleteDevice`, unchanged. The `data` of each device is
// dropped by the store: the devices page shows no data (spec 017).
import type { Device } from "../types";
import { fetchJSON, API_BASE } from "./client";

export type DeviceWithData = Device & { data?: unknown[] };

export async function getDevices(): Promise<DeviceWithData[]> {
  return fetchJSON<DeviceWithData[]>(`${API_BASE}/devices`);
}

export async function updateDevice(
  id: string,
  updates: { name?: string; zoneId?: string | null },
): Promise<Device> {
  return fetchJSON<Device>(`${API_BASE}/devices/${id}`, {
    method: "PUT",
    body: JSON.stringify(updates),
  });
}

export async function deleteDevice(id: string): Promise<void> {
  return fetchJSON<void>(`${API_BASE}/devices/${id}`, { method: "DELETE" });
}
