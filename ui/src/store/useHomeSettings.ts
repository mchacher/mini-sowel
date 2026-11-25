// Solite's own file (spec 037): what the top bar needs of `GET /api/v1/settings`, the home's name
// and whether a location is set. Sowel's `AppLayout` reads the name into a local state; the
// location hint is Solite's (D62).
import { create } from "zustand";
import { getSettings } from "../api";

interface HomeSettingsState {
  /** `home.name`; null until the settings are read, `""` when unset or when the first read failed. */
  name: string | null;
  /** Whether `home.latitude` and `home.longitude` are both set; null until the settings are read. */
  hasLocation: boolean | null;
  fetch: () => Promise<void>;
}

export const useHomeSettings = create<HomeSettingsState>((set, get) => ({
  name: null,
  hasLocation: null,
  fetch: async () => {
    try {
      const all = await getSettings();
      set({
        name: all["home.name"]?.trim() ?? "",
        hasLocation: Boolean(all["home.latitude"]?.trim() && all["home.longitude"]?.trim()),
      });
    } catch {
      // A failed read keeps what was read before; with nothing read yet the name falls back to
      // the default (the empty name), the location stays unknown.
      if (get().name === null) set({ name: "" });
    }
  },
}));
