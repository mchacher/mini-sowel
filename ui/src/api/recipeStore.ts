// Solite's own file (spec 042, D78): the catalogue of script recipes and their install, update and
// removal (`/recipes/store`, `/recipes/install`, `/recipes/:id/update`, `/recipes/:id/uninstall`),
// and `GET /system/resources`. Sowel has none of these routes.
import { API_BASE, fetchJSON } from "./client";

/** One recipe of the catalogue, with what the box holds of it. */
export interface StoreEntry {
  id: string;
  version: string;
  name: string;
  description: string;
  icon?: string;
  size: number;
  sha256: string;
  url: string;
  minVersion?: string;
  /** The version of the loaded file, `null` when none is. */
  installed: string | null;
  /** Why a file of that id was refused at start, `null` when none was. */
  refused: string | null;
  /** A native recipe or a script given by `--recipe-script` has that id. */
  builtIn: boolean;
}

export interface RecipeStore {
  recipes: StoreEntry[];
  /** When the catalogue was last read (ISO 8601), `null` before any read. */
  readAt: string | null;
  /** Why the last read failed, `null` when it did not. */
  error: string | null;
}

/** What the box holds: never the network. */
export async function getRecipeStore(): Promise<RecipeStore> {
  return fetchJSON<RecipeStore>(`${API_BASE}/recipes/store`);
}

/** Reads the catalogue again (at most once a minute: a 429 leaves the list held). */
export async function refreshRecipeStore(): Promise<RecipeStore> {
  return fetchJSON<RecipeStore>(`${API_BASE}/recipes/store/refresh`, { method: "POST" });
}

export async function installRecipe(id: string): Promise<void> {
  await fetchJSON<unknown>(`${API_BASE}/recipes/install`, {
    method: "POST",
    body: JSON.stringify({ id }),
  });
}

/** A signed file (`.srs`, at most 16 KB) as the body: an ordinary request body. */
export async function installRecipeFile(file: File): Promise<void> {
  await fetchJSON<unknown>(`${API_BASE}/recipes/install`, {
    method: "POST",
    headers: { "Content-Type": "application/octet-stream" },
    body: file,
  });
}

export async function updateRecipe(id: string): Promise<void> {
  await fetchJSON<unknown>(`${API_BASE}/recipes/${encodeURIComponent(id)}/update`, {
    method: "POST",
  });
}

export async function uninstallRecipe(id: string): Promise<void> {
  await fetchJSON<unknown>(`${API_BASE}/recipes/${encodeURIComponent(id)}/uninstall`, {
    method: "POST",
  });
}

export interface ScriptResource {
  id: string;
  version: string | null;
  source: "builtIn" | "installed";
  state: "ready" | "failed" | "refused";
  reason: string | null;
  held: number;
  limit: number;
  instances: number;
  calls: number;
  averageUs: number;
  maxUs: number;
}

/** `GET /system/resources`, bytes and microseconds as numbers; `null` where the platform reads none. */
export interface Resources {
  internal: { free: number; minFree: number; largestBlock: number } | null;
  psram: { free: number; minFree: number; total: number; reserve: number } | null;
  storage: { used: number; total: number | null };
  scripts: ScriptResource[];
}

export async function getResources(): Promise<Resources> {
  return fetchJSON<Resources>(`${API_BASE}/system/resources`);
}
