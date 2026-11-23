// Ported from Sowel ui/src/store/useRecipes.ts (Sowel commit 5da84dc1, UI 1.87.0). Cuts: `sendAction`
// and `getLog` (the journal reads its log through the API itself); the coalesced refetch on the
// `recipe.instance.*` events lives in `useLive.ts`, which names the events.
import { create } from "zustand";
import type { RecipeInfo, RecipeInstance } from "../types";
import {
  getRecipes,
  getRecipeInstances,
  createRecipeInstance,
  updateRecipeInstance,
  deleteRecipeInstance,
  enableRecipeInstance,
  disableRecipeInstance,
} from "../api";
import { apiErrorText } from "../lib/api-error";

interface RecipesState {
  recipes: RecipeInfo[];
  instances: RecipeInstance[];
  loading: boolean;
  error: string | null;
  fetchRecipes: () => Promise<void>;
  fetchInstances: () => Promise<void>;
  createInstance: (recipeId: string, params: Record<string, unknown>) => Promise<RecipeInstance>;
  updateInstance: (instanceId: string, params: Record<string, unknown>) => Promise<void>;
  deleteInstance: (instanceId: string) => Promise<void>;
  enableInstance: (instanceId: string) => Promise<void>;
  disableInstance: (instanceId: string) => Promise<void>;
}

export const useRecipes = create<RecipesState>((set, get) => ({
  recipes: [],
  instances: [],
  loading: false,
  error: null,

  fetchRecipes: async () => {
    try {
      const recipes = await getRecipes();
      set({ recipes });
    } catch {
      // Ignore: the room page works without the panel.
    }
  },

  fetchInstances: async () => {
    set({ loading: true, error: null });
    try {
      const instances = await getRecipeInstances();
      set({ instances, loading: false });
    } catch (err) {
      set({ loading: false, error: apiErrorText(err) });
    }
  },

  // The page reads the list again at once after its own request, it does not wait for the event.
  createInstance: async (recipeId, params) => {
    const instance = await createRecipeInstance(recipeId, params);
    await get().fetchInstances();
    return instance;
  },

  updateInstance: async (instanceId, params) => {
    await updateRecipeInstance(instanceId, params);
    await get().fetchInstances();
  },

  deleteInstance: async (instanceId) => {
    await deleteRecipeInstance(instanceId);
    await get().fetchInstances();
  },

  enableInstance: async (instanceId) => {
    await enableRecipeInstance(instanceId);
    await get().fetchInstances();
  },

  disableInstance: async (instanceId) => {
    await disableRecipeInstance(instanceId);
    await get().fetchInstances();
  },
}));
