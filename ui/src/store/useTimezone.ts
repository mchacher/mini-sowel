// Ported from Sowel ui/src/store/useTimezone.ts (Sowel commit 2d360726). Unchanged but for the
// comment below, rewritten for Solite.
import { create } from "zustand";
import { getSystemTimezone } from "../api";

interface TimezoneState {
  tz: string;
  source: "env" | "auto" | "fallback" | "unknown";
  offsetHours: number;
  loaded: boolean;
  fetch: () => Promise<void>;
}

/**
 * Global timezone store.
 *
 * Filled when the home shows and at every opening of the live connection, and
 * again after Settings saves (spec 037, R3), from `GET /api/v1/system/timezone`
 * (the setting `home.timezone`, or `UTC` when unset).
 *
 * Used by the top bar's HomeTimeNote, to show the home's time when the browser
 * sits in another time zone.
 */
export const useTimezone = create<TimezoneState>((set) => ({
  tz: "UTC",
  source: "unknown",
  offsetHours: 0,
  loaded: false,
  fetch: async () => {
    try {
      const info = await getSystemTimezone();
      set({
        tz: info.tz,
        source: info.source,
        offsetHours: info.offsetHours,
        loaded: true,
      });
    } catch {
      // User might not be authenticated yet — fall back silently
    }
  },
}));
