// Ported from Sowel ui/src/components/layout/usePluginUpdates.ts (Sowel commit 5da84dc1, UI 1.87.0)
// (spec 042, R5.4). Cut to what the box holds: the count is read from `GET /recipes/store` (the
// catalogue as last read, never the network) and counts the installed recipes the catalogue has a
// newer version of; the Plugins page gives it the list it already has instead of asking again.
import { useEffect } from "react";
import { create } from "zustand";
import { getRecipeStore, type RecipeStore } from "../../api";
import { compareVersions } from "../../lib/version";

/** The installed recipes the held catalogue has a newer version of. */
export function countRecipeUpdates(store: RecipeStore): number {
  return store.recipes.filter(
    (entry) => entry.installed !== null && compareVersions(entry.version, entry.installed) > 0,
  ).length;
}

interface RecipeUpdateState {
  count: number;
  refresh: () => void;
}

const useRecipeUpdateStore = create<RecipeUpdateState>((set) => ({
  count: 0,
  refresh: () => {
    getRecipeStore()
      .then((store) => set({ count: countRecipeUpdates(store) }))
      .catch(() => {});
  },
}));

/** The number of installed recipes with an update; read once when the sidebar opens. */
export function useRecipeUpdates(): number {
  const count = useRecipeUpdateStore((s) => s.count);
  const refresh = useRecipeUpdateStore((s) => s.refresh);
  useEffect(() => {
    refresh();
  }, [refresh]);
  return count;
}

/** The Plugins page, after it read or changed the list: the count everywhere follows. */
export function noteRecipeStore(store: RecipeStore): void {
  useRecipeUpdateStore.setState({ count: countRecipeUpdates(store) });
}

/** Back to zero (the tests' reset). */
export function resetRecipeUpdates(): void {
  useRecipeUpdateStore.setState({ count: 0 });
}
