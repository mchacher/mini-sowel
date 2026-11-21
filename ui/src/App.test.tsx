import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import { setLanguage } from "./i18n";
import {
  FakeWebSocket,
  aggregate,
  homeRoutes,
  installFakeWebSocket,
  light,
  mockApi,
  resetStores,
  sensor,
  zone,
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

async function boot(home: Parameters<typeof homeRoutes>[0]) {
  const api = mockApi(homeRoutes(home));
  render(<App />);
  act(() => FakeWebSocket.last.open());
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
    const links = screen.getAllByRole("link");
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
    expect(screen.getByText("No room yet: create one with the API")).toBeTruthy();
    expect(within(nav()).getByText("Home").closest("a")!.getAttribute("aria-current")).toBe("page");
  });

  it("says there is no room when the API has no zone at all", async () => {
    await boot({ zones: [] });
    expect(await screen.findByText("No room yet: create one with the API")).toBeTruthy();
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
    render(<App />);
    act(() => FakeWebSocket.last.open());
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
    vi.stubGlobal("fetch", () => Promise.reject(new TypeError("Failed to fetch")));
    render(<App />);
    act(() => FakeWebSocket.last.open());
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
});
