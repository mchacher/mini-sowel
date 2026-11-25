import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { authTokens, mockApi, resetStores, storeSession } from "../test-utils";
import { useZones } from "../store/useZones";
import { useAuth } from "../store/useAuth";
import { ApiError, fetchJSON, fetchPublic } from "./client";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  resetStores();
});

describe("client", () => {
  it("retries a GET answered 503 once, after 1 s, and gives the data", async () => {
    const api = mockApi({
      "GET /api/v1/zones": [{ status: 503, body: { error: "Busy" } }, { body: [{ id: "z" }] }],
    });
    const pending = fetchJSON("/api/v1/zones");
    await vi.advanceTimersByTimeAsync(999);
    expect(api.count("GET /api/v1/zones")).toBe(1);
    await vi.advanceTimersByTimeAsync(1);
    await expect(pending).resolves.toEqual([{ id: "z" }]);
    expect(api.count("GET /api/v1/zones")).toBe(2);
  });

  it("does not retry twice, nor a POST", async () => {
    const api = mockApi({
      "GET /api/v1/zones": { status: 503, body: { error: "Busy" } },
      "POST /api/v1/zones": { status: 503, body: { error: "Busy" } },
    });
    const get = fetchJSON("/api/v1/zones").catch((e: Error) => e.message);
    await vi.advanceTimersByTimeAsync(1000);
    expect(await get).toBe("Busy");
    expect(api.count("GET /api/v1/zones")).toBe(2);
    await expect(fetchJSON("/api/v1/zones", { method: "POST", body: "{}" })).rejects.toThrow(
      "Busy",
    );
    expect(api.count("POST /api/v1/zones")).toBe(1);
  });

  it("gives a store the text of a failed fetch when nothing answers", async () => {
    vi.stubGlobal("fetch", () => Promise.reject(new TypeError("Failed to fetch")));
    await useZones.getState().fetchZones();
    expect(useZones.getState().error).toBe("Cannot reach Solite");
    expect(useZones.getState().loading).toBe(false);
  });

  it("sends the access token on every request, and none to a public one", async () => {
    storeSession("access-0", "refresh-0");
    const seen: (string | null)[] = [];
    const fetchMock = vi.fn(async (_url: string, init?: RequestInit) => {
      seen.push(new Headers(init?.headers).get("Authorization"));
      return new Response("{}", { status: 200 });
    });
    vi.stubGlobal("fetch", fetchMock);
    await fetchJSON("/api/v1/zones");
    await fetchJSON("/api/v1/zones", { method: "POST", body: "{}" });
    await fetchPublic("/api/v1/auth/status");
    expect(seen).toEqual(["Bearer access-0", "Bearer access-0", null]);
  });

  it("on a 401 asks for one refresh and repeats the request with the new token", async () => {
    storeSession("access-0", "refresh-0");
    const bearers: string[] = [];
    const api = mockApi({
      "GET /api/v1/zones": () => (bearers.length ? { body: [] } : { status: 401 }),
      "POST /api/v1/auth/refresh": () => {
        bearers.push("refreshed");
        return { body: authTokens("access-1", "refresh-1") };
      },
    });
    await expect(fetchJSON("/api/v1/zones")).resolves.toEqual([]);
    expect(api.count("POST /api/v1/auth/refresh")).toBe(1);
    expect(api.calls.find((c) => c.key === "POST /api/v1/auth/refresh")!.body).toEqual({
      refreshToken: "refresh-0",
    });
    expect(localStorage.getItem("sowel_access_token")).toBe("access-1");
    expect(localStorage.getItem("sowel_refresh_token")).toBe("refresh-1");
  });

  it("two 401 at once make one refresh", async () => {
    storeSession("access-0", "refresh-0");
    let token = "access-0";
    const api = mockApi({
      "GET /api/v1/zones": () => (token === "access-1" ? { body: [] } : { status: 401 }),
      "GET /api/v1/equipments": () => (token === "access-1" ? { body: [] } : { status: 401 }),
      "POST /api/v1/auth/refresh": async () => {
        await new Promise((resolve) => setTimeout(resolve, 10));
        token = "access-1";
        return { body: authTokens("access-1", "refresh-1") };
      },
    });
    const both = Promise.all([fetchJSON("/api/v1/zones"), fetchJSON("/api/v1/equipments")]);
    await vi.advanceTimersByTimeAsync(20);
    await expect(both).resolves.toEqual([[], []]);
    expect(api.count("POST /api/v1/auth/refresh")).toBe(1);
  });

  it("a refused refresh clears the tokens and the session, and the request fails", async () => {
    storeSession("access-0", "refresh-0");
    useAuth.setState({ isAuthenticated: true, loading: false, setupRequired: false });
    mockApi({
      "GET /api/v1/zones": { status: 401 },
      "POST /api/v1/auth/refresh": {
        status: 401,
        body: { error: "Invalid or expired refresh token" },
      },
    });
    await expect(fetchJSON("/api/v1/zones")).rejects.toMatchObject({ status: 401 });
    expect(localStorage.getItem("sowel_access_token")).toBeNull();
    expect(localStorage.getItem("sowel_refresh_token")).toBeNull();
    expect(useAuth.getState().isAuthenticated).toBe(false);
  });

  it("a 403 with setupRequired from any route shows the first-start screen state", async () => {
    storeSession("access-0", "refresh-0");
    useAuth.setState({ isAuthenticated: true, loading: false, setupRequired: false });
    mockApi({
      "GET /api/v1/zones": { status: 403, body: { error: "Setup required", setupRequired: true } },
    });
    await expect(fetchJSON("/api/v1/zones")).rejects.toBeInstanceOf(ApiError);
    expect(useAuth.getState().setupRequired).toBe(true);
    expect(useAuth.getState().isAuthenticated).toBe(false);
    expect(localStorage.getItem("sowel_refresh_token")).toBeNull();
  });

  it("a 403 without setupRequired (the box's confirmation) changes nothing but the error", async () => {
    storeSession("access-0", "refresh-0");
    useAuth.setState({ isAuthenticated: true, loading: false, setupRequired: false });
    mockApi({
      "POST /api/v1/system/factory-reset": {
        status: 403,
        body: { error: "Press the button on the box, then try again", confirmRequired: true },
      },
    });
    await expect(
      fetchJSON("/api/v1/system/factory-reset", { method: "POST", body: "{}" }),
    ).rejects.toThrow("Press the button on the box, then try again");
    expect(useAuth.getState().isAuthenticated).toBe(true);
  });
});
