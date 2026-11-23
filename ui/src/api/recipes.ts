// Ported from Sowel ui/src/api/recipes.ts (Sowel commit 5da84dc1, UI 1.87.0). Cuts: `sendRecipeInstanceAction`
// (Solite's one recipe has no action; the routes are those of spec 012).
import type { RecipeInfo, RecipeInstance, RecipeLogEntry } from "../types";
import { fetchJSON, API_BASE } from "./client";

export async function getRecipes(): Promise<RecipeInfo[]> {
  return fetchJSON<RecipeInfo[]>(`${API_BASE}/recipes`);
}

export async function getRecipeInstances(): Promise<RecipeInstance[]> {
  return fetchJSON<RecipeInstance[]>(`${API_BASE}/recipe-instances`);
}

export async function createRecipeInstance(
  recipeId: string,
  params: Record<string, unknown>,
): Promise<RecipeInstance> {
  return fetchJSON<RecipeInstance>(`${API_BASE}/recipe-instances`, {
    method: "POST",
    body: JSON.stringify({ recipeId, params }),
  });
}

export async function updateRecipeInstance(
  instanceId: string,
  params: Record<string, unknown>,
): Promise<RecipeInstance> {
  return fetchJSON<RecipeInstance>(`${API_BASE}/recipe-instances/${instanceId}`, {
    method: "PUT",
    body: JSON.stringify({ params }),
  });
}

export async function deleteRecipeInstance(instanceId: string): Promise<void> {
  return fetchJSON<void>(`${API_BASE}/recipe-instances/${instanceId}`, { method: "DELETE" });
}

export async function enableRecipeInstance(instanceId: string): Promise<void> {
  return fetchJSON<void>(`${API_BASE}/recipe-instances/${instanceId}/enable`, { method: "POST" });
}

export async function disableRecipeInstance(instanceId: string): Promise<void> {
  return fetchJSON<void>(`${API_BASE}/recipe-instances/${instanceId}/disable`, { method: "POST" });
}

export async function getRecipeInstanceLog(
  instanceId: string,
  limit = 50,
): Promise<RecipeLogEntry[]> {
  return fetchJSON<RecipeLogEntry[]>(
    `${API_BASE}/recipe-instances/${instanceId}/log?limit=${limit}`,
  );
}
