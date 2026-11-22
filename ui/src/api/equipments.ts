// Ported from Sowel ui/src/api/equipments.ts (Sowel commit 5da84dc1, UI 1.87.0). Cuts: everything but `getEquipments`, `createEquipment`, `updateEquipment`, `deleteEquipment` and `executeEquipmentOrder`.
import type { Equipment, EquipmentType, EquipmentWithDetails } from "../types";
import { fetchJSON, API_BASE } from "./client";

// ============================================================
// Equipments
// ============================================================

export async function getEquipments(): Promise<EquipmentWithDetails[]> {
  return fetchJSON<EquipmentWithDetails[]>(`${API_BASE}/equipments`);
}

// Solite creates the bindings in the same request: `deviceIds` are bound under their key as alias
// (decision D29; Sowel's interface sends one request per binding).
export async function createEquipment(data: {
  name: string;
  type: EquipmentType;
  zoneId: string;
  deviceIds?: string[];
}): Promise<EquipmentWithDetails> {
  return fetchJSON<EquipmentWithDetails>(`${API_BASE}/equipments`, {
    method: "POST",
    body: JSON.stringify(data),
  });
}

export async function updateEquipment(
  id: string,
  data: { name?: string; zoneId?: string },
): Promise<Equipment> {
  return fetchJSON<Equipment>(`${API_BASE}/equipments/${id}`, {
    method: "PUT",
    body: JSON.stringify(data),
  });
}

export async function deleteEquipment(id: string): Promise<void> {
  return fetchJSON<void>(`${API_BASE}/equipments/${id}`, { method: "DELETE" });
}

export async function executeEquipmentOrder(
  equipmentId: string,
  alias: string,
  value: unknown,
): Promise<void> {
  return fetchJSON<void>(`${API_BASE}/equipments/${equipmentId}/orders/${alias}`, {
    method: "POST",
    body: JSON.stringify({ value }),
  });
}
