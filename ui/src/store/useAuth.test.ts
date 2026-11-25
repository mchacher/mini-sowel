import { afterEach, describe, expect, it, vi } from "vitest";
import { authTokens, mockApi, resetStores, storeSession } from "../test-utils";
import { useAuth } from "./useAuth";

afterEach(() => {
  resetStores();
  vi.unstubAllGlobals();
});

describe("auth store", () => {
  it("setup required: the status says so, tokens left over are dropped", async () => {
    storeSession();
    mockApi({ "GET /api/v1/auth/status": { body: { setupRequired: true } } });
    await useAuth.getState().checkStatus();
    expect(useAuth.getState()).toMatchObject({ setupRequired: true, isAuthenticated: false });
    expect(localStorage.getItem("sowel_refresh_token")).toBeNull();
  });

  it("no stored token: the login screen state, no refresh asked", async () => {
    const api = mockApi({ "GET /api/v1/auth/status": { body: { setupRequired: false } } });
    await useAuth.getState().checkStatus();
    expect(useAuth.getState()).toMatchObject({
      setupRequired: false,
      isAuthenticated: false,
      loading: false,
    });
    expect(api.count("POST /api/v1/auth/refresh")).toBe(0);
  });

  it("a stored token: the session is restored by one refresh", async () => {
    storeSession();
    const api = mockApi({
      "GET /api/v1/auth/status": { body: { setupRequired: false } },
      "POST /api/v1/auth/refresh": { body: authTokens() },
    });
    await useAuth.getState().checkStatus();
    expect(useAuth.getState().isAuthenticated).toBe(true);
    expect(api.count("POST /api/v1/auth/refresh")).toBe(1);
  });

  it("a host that cannot be reached is not a login screen", async () => {
    vi.stubGlobal("fetch", () => Promise.reject(new TypeError("Failed to fetch")));
    await useAuth.getState().checkStatus();
    expect(useAuth.getState()).toMatchObject({ unreachable: true, isAuthenticated: false });
  });

  it("concurrent refreshes share one request", async () => {
    storeSession();
    const api = mockApi({ "POST /api/v1/auth/refresh": { body: authTokens() } });
    const [a, b] = await Promise.all([
      useAuth.getState().refreshSession(),
      useAuth.getState().refreshSession(),
    ]);
    expect([a, b]).toEqual([true, true]);
    expect(api.count("POST /api/v1/auth/refresh")).toBe(1);
  });

  it("a refresh refused with 401 clears the tokens", async () => {
    storeSession();
    useAuth.setState({ isAuthenticated: true });
    mockApi({
      "POST /api/v1/auth/refresh": { status: 401, body: { error: "Invalid refresh token" } },
    });
    expect(await useAuth.getState().refreshSession()).toBe(false);
    expect(useAuth.getState().isAuthenticated).toBe(false);
    expect(localStorage.getItem("sowel_refresh_token")).toBeNull();
  });

  it("a refresh that fails on a busy host keeps the tokens", async () => {
    storeSession("access-0", "refresh-0");
    useAuth.setState({ isAuthenticated: true });
    mockApi({ "POST /api/v1/auth/refresh": { status: 503, body: { error: "Busy" } } });
    expect(await useAuth.getState().refreshSession()).toBe(false);
    expect(useAuth.getState().isAuthenticated).toBe(true);
    expect(localStorage.getItem("sowel_refresh_token")).toBe("refresh-0");
  });

  it("logout sends the token and the refresh token, then clears both", async () => {
    storeSession("access-0", "refresh-0");
    useAuth.setState({ isAuthenticated: true });
    let bearer: string | null = null;
    const fetchMock = vi.fn(async (_url: string, init?: RequestInit) => {
      bearer = new Headers(init?.headers).get("Authorization");
      return new Response(null, { status: 204 });
    });
    vi.stubGlobal("fetch", fetchMock);
    await useAuth.getState().logout();
    expect(fetchMock.mock.calls[0][0]).toBe("/api/v1/auth/logout");
    expect(JSON.parse(String(fetchMock.mock.calls[0][1]?.body))).toEqual({
      refreshToken: "refresh-0",
    });
    expect(bearer).toBe("Bearer access-0");
    expect(localStorage.getItem("sowel_access_token")).toBeNull();
    expect(useAuth.getState().isAuthenticated).toBe(false);
  });

  it("logout clears the session even when the host refuses it", async () => {
    storeSession();
    useAuth.setState({ isAuthenticated: true });
    mockApi({ "POST /api/v1/auth/logout": { status: 401 } });
    await useAuth.getState().logout();
    expect(localStorage.getItem("sowel_refresh_token")).toBeNull();
    expect(useAuth.getState().isAuthenticated).toBe(false);
  });
});
