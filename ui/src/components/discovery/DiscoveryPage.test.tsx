import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../../App";
import {
  FakeWebSocket,
  advance,
  device,
  homeRoutes,
  installFakeWebSocket,
  mockApi,
  resetStores,
  zone,
  type Route,
} from "../../test-utils";
import type { Device } from "../../types";
import { DiscoveryPage } from "./DiscoveryPage";
import { INTEGRATIONS } from "./integrations";

const PERMIT = "/api/v1/integrations/solite/permit-join";
const tree = () => [zone("root", "Home", [zone("garage", "Garage", [], "root")])];

beforeEach(() => {
  installFakeWebSocket();
  window.location.hash = "#/discovery/solite";
});

afterEach(async () => {
  cleanup();
  resetStores();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  // Preact's effects wait for a frame (a 100 ms timer): let it fire before the test environment goes.
  await new Promise((resolve) => setTimeout(resolve, 120));
});

async function boot(devices: Device[] = [], extra: Record<string, Route | Route[]> = {}) {
  const api = mockApi({ ...homeRoutes({ zones: tree(), devices }), ...extra });
  render(<App />);
  act(() => FakeWebSocket.last.open());
  await screen.findByRole("heading", { level: 1, name: "Discovery" });
  await waitFor(() => expect(api.count("GET /api/v1/devices")).toBe(1));
  return api;
}

const fake = () =>
  vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "setTimeout", "clearTimeout"] });

