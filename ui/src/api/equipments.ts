// Ported from Sowel ui/src/api/equipments.ts (Sowel commit 5da84dc1, UI 1.87.0). Cuts: everything but `getEquipments` and `executeEquipmentOrder`.
import type { EquipmentWithDetails } from "../types";
import { fetchJSON, API_BASE } from "./client";

// ============================================================
// Equipments
// ============================================================

export async function getEquipments(): Promise<EquipmentWithDetails[]> {
  return fetchJSON<EquipmentWithDetails[]>(`${API_BASE}/equipments`);
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
