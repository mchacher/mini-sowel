// Solite's own file (spec 026). `getSettings` and `updateSettings` take the names and shapes of
// Sowel's ui/src/api/settings.ts; `getTimezones` and the backup are Solite's own routes (D41, D56); `getSystemTimezone` takes the
// shape of Sowel's ui/src/api/system.ts (spec 037).
import { fetchBlob, fetchJSON, API_BASE } from "./client";

/** Sowel's flat map of strings: only the keys that are set. */
export type Settings = Record<string, string>;

export async function getSettings(): Promise<Settings> {
  return fetchJSON<Settings>(`${API_BASE}/settings`);
}

export async function updateSettings(data: Settings): Promise<void> {
  await fetchJSON<unknown>(`${API_BASE}/settings`, { method: "PUT", body: JSON.stringify(data) });
}

/** Sowel's `SystemTimezoneInfo` (ui/src/api/system.ts, Sowel commit 2d360726): the home's time zone. */
export interface SystemTimezoneInfo {
  tz: string;
  source: "env" | "auto" | "fallback" | "unknown";
  offsetHours: number;
}

export async function getSystemTimezone(): Promise<SystemTimezoneInfo> {
  return fetchJSON<SystemTimezoneInfo>(`${API_BASE}/system/timezone`);
}

export async function getTimezones(): Promise<string[]> {
  return fetchJSON<string[]>(`${API_BASE}/settings/timezones`);
}

/** The password travels in a header, percent-encoded so that any character fits (never in the URL). */
const passwordHeader = (password: string) => ({
  "X-Backup-Password": encodeURIComponent(password),
});

/** The full backup (spec 035): the archive of `GET /api/v1/system/backup`, as a file. */
export async function exportBackup(password: string): Promise<Blob> {
  return fetchBlob(`${API_BASE}/system/backup`, { headers: passwordHeader(password) });
}

/** The answer of `POST /api/v1/system/backup`. */
export interface RestoreResult {
  success: boolean;
  message: string;
  files: number;
  wifi: boolean;
}

/** The archive (`.slb`) as the body, streamed by the browser; the box restarts after a 200. */
export async function restoreBackup(
  file: File,
  password: string,
  wifi: boolean,
): Promise<RestoreResult> {
  return fetchJSON<RestoreResult>(`${API_BASE}/system/backup${wifi ? "?wifi=true" : ""}`, {
    method: "POST",
    headers: { "Content-Type": "application/octet-stream", ...passwordHeader(password) },
    body: file,
  });
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
