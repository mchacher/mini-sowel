import { cleanup, fireEvent, render, screen, within } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flattenZones } from "../../zones";
import { zone } from "../../test-utils";
import { MobileDrawer, Sidebar, type Page } from "./Sidebar";

const flat = flattenZones([
  zone("root", "Home", [zone("garage", "Garage", [], "root"), zone("house", "House", [], "root")]),
]);

beforeEach(() => {
  window.location.hash = "";
});
afterEach(cleanup);

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
    expect(entries().slice(-4)).toEqual(["Devices", "Equipments", "Topology", "Discovery"]);
    expect(title().getAttribute("aria-expanded")).toBe("true");
    fireEvent.click(title());
    expect(entries()).toEqual(["Home", "Garage", "House"]);
  });

  it("sits at the foot of the navigation, under a top border", () => {
    show();
    const section = title().parentElement!;
    expect(section.className).toContain("border-t");
    expect(section.className).toContain("border-border-light");
    expect(section.nextElementSibling).toBeNull();
  });

  it.each([
    ["devices", "Devices"],
    ["equipments", "Equipments"],
    ["zones", "Topology"],
    ["discovery", "Discovery"],
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
