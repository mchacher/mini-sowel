// Ported from Sowel ui/src/store/useEquipments.ts (Sowel commit 5da84dc1, UI 1.87.0). Cuts: the actions
// that edit bindings (one request creates them, decision D29); the order timing (`useOrderTiming`, which only feeds Sowel's
// pending-order spinners).
import { create } from "zustand";
import type { EquipmentStatus, EquipmentType, EquipmentWithDetails } from "../types";
import {
  getEquipments,
  createEquipment as apiCreateEquipment,
  updateEquipment as apiUpdateEquipment,
  deleteEquipment as apiDeleteEquipment,
  executeEquipmentOrder as apiExecuteOrder,
} from "../api";
import { coalesceCalls } from "../lib/coalesce-calls";
import { apiErrorText } from "../lib/api-error";

interface EquipmentsState {
  equipments: EquipmentWithDetails[];
  loading: boolean;
  error: string | null;

  // Actions
  fetchEquipments: () => Promise<void>;
  createEquipment: (data: {
    name: string;
    type: EquipmentType;
    zoneId: string;
    deviceIds?: string[];
  }) => Promise<EquipmentWithDetails>;
  updateEquipment: (id: string, data: { name?: string; zoneId?: string }) => Promise<void>;
  deleteEquipment: (id: string) => Promise<void>;
  executeOrder: (equipmentId: string, alias: string, value: unknown) => Promise<void>;
  // WebSocket handlers
  handleEquipmentCreated: () => void;
  handleEquipmentUpdated: () => void;
  handleEquipmentRemoved: () => void;
  handleEquipmentDataChanged: (equipmentId: string, alias: string, value: unknown) => void;
  handleEquipmentStatusChanged: (equipmentId: string, newStatus: EquipmentStatus) => void;
}

/** Equipment WS events arrive in batches — one refetch per batch, not one per event. */
const refetchEquipments = coalesceCalls(() => {
  void useEquipments.getState().fetchEquipments();
}, 1000);

export const useEquipments = create<EquipmentsState>((set, get) => ({
  equipments: [],
  loading: false,
  error: null,

  fetchEquipments: async () => {
    set({ loading: true, error: null });
    try {
      const equipments = await getEquipments();
      set({ equipments, loading: false });
    } catch (err) {
      set({
        loading: false,
        error: apiErrorText(err),
      });
    }
  },

  // The page reads the list again at once after its own request, it does not wait for the event.
  createEquipment: async (data) => {
    const equipment = await apiCreateEquipment(data);
    await get().fetchEquipments();
    return equipment;
  },

  updateEquipment: async (id, data) => {
    await apiUpdateEquipment(id, data);
    await get().fetchEquipments();
  },

  deleteEquipment: async (id) => {
    await apiDeleteEquipment(id);
    await get().fetchEquipments();
  },

  executeOrder: async (equipmentId, alias, value) => {
    await apiExecuteOrder(equipmentId, alias, value);
  },

  // WebSocket handlers
  handleEquipmentCreated: () => {
    refetchEquipments();
  },
  handleEquipmentUpdated: () => {
    refetchEquipments();
  },
  handleEquipmentRemoved: () => {
    refetchEquipments();
  },
  handleEquipmentDataChanged: (equipmentId, alias, value) => {
    const now = new Date().toISOString();
    set((state) => ({
      equipments: state.equipments.map((eq) => {
        if (eq.id !== equipmentId) return eq;

        // Check if alias matches a data binding
        const hasBinding = eq.dataBindings.some((db) => db.alias === alias);

        if (hasBinding) {
          return {
            ...eq,
            dataBindings: eq.dataBindings.map((db) => {
              if (db.alias !== alias) return db;
              const valueChanged =
                JSON.stringify(db.value) !== JSON.stringify(value) || db.category === "action";
              return {
                ...db,
                value,
                lastUpdated: now,
                lastChanged: valueChanged ? now : db.lastChanged,
              };
            }),
          };
        }

        // No binding found — update computedData (e.g. energy cumuls from EnergyAggregator)
        const existing = eq.computedData ?? [];
        const idx = existing.findIndex((c) => c.alias === alias);
        const entry = {
          alias,
          value,
          lastUpdated: now,
          unit: idx >= 0 ? existing[idx].unit : undefined,
          category: idx >= 0 ? existing[idx].category : undefined,
        };
        const updated =
          idx >= 0
            ? existing.map((c, i) => (i === idx ? { ...c, value, lastUpdated: now } : c))
            : [...existing, entry];
        return { ...eq, computedData: updated };
      }),
    }));
  },
  handleEquipmentStatusChanged: (equipmentId, newStatus) => {
    // Spec 116: refetch when status changes — statusReason needs the full
    // server-side derivation (offlineDevices, staleBindings, offlineSince).
    // Coalesced: an integration coming back online flips every equipment at once.
    refetchEquipments();
    // Optimistic local update for instant badge feedback while the fetch
    // is in flight (~100ms RTT on LAN).
    set((state) => ({
      equipments: state.equipments.map((eq) =>
        eq.id === equipmentId ? { ...eq, status: newStatus } : eq,
      ),
    }));
  },
}));
