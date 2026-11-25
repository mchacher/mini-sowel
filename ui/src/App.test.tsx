import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import { setLanguage } from "./i18n";
import {
  FakeWebSocket,
  openSocket,
  storeSession,
  aggregate,
  device,
  deviceWithData,
  homeRoutes,
  installFakeWebSocket,
  light,
  mockApi,
  resetStores,
  sensor,
  zone,
  type Route,
} from "./test-utils";

beforeEach(() => {
  installFakeWebSocket();
  window.location.hash = "";
});

afterEach(() => {
  cleanup();
  resetStores();
  vi.unstubAllGlobals();
});

// Root > Garage > Workshop, Root > House.
const tree = () => [
  zone("root", "Home", [
    zone("garage", "Garage", [zone("workshop", "Workshop", [], "garage")], "root"),
    zone("house", "House", [], "root"),
  ]),
];

const nav = () => screen.getByRole("navigation");

async function boot(home: Parameters<typeof homeRoutes>[0], extra: Record<string, Route> = {}) {
  const api = mockApi({ ...homeRoutes(home), ...extra });
  storeSession();
  render(<App />);
  await openSocket();
  await waitFor(() => expect(api.count("GET /api/v1/zones")).toBe(1));
  return api;
}

describe("App", () => {
  it("shows the zones as a tree, indented by depth, and the first zone with equipments", async () => {
    await boot({
      zones: tree(),
      equipments: [
        sensor("s", "Garage climate", "garage"),
        light("l", "Garage light", "garage", true),
      ],
    });
    expect(await screen.findByRole("heading", { level: 1, name: "Garage" })).toBeTruthy();
    // The zone tree's links, not the equipment names (links to their pages) of the room.
    const links = within(nav())
      .getAllByRole("link")
      .filter((l) => (l.getAttribute("href") ?? "").startsWith("#/zone/"));
    expect(links.map((l) => l.textContent)).toEqual(["Home", "Garage", "Workshop", "House"]);
    const padding = (name: string) =>
      (within(nav()).getByText(name).closest("a")!.parentElement as HTMLElement).style.paddingLeft;
    expect(padding("Garage")).toBe("20px");
    expect(padding("Workshop")).toBe("32px");
    expect(within(nav()).getByText("Garage").closest("a")!.getAttribute("aria-current")).toBe(
      "page",
    );
    expect(screen.getByText("Garage light")).toBeTruthy();
  });

  it("falls back to the first zone with equipments for an unknown zone id in the hash", async () => {
    window.location.hash = "#/zone/nowhere";
    await boot({ zones: tree(), equipments: [light("l", "House light", "house", false)] });
    expect(await screen.findByRole("heading", { level: 1, name: "House" })).toBeTruthy();
  });

  it("says that a zone has no equipment", async () => {
    window.location.hash = "#/zone/workshop";
    await boot({ zones: tree(), equipments: [light("l", "Garage light", "garage", false)] });
    expect(await screen.findByRole("heading", { level: 1, name: "Workshop" })).toBeTruthy();
    expect(screen.getByText("No equipment in this room")).toBeTruthy();
  });

  it("selects the root and says there is no room yet when there is only the root zone", async () => {
    await boot({ zones: [zone("root", "Home")] });
    expect(await screen.findByRole("heading", { level: 1, name: "Home" })).toBeTruthy();
    expect(screen.getByText("No room yet: create one in the Topology page")).toBeTruthy();
    expect(within(nav()).getByText("Home").closest("a")!.getAttribute("aria-current")).toBe("page");
  });

  it("shows the root's equipments, not 'No room yet', when only the root exists", async () => {
    await boot({
      zones: [zone("root", "Home")],
      equipments: [light("l", "Root light", "root", false)],
    });
    expect(await screen.findByText("Root light")).toBeTruthy();
    expect(screen.queryByText("No room yet: create one in the Topology page")).toBeNull();
  });

  it("says there is no room when the API has no zone at all", async () => {
    await boot({ zones: [] });
    expect(await screen.findByText("No room yet: create one in the Topology page")).toBeTruthy();
  });

  it("sets the hash when a zone is clicked and follows hashchange", async () => {
    await boot({
      zones: tree(),
      equipments: [
        light("l", "Garage light", "garage", false),
        light("h", "House light", "house", false),
      ],
    });
    await screen.findByRole("heading", { level: 1, name: "Garage" });
    fireEvent.click(within(nav()).getByText("House"));
    expect(window.location.hash).toBe("#/zone/house");
    expect(await screen.findByRole("heading", { level: 1, name: "House" })).toBeTruthy();
    window.location.hash = "#/zone/garage";
    window.dispatchEvent(new HashChangeEvent("hashchange"));
    expect(await screen.findByRole("heading", { level: 1, name: "Garage" })).toBeTruthy();
  });

  it("goes back to the first zone when the zone shown is removed", async () => {
    window.location.hash = "#/zone/house";
    const routes = homeRoutes({
      zones: tree(),
      equipments: [
        light("l", "Garage light", "garage", false),
        light("h", "House light", "house", false),
      ],
    });
    let zones = tree();
    routes["GET /api/v1/zones"] = () => ({ body: zones });
    mockApi(routes);
    storeSession();
    render(<App />);
    await openSocket();
    expect(await screen.findByRole("heading", { level: 1, name: "House" })).toBeTruthy();
    zones = [zone("root", "Home", [zone("garage", "Garage", [], "root")])];
    act(() =>
      FakeWebSocket.last.receive({ type: "zone.removed", zoneId: "house", zoneName: "House" }),
    );
    expect(await screen.findByRole("heading", { level: 1, name: "Garage" })).toBeTruthy();
  });

  it("shows the first zone with equipments for a malformed hash, without crashing", async () => {
    window.location.hash = "#/zone/%";
    await boot({ zones: tree(), equipments: [light("l", "House light", "house", false)] });
    expect(await screen.findByRole("heading", { level: 1, name: "House" })).toBeTruthy();
  });

  it("hides the sidebar below md and opens the zones in a drawer from the menu button", async () => {
    await boot({ zones: tree(), equipments: [light("l", "House light", "house", false)] });
    await screen.findByRole("heading", { level: 1, name: "House" });
    const aside = nav().closest("aside")!;
    expect(aside.className).toContain("hidden");
    expect(aside.className).toContain("md:flex");
    expect(screen.queryByRole("dialog")).toBeNull();
    const menu = screen.getByRole("button", { name: "Menu" });
    expect(menu.closest("header")!.className).toContain("md:hidden");
    fireEvent.click(menu);
    const drawer = screen.getByRole("dialog");
    expect(
      within(drawer)
        .getAllByRole("link")
        .map((l) => l.textContent),
    ).toEqual(["Home", "Garage", "Workshop", "House"]);
    fireEvent.click(within(drawer).getByRole("button", { name: "Close the menu" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("shows the mark and the wordmark in the sidebar, the top bar and the drawer", async () => {
    await boot({ zones: tree(), equipments: [light("l", "House light", "house", false)] });
    await screen.findByRole("heading", { level: 1, name: "House" });
    expect(within(nav().closest("aside")!).getByRole("img", { name: "Solite" })).toBeTruthy();
    const bar = screen.getByRole("button", { name: "Menu" }).closest("header")!;
    expect(within(bar).getByRole("img", { name: "Solite" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Menu" }));
    const drawer = screen.getByRole("dialog");
    expect(within(drawer).getByRole("img", { name: "Solite" })).toBeTruthy();
  });

  it("closes the drawer when a zone is picked in it", async () => {
    await boot({ zones: tree(), equipments: [light("l", "House light", "house", false)] });
    await screen.findByRole("heading", { level: 1, name: "House" });
    fireEvent.click(screen.getByRole("button", { name: "Menu" }));
    fireEvent.click(within(screen.getByRole("dialog")).getByText("Garage"));
    expect(window.location.hash).toBe("#/zone/garage");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(await screen.findByRole("heading", { level: 1, name: "Garage" })).toBeTruthy();
  });

  it("names the navigation in the language of the page", async () => {
    setLanguage("fr");
    try {
      await boot({ zones: tree() });
      expect(await screen.findByRole("navigation", { name: "Principale" })).toBeTruthy();
    } finally {
      setLanguage("en");
    }
  });

  it("shows the text of a store whose fetch failed", async () => {
    const routes = homeRoutes({ zones: tree() });
    routes["GET /api/v1/zones"] = () => {
      throw new TypeError("Failed to fetch");
    };
    mockApi(routes);
    storeSession();
    render(<App />);
    await openSocket();
    expect(await screen.findByText("Cannot reach Solite")).toBeTruthy();
  });

  it("updates the aggregate and the light live, without reloading", async () => {
    await boot({
      zones: tree(),
      equipments: [light("l", "Garage light", "garage", false)],
      aggregation: { garage: aggregate({ lightsTotal: 1, lightsOn: 0 }) },
    });
    expect(await screen.findByText("light off")).toBeTruthy();
    act(() =>
      FakeWebSocket.last.receive([
        {
          type: "zone.data.changed",
          zoneId: "garage",
          aggregatedData: aggregate({ lightsTotal: 1, lightsOn: 1, temperature: 12.6 }),
        },
      ]),
    );
    expect(await screen.findByText("1 light on")).toBeTruthy();
  });

  it("shows the devices page at #/devices, with the Administration section open and Devices marked", async () => {
    window.location.hash = "#/devices";
    await boot({ zones: tree(), equipments: [light("l", "House light", "house", false)] });
    expect(await screen.findByRole("heading", { level: 1, name: "Devices" })).toBeTruthy();
    const entry = within(nav()).getByRole("link", { name: "Devices" });
    expect(entry.getAttribute("aria-current")).toBe("page");
    expect(screen.queryByText("House light")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Menu" }));
    const drawer = screen.getByRole("dialog");
    expect(within(drawer).getByRole("link", { name: "Devices" })).toBeTruthy();
    fireEvent.click(within(drawer).getByText("Garage"));
    expect(window.location.hash).toBe("#/zone/garage");
  });

  it("opens the devices page from the Administration section, and the room pages are unchanged", async () => {
    await boot({ zones: tree(), equipments: [light("l", "Garage light", "garage", false)] });
    await screen.findByRole("heading", { level: 1, name: "Garage" });
    expect(within(nav()).queryByRole("link", { name: "Devices" })).toBeNull();
    fireEvent.click(within(nav()).getByRole("button", { name: "Administration" }));
    fireEvent.click(within(nav()).getByRole("link", { name: "Devices" }));
    expect(window.location.hash).toBe("#/devices");
    expect(await screen.findByRole("heading", { level: 1, name: "Devices" })).toBeTruthy();
  });

  it.each([
    ["#/zones", "Home Topology", "Topology"],
    ["#/equipments", "Equipments", "Equipments"],
    ["#/devices", "Devices", "Devices"],
    ["#/discovery/solite", "Discovery", "Discovery"],
    ["#/settings", "Settings", "Settings"],
  ])(
    "shows the page at %s with its sidebar entry marked and no room",
    async (hash, title, entry) => {
      window.location.hash = hash;
      await boot({ zones: tree(), equipments: [light("l", "House light", "house", false)] });
      expect(await screen.findByRole("heading", { level: 1, name: title })).toBeTruthy();
      expect(within(nav()).getByRole("link", { name: entry }).getAttribute("aria-current")).toBe(
        "page",
      );
      const marked = within(nav())
        .getAllByRole("link")
        .filter((a) => a.getAttribute("aria-current") === "page");
      expect(marked).toHaveLength(1);
    },
  );

  it("shows the equipment's page at #/equipments/<id>, Equipments marked in the sidebar", async () => {
    window.location.hash = "#/equipments/l";
    await boot({ zones: tree(), equipments: [light("l", "House light", "house", false)] });
    expect(await screen.findByRole("heading", { level: 1, name: "House light" })).toBeTruthy();
    expect(screen.getByRole("heading", { level: 3, name: "Configuration" })).toBeTruthy();
    expect(
      within(nav()).getByRole("link", { name: "Equipments" }).getAttribute("aria-current"),
    ).toBe("page");
    expect(screen.queryByRole("heading", { level: 1, name: "Equipments" })).toBeNull();
  });

  it("shows the device's page at #/devices/<id>, Devices marked in the sidebar", async () => {
    window.location.hash = "#/devices/dev-1";
    await boot(
      {
        zones: tree(),
        equipments: [light("l", "House light", "house", false)],
        devices: [device("dev-1", "Relay")],
      },
      { "GET /api/v1/devices/dev-1": { body: deviceWithData("dev-1", "Relay", []) } },
    );
    expect(await screen.findByRole("heading", { level: 2, name: "Data" })).toBeTruthy();
    expect(screen.getByText("Relay")).toBeTruthy();
    expect(within(nav()).getByRole("link", { name: "Devices" }).getAttribute("aria-current")).toBe(
      "page",
    );
    expect(screen.queryByRole("heading", { level: 1, name: "Devices" })).toBeNull();
  });

  it.each([
    ["#/equipments/%", "Equipments"],
    ["#/equipments/", "Equipments"],
    ["#/devices/%", "Devices"],
    ["#/devices/", "Devices"],
  ])("shows the list at the malformed address %s", async (hash, title) => {
    window.location.hash = hash;
    await boot({ zones: tree(), equipments: [light("l", "House light", "house", false)] });
    expect(await screen.findByRole("heading", { level: 1, name: title })).toBeTruthy();
  });

  it.each(["#/discovery", "#/discovery/unknown"])(
    "shows Zigbee's Discovery at %s",
    async (hash) => {
      window.location.hash = hash;
      await boot({ zones: tree() });
      expect(await screen.findByRole("heading", { level: 1, name: "Discovery" })).toBeTruthy();
      expect(screen.getByRole("button", { name: "Add a device" })).toBeTruthy();
      expect(
        within(nav()).getByRole("link", { name: "Discovery" }).getAttribute("aria-current"),
      ).toBe("page");
    },
  );

  it("lists Devices, Equipments, Topology, Discovery and Settings in this order in the section", async () => {
    await boot({ zones: tree(), equipments: [light("l", "House light", "house", false)] });
    await screen.findByRole("heading", { level: 1, name: "House" });
    fireEvent.click(within(nav()).getByRole("button", { name: "Administration" }));
    expect(
      within(nav())
        .getAllByRole("link")
        .slice(-5)
        .map((l) => l.textContent),
    ).toEqual(["Devices", "Equipments", "Topology", "Discovery", "Settings"]);
    fireEvent.click(within(nav()).getByText("Topology"));
    expect(window.location.hash).toBe("#/zones");
  });

  it("has no loose Devices, Equipments or Topology entry under the rooms", async () => {
    await boot({ zones: tree(), equipments: [light("l", "House light", "house", false)] });
    await screen.findByRole("heading", { level: 1, name: "House" });
    for (const name of ["Devices", "Equipments", "Topology", "Discovery", "Settings"]) {
      expect(within(nav()).queryByRole("link", { name })).toBeNull();
    }
  });
});

describe("App, the gate (spec 029)", () => {
  it("setupRequired: the first-start screen, no home", async () => {
    mockApi({ "GET /api/v1/auth/status": { body: { setupRequired: true } } });
    render(<App />);
    expect(await screen.findByText("Set up your Solite")).toBeTruthy();
    expect(FakeWebSocket.instances).toEqual([]);
  });

  it("no stored token: the login screen, no socket", async () => {
    mockApi(homeRoutes({ zones: tree() }));
    render(<App />);
    expect(await screen.findByRole("button", { name: "Log in" })).toBeTruthy();
    expect(FakeWebSocket.instances).toEqual([]);
  });

  it("a stored token: the home, the socket offers the new token", async () => {
    await boot({ zones: tree(), equipments: [light("l", "House light", "house", false)] });
    expect(await screen.findByRole("navigation")).toBeTruthy();
    expect(FakeWebSocket.last.protocol).toEqual(["bearer.none", "bearer.access-1"]);
  });

  it("a stored token the host refuses: the login screen", async () => {
    storeSession();
    mockApi({
      ...homeRoutes({ zones: tree() }),
      "POST /api/v1/auth/refresh": {
        status: 401,
        body: { error: "Invalid or expired refresh token" },
      },
    });
    render(<App />);
    expect(await screen.findByRole("button", { name: "Log in" })).toBeTruthy();
  });

  it("a 403 setupRequired from a route shows the first-start screen", async () => {
    const routes = homeRoutes({ zones: tree() });
    routes["GET /api/v1/zones"] = {
      status: 403,
      body: { error: "Setup required", setupRequired: true },
    };
    mockApi(routes);
    storeSession();
    render(<App />);
    await openSocket();
    expect(await screen.findByText("Set up your Solite")).toBeTruthy();
    expect(localStorage.getItem("sowel_refresh_token")).toBeNull();
  });

  it("Log out leads to the login screen and closes the socket", async () => {
    const api = await boot({ zones: tree() });
    api.calls.length = 0;
    const socket = FakeWebSocket.last;
    fireEvent.click(within(nav()).getByRole("button", { name: "Log out" }));
    expect(await screen.findByRole("button", { name: "Log in" })).toBeTruthy();
    await waitFor(() => expect(socket.readyState).toBe(3));
  });

  it("an unreachable host: a message and a retry", async () => {
    vi.stubGlobal("fetch", () => Promise.reject(new TypeError("Failed to fetch")));
    render(<App />);
    expect(await screen.findByText("Cannot reach Solite")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Try again" })).toBeTruthy();
  });
});
