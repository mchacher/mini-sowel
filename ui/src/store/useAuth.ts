// Ported from Sowel ui/src/store/useAuth.ts (Sowel commit 5da84dc1): the session, the tokens in
// `localStorage` under Sowel's keys, a refresh on 401. Cuts: the second factor and the trusted
// device, the preferences. Added: one refresh in flight at a time (the live connection and the
// client share it), and `setupRequired`, set again when the host says the home was reset.
import { create } from "zustand";
import type { User } from "../types";
import {
  authLogin,
  authLogout,
  authRefresh,
  authSetup,
  factoryReset,
  getAuthStatus,
} from "../api/auth";
import { ApiError, setAccessToken, setOnSetupRequired, setOnUnauthorized } from "../api/client";

interface AuthState {
  user: User | null;
  isAuthenticated: boolean;
  /** `null` until the first answer of `GET /auth/status`. */
  setupRequired: boolean | null;
  loading: boolean;
  /** The status could not be read (the host is unreachable). */
  unreachable: boolean;

  checkStatus: () => Promise<void>;
  login: (username: string, password: string) => Promise<void>;
  setup: (data: {
    username: string;
    password: string;
    displayName: string;
    language?: "fr" | "en";
  }) => Promise<void>;
  logout: () => Promise<void>;
  /** Asks for new tokens; true when it worked. Concurrent calls share one request. */
  refreshSession: () => Promise<boolean>;
  /** Erases the home and the account, then shows the first-start screen. */
  factoryReset: () => Promise<void>;
}

const STORAGE_KEY_ACCESS = "sowel_access_token";
const STORAGE_KEY_REFRESH = "sowel_refresh_token";

function saveTokens(accessToken: string, refreshToken: string): void {
  localStorage.setItem(STORAGE_KEY_ACCESS, accessToken);
  localStorage.setItem(STORAGE_KEY_REFRESH, refreshToken);
  setAccessToken(accessToken);
}

function clearTokens(): void {
  localStorage.removeItem(STORAGE_KEY_ACCESS);
  localStorage.removeItem(STORAGE_KEY_REFRESH);
  setAccessToken(null);
}

function getStoredRefreshToken(): string | null {
  return localStorage.getItem(STORAGE_KEY_REFRESH);
}

let inFlight: Promise<boolean> | null = null;

export const useAuth = create<AuthState>((set, get) => {
  const stored = localStorage.getItem(STORAGE_KEY_ACCESS);
  if (stored) setAccessToken(stored);

  // A 401: one refresh; a refused one has already cleared the tokens (the login screen shows).
  setOnUnauthorized(() => get().refreshSession());
  // A 403 `setupRequired`: the home was reset from elsewhere.
  setOnSetupRequired(() => {
    clearTokens();
    set({ user: null, isAuthenticated: false, setupRequired: true, loading: false });
  });

  return {
    user: null,
    isAuthenticated: false,
    setupRequired: null,
    loading: true,
    unreachable: false,

    checkStatus: async () => {
      set({ loading: true, unreachable: false });
      try {
        const { setupRequired } = await getAuthStatus();
        if (setupRequired) {
          clearTokens();
          set({ setupRequired: true, loading: false, isAuthenticated: false });
          return;
        }
        set({ setupRequired: false });
        // Restore the session from the stored tokens.
        if (getStoredRefreshToken() && (await get().refreshSession())) return;
        set({ loading: false, isAuthenticated: false });
      } catch {
        set({ loading: false, setupRequired: null, unreachable: true });
      }
    },

    login: async (username, password) => {
      const tokens = await authLogin(username, password);
      saveTokens(tokens.accessToken, tokens.refreshToken);
      set({ user: tokens.user, isAuthenticated: true, loading: false });
    },

    setup: async (data) => {
      const tokens = await authSetup(data);
      saveTokens(tokens.accessToken, tokens.refreshToken);
      set({ user: tokens.user, isAuthenticated: true, setupRequired: false, loading: false });
    },

    logout: async () => {
      const refreshToken = getStoredRefreshToken();
      if (refreshToken) {
        try {
          await authLogout(refreshToken);
        } catch {
          // The session ends here whatever the host answers.
        }
      }
      clearTokens();
      set({ user: null, isAuthenticated: false });
    },

    refreshSession: () => {
      if (inFlight) return inFlight;
      const refreshToken = getStoredRefreshToken();
      if (!refreshToken) {
        clearTokens();
        set({ user: null, isAuthenticated: false, loading: false });
        return Promise.resolve(false);
      }
      inFlight = authRefresh(refreshToken)
        .then((tokens) => {
          saveTokens(tokens.accessToken, tokens.refreshToken);
          set({ user: tokens.user, isAuthenticated: true, loading: false });
          return true;
        })
        .catch((err: unknown) => {
          // Only the host's refusal ends the session; a busy host or a lost connection keeps the
          // tokens (the 30-day refresh token may still be good).
          if (err instanceof ApiError && err.status === 401) {
            clearTokens();
            set({ user: null, isAuthenticated: false, loading: false });
          } else {
            set({ loading: false });
          }
          return false;
        })
        .finally(() => {
          inFlight = null;
        });
      return inFlight;
    },

    factoryReset: async () => {
      await factoryReset();
      clearTokens();
      set({ user: null, isAuthenticated: false, setupRequired: true, loading: false });
    },
  };
});