describe("Discovery", () => {
  it("shows the title and the Zigbee heading, with no tab bar and nothing scrollable", async () => {
    await boot([device("a", "A"), device("b", "B"), device("c", "C", { integrationId: "other" })]);
    expect(screen.getByRole("heading", { level: 2, name: "Zigbee" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: /^Zigbee/ })).toBeNull();
    expect(document.querySelector(".overflow-x-auto")).toBeNull();
  });

  it("shows the tab bar, not clipped by a scrollbar, only past one integration", () => {
    const Other = () => <p>Other screen</p>;
    const two = [
      ...INTEGRATIONS,
      { id: "other", labelKey: "discovery.integration.solite", Screen: Other },
    ];
    const { container } = render(<DiscoveryPage hash="#/discovery/other" integrations={two} />);
    expect(screen.getAllByRole("button", { name: /Zigbee/ })).toHaveLength(2);
    expect(screen.getByText("Other screen")).toBeTruthy();
    expect(container.querySelector(".overflow-x-auto")!.className).toContain("overflow-y-hidden");
  });

  it.each(["#/discovery", "#/discovery/unknown"])("shows the Zigbee screen at %s", async (hash) => {
    window.location.hash = hash;
    await boot();
    expect(screen.getByRole("button", { name: "Add a device" })).toBeTruthy();
  });

  it("Add a device posts 180, shows the countdown and Stop", async () => {
    fake();
    const api = await boot([], {
      [`POST ${PERMIT}`]: { status: 202, body: { permitJoin: true, remaining: 180 } },
      [`GET ${PERMIT}`]: [
        { body: { permitJoin: false, remaining: 0 } },
        { body: { permitJoin: true, remaining: 179 } },
      ],
    });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Add a device" }));
    });
    expect(api.calls.find((c) => c.key === `POST ${PERMIT}`)!.body).toEqual({ time: 180 });
    await advance(0);
    expect(screen.getByText(/Pairing open: put the device in pairing mode/)).toBeTruthy();
    expect(screen.getByText("180 s")).toBeTruthy();
    await advance(1000);
    expect(screen.getByText("179 s")).toBeTruthy();
  });

  it("Stop posts 0", async () => {
    fake();
    const api = await boot([], {
      [`POST ${PERMIT}`]: [
        { status: 202, body: { permitJoin: true, remaining: 180 } },
        { status: 202, body: { permitJoin: false, remaining: 0 } },
      ],
      [`GET ${PERMIT}`]: [
        { body: { permitJoin: false, remaining: 0 } },
        { body: { permitJoin: true, remaining: 179 } },
      ],
    });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Add a device" }));
    });
    await advance(0);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Stop" }));
    });
    expect(api.calls.filter((c) => c.key === `POST ${PERMIT}`).map((c) => c.body)).toEqual([
      { time: 180 },
      { time: 0 },
    ]);
    await advance(0);
    expect(screen.getByRole("button", { name: "Add a device" })).toBeTruthy();
  });

  it("shows the radio's refusal", async () => {
    fake();
    await boot([], {
      [`POST ${PERMIT}`]: { status: 202, body: { permitJoin: true, remaining: 180 } },
      [`GET ${PERMIT}`]: { body: { permitJoin: false, remaining: 0, error: "the radio refused" } },
    });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Add a device" }));
    });
    await advance(1000);
    expect(screen.getByText("the radio refused")).toBeTruthy();
  });

  it("says what to do while nothing is discovered, with a link to the devices", async () => {
    await boot([device("a", "Known")]);
    expect(
      screen.getByText("Press Add a device, then put the device in pairing mode."),
    ).toBeTruthy();
    expect(screen.getByRole("link", { name: "See all the devices" }).getAttribute("href")).toBe(
      "#/devices",
    );
    expect(screen.queryByText("Known")).toBeNull();
  });

  it("shows a discovered device first, marked New, with its Equipment and Create equipment; a status change adds no mark", async () => {
    await boot([device("a", "Known")]);
    act(() =>
      FakeWebSocket.last.receive({ type: "device.discovered", device: device("b", "Fresh") }),
    );
    act(() =>
      FakeWebSocket.last.receive({ type: "device.discovered", device: device("c", "Fresher") }),
    );
    const rows = screen.getAllByRole("row").slice(1);
    expect(rows.map((r) => within(r).getAllByRole("cell")[1].textContent)).toEqual([
      "FresherNew",
      "FreshNew",
    ]);
    expect(within(rows[0]).getAllByRole("cell")[4].textContent).toBe("—");
    expect(within(rows[0]).getByRole("button", { name: "Create equipment" })).toBeTruthy();
    act(() =>
      FakeWebSocket.last.receive({
        type: "device.status_changed",
        deviceId: "a",
        deviceName: "Known",
        status: "offline",
      }),
    );
    expect(screen.queryByText("Known")).toBeNull();
    expect(screen.getAllByText("New")).toHaveLength(2);
  });

  it("follows device.removed live", async () => {
    await boot();
    act(() =>
      FakeWebSocket.last.receive({ type: "device.discovered", device: device("b", "Fresh") }),
    );
    await screen.findByText("Fresh");
    act(() => FakeWebSocket.last.receive({ type: "device.removed", deviceId: "b" }));
    expect(screen.queryByText("Fresh")).toBeNull();
  });

  it("empties the list when the screen is left and opened again, and reads the countdown again", async () => {
    const api = await boot([], {
      [`GET ${PERMIT}`]: { body: { permitJoin: true, remaining: 100 } },
    });
    act(() =>
      FakeWebSocket.last.receive({ type: "device.discovered", device: device("b", "Fresh") }),
    );
    await screen.findByText("Fresh");
    act(() => {
      window.location.hash = "#/devices";
      window.dispatchEvent(new HashChangeEvent("hashchange"));
    });
    await screen.findByRole("heading", { level: 1, name: "Devices" });
    act(() => {
      window.location.hash = "#/discovery/solite";
      window.dispatchEvent(new HashChangeEvent("hashchange"));
    });
    await screen.findByRole("heading", { level: 1, name: "Discovery" });
    expect(screen.queryByText("Fresh")).toBeNull();
    expect(screen.queryByText("New")).toBeNull();
    expect(api.count(`GET ${PERMIT}`)).toBeGreaterThanOrEqual(1);
  });

  it("goes on counting while the connection is lost, and reads the state and devices on reconnection", async () => {
    fake();
    vi.spyOn(Math, "random").mockReturnValue(0);
    const api = await boot([], {
      [`POST ${PERMIT}`]: { status: 202, body: { permitJoin: true, remaining: 180 } },
      [`GET ${PERMIT}`]: [
        { body: { permitJoin: false, remaining: 0 } },
        { body: { permitJoin: true, remaining: 170 } },
        { body: { permitJoin: true, remaining: 170 } },
      ],
    });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Add a device" }));
    });
    act(() => FakeWebSocket.last.drop());
    await advance(500);
    expect(screen.getByText("180 s")).toBeTruthy();
    expect(api.count("GET /api/v1/devices")).toBe(1);
    await advance(1500);
    const before = api.count("GET /api/v1/devices");
    act(() => FakeWebSocket.last.open());
    await advance(0);
    expect(api.count("GET /api/v1/devices")).toBe(before + 1);
    expect(screen.getByText("170 s")).toBeTruthy();
  });
});
