// Ported from Sowel ui/src/api/auth.ts (Sowel commit 5da84dc1): status, setup, login, refresh and
// logout, without the second factor and the trusted device. Logout carries the token (Solite's
// gate, spec 029 R4); `factoryReset` is Solite's own route (D48).
import type { AuthTokens } from "../types";
import { API_BASE, fetchJSON, fetchPublic, fetchSession } from "./client";

export async function getAuthStatus(): Promise<{ setupRequired: boolean }> {
  return fetchPublic(`${API_BASE}/auth/status`);
}

export async function authSetup(data: {
  username: string;
  password: string;
  displayName: string;
  language?: "fr" | "en";
}): Promise<AuthTokens> {
  return fetchPublic(`${API_BASE}/auth/setup`, { method: "POST", body: JSON.stringify(data) });
}

export async function authLogin(username: string, password: string): Promise<AuthTokens> {
  return fetchPublic(`${API_BASE}/auth/login`, {
    method: "POST",
    body: JSON.stringify({ username, password }),
  });
}

export async function authRefresh(refreshToken: string): Promise<AuthTokens> {
  return fetchPublic(`${API_BASE}/auth/refresh`, {
    method: "POST",
    body: JSON.stringify({ refreshToken }),
  });
}

export async function authLogout(refreshToken: string): Promise<void> {
  return fetchSession(`${API_BASE}/auth/logout`, {
    method: "POST",
    body: JSON.stringify({ refreshToken }),
  });
}

/** Erases the home and the account (204). The host asks for the box's confirmation first (403). */
export async function factoryReset(): Promise<void> {
  return fetchJSON(`${API_BASE}/system/factory-reset`, { method: "POST", body: "{}" });
}
