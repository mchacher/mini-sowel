// Ported from Sowel ui/src/store/useZones.ts (Sowel commit 5da84dc1, UI 1.87.0). Cuts: `reorderZones`
// (Solite has no reorder route).
import { create } from "zustand";
import type { Zone, ZoneWithChildren } from "../types";
import {
  getZones,
  createZone as apiCreateZone,
  updateZone as apiUpdateZone,
  deleteZone as apiDeleteZone,
  type ZoneInput,
} from "../api";
import { apiErrorText } from "../lib/api-error";

interface ZonesState {
  tree: ZoneWithChildren[];
  loading: boolean;
  error: string | null;

  // Actions
  fetchZones: () => Promise<void>;
  createZone: (data: ZoneInput) => Promise<Zone>;
  updateZone: (id: string, data: Partial<ZoneInput>) => Promise<void>;
  deleteZone: (id: string) => Promise<void>;

  // WebSocket handlers
  handleZoneCreated: (zone: Zone) => void;
  handleZoneUpdated: (zone: Zone) => void;
  handleZoneRemoved: (zoneId: string) => void;
}

export const useZones = create<ZonesState>((set, get) => ({
  tree: [],
  loading: false,
  error: null,

  fetchZones: async () => {
    set({ loading: true, error: null });
    try {
      const tree = await getZones();
      set({ tree, loading: false });
    } catch (err) {
      set({
        loading: false,
        error: apiErrorText(err),
      });
    }
  },

  // The page reads the tree again at once after its own request, it does not wait for the event.
  createZone: async (data) => {
    const zone = await apiCreateZone(data);
    await get().fetchZones();
    return zone;
  },

  updateZone: async (id, data) => {
    await apiUpdateZone(id, data);
    await get().fetchZones();
  },

  deleteZone: async (id) => {
    await apiDeleteZone(id);
    await get().fetchZones();
  },

  // WebSocket handlers — refetch tree on any change
  handleZoneCreated: () => {
    get().fetchZones();
  },
  handleZoneUpdated: () => {
    get().fetchZones();
  },
  handleZoneRemoved: () => {
    get().fetchZones();
  },
}));
