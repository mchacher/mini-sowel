// Solite's own file (spec 026). `getSettings` and `updateSettings` take the names and shapes of
// Sowel's ui/src/api/settings.ts; `getTimezones` and the backup are Solite's own routes (D41, D42).
import { fetchJSON, API_BASE } from "./client";

/** Sowel's flat map of strings: only the keys that are set. */
export type Settings = Record<string, string>;

/** The answer of `POST /api/v1/backup`. */
export interface ImportResult {
  success: boolean;
  settings: number;
  zonesCreated: number;
  zonesUpdated: number;
  zonesUnchanged: number;
}

export async function getSettings(): Promise<Settings> {
  return fetchJSON<Settings>(`${API_BASE}/settings`);
}

export async function updateSettings(data: Settings): Promise<void> {
  await fetchJSON<unknown>(`${API_BASE}/settings`, { method: "PUT", body: JSON.stringify(data) });
}

export async function getTimezones(): Promise<string[]> {
  return fetchJSON<string[]>(`${API_BASE}/settings/timezones`);
}

/** The export: the host's document, as compact JSON text for the file. */
export async function exportBackup(): Promise<string> {
  const doc = await fetchJSON<unknown>(`${API_BASE}/backup`);
  return JSON.stringify(doc);
}

/** The import: `text` is the file's content, sent as it is. */
export async function importBackup(text: string): Promise<ImportResult> {
  return fetchJSON<ImportResult>(`${API_BASE}/backup`, { method: "POST", body: text });
}

/** Sowel's `VersionInfo` as Solite answers it: only the running version. */
export interface VersionInfo {
  current: string;
}

export async function getSystemVersion(): Promise<VersionInfo> {
  return fetchJSON<VersionInfo>(`${API_BASE}/system/version`);
}

/** The answer of `POST /api/v1/system/update`. */
export interface UpdateResult {
  message: string;
}

/** The signed file (`.sup`) as the body, streamed by the browser; the box restarts after a 200. */
export async function installUpdate(file: File): Promise<UpdateResult> {
  return fetchJSON<UpdateResult>(`${API_BASE}/system/update`, {
    method: "POST",
    headers: { "Content-Type": "application/octet-stream" },
    body: file,
  });
}
