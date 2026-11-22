// Ported from Sowel ui/src/api/equipments.ts (Sowel commit 5da84dc1, UI 1.87.0). Cuts: everything but `getEquipments`, `createEquipment`, `addDataBinding`, `addOrderBinding`, `updateEquipment`, `deleteEquipment` and `executeEquipmentOrder`.
import type {
  DataBinding,
  Equipment,
  EquipmentType,
  EquipmentWithDetails,
  OrderBinding,
} from "../types";
import { fetchJSON, API_BASE } from "./client";

// ============================================================
// Equipments
// ============================================================

export async function getEquipments(): Promise<EquipmentWithDetails[]> {
  return fetchJSON<EquipmentWithDetails[]>(`${API_BASE}/equipments`);
}

// The bindings are created one by one from the plan computed in the browser, as Sowel's interface
// does (decision D30): `addDataBinding` and `addOrderBinding`, after `createEquipment`. The API's
// `deviceIds` (every key bound) is for its other clients.
export async function createEquipment(data: {
  name: string;
  type: EquipmentType;
  zoneId: string;
}): Promise<EquipmentWithDetails> {
  return fetchJSON<EquipmentWithDetails>(`${API_BASE}/equipments`, {
    method: "POST",
    body: JSON.stringify(data),
  });
}

export async function addDataBinding(
  equipmentId: string,
  data: { deviceDataId: string; alias: string },
): Promise<DataBinding> {
  return fetchJSON<DataBinding>(`${API_BASE}/equipments/${equipmentId}/data-bindings`, {
    method: "POST",
    body: JSON.stringify(data),
  });
}

export async function addOrderBinding(
  equipmentId: string,
  data: { deviceOrderId: string; alias: string },
): Promise<OrderBinding> {
  return fetchJSON<OrderBinding>(`${API_BASE}/equipments/${equipmentId}/order-bindings`, {
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
