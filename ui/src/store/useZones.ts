// Ported from Sowel ui/src/store/useZones.ts (Sowel commit 5da84dc1, UI 1.87.0). Cuts: the actions
// that edit zones (createZone, updateZone, deleteZone): the interface only reads them.
import { create } from "zustand";
import type { Zone, ZoneWithChildren } from "../types";
import { getZones } from "../api";
import { apiErrorText } from "../lib/api-error";

interface ZonesState {
  tree: ZoneWithChildren[];
  loading: boolean;
  error: string | null;

  // Actions
  fetchZones: () => Promise<void>;

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
