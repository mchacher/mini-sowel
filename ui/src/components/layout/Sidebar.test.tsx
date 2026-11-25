import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flattenZones } from "../../zones";
import { mockApi, resetStores, storeSession, zone } from "../../test-utils";
import { useAuth } from "../../store/useAuth";
import { MobileDrawer, Sidebar, type Page } from "./Sidebar";

const flat = flattenZones([
  zone("root", "Home", [zone("garage", "Garage", [], "root"), zone("house", "House", [], "root")]),
]);

beforeEach(() => {
  window.location.hash = "";
});
afterEach(() => {
  cleanup();
  resetStores();
  vi.unstubAllGlobals();
});

const show = (page: Page | null = null) =>
  render(<Sidebar zones={flat} activeId="garage" page={page} />);
const entries = () =>
  within(screen.getByRole("navigation"))
    .getAllByRole("link")
    .map((l) => l.textContent);
const title = () => screen.getByRole("button", { name: "Administration" });

describe("Sidebar, Administration section", () => {
  it("is closed on a room page, with the room marked and no entry", () => {
    show();
    expect(title().getAttribute("aria-expanded")).toBe("false");
    expect(entries()).toEqual(["Home", "Garage", "House"]);
    const marked = screen.getAllByRole("link").filter((a) => a.getAttribute("aria-current"));
    expect(marked.map((a) => a.textContent)).toEqual(["Garage"]);
  });

  it("opens and closes on a click of its title, entries in order", () => {
    show();
    fireEvent.click(title());
    expect(entries().slice(-5)).toEqual([
      "Devices",
      "Equipments",
      "Topology",
      "Discovery",
      "Settings",
    ]);
    expect(title().getAttribute("aria-expanded")).toBe("true");
    fireEvent.click(title());
    expect(entries()).toEqual(["Home", "Garage", "House"]);
  });

  it("sits at the foot of the navigation, under a top border", () => {
    show();
    const section = title().parentElement!;
    expect(section.className).toContain("border-t");
    expect(section.className).toContain("border-border-light");
    // Log out follows, and nothing after it.
    expect(section.nextElementSibling?.textContent).toBe("Log out");
    expect(section.nextElementSibling?.nextElementSibling).toBeNull();
  });

  it.each([
    ["devices", "Devices"],
    ["equipments", "Equipments"],
    ["zones", "Topology"],
    ["discovery", "Discovery"],
    ["settings", "Settings"],
  ] as const)(
    "is open at %s with its entry marked, no room, and one mark in all",
    (page, label) => {
      show(page);
      expect(title().getAttribute("aria-expanded")).toBe("true");
      const marked = screen.getAllByRole("link").filter((a) => a.getAttribute("aria-current"));
      expect(marked.map((a) => a.textContent)).toEqual([label]);
    },
  );

  it("links each entry in the hash", () => {
    show("devices");
    const hrefs = Object.fromEntries(
      screen.getAllByRole("link").map((a) => [a.textContent, a.getAttribute("href")]),
    );
    expect(hrefs.Devices).toBe("#/devices");
    expect(hrefs.Equipments).toBe("#/equipments");
    expect(hrefs.Topology).toBe("#/zones");
    expect(hrefs.Discovery).toBe("#/discovery/solite");
    expect(hrefs.Settings).toBe("#/settings");
  });

  it("closes the drawer when an entry is picked, after setting the hash", () => {
    const onClose = vi.fn();
    render(<MobileDrawer zones={flat} activeId="garage" page={null} onClose={onClose} />);
    fireEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", { name: "Administration" }),
    );
    fireEvent.click(within(screen.getByRole("dialog")).getByText("Devices"));
    expect(window.location.hash).toBe("#/devices");
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});

describe("Sidebar, Log out", () => {
  it("sends the logout, clears the tokens and ends the session", async () => {
    storeSession("access-0", "refresh-0");
    useAuth.setState({ isAuthenticated: true });
    const api = mockApi({ "POST /api/v1/auth/logout": { status: 204 } });
    show();
    fireEvent.click(screen.getByRole("button", { name: "Log out" }));
    await waitFor(() => expect(useAuth.getState().isAuthenticated).toBe(false));
    expect(api.calls[0]).toEqual({
      key: "POST /api/v1/auth/logout",
      body: { refreshToken: "refresh-0" },
    });
    expect(localStorage.getItem("sowel_access_token")).toBeNull();
    expect(localStorage.getItem("sowel_refresh_token")).toBeNull();
  });
});
