import { act, cleanup, fireEvent, render, screen } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { setLanguage } from "../../i18n";
import { aggregate, resetStores, zone } from "../../test-utils";
import { useHomeSettings } from "../../store/useHomeSettings";
import { useZoneAggregation } from "../../store/useZoneAggregation";
import { useZones } from "../../store/useZones";
import { ROOT_ZONE_ID } from "../../zones";
import { TopBar } from "./TopBar";

afterEach(() => {
  cleanup();
  resetStores();
  setLanguage("en");
  vi.restoreAllMocks();
});

const tree = () => [
  zone(ROOT_ZONE_ID, "Home", [
    zone("floor", "Ground floor", [zone("garage", "Garage", [], "floor")], ROOT_ZONE_ID),
  ]),
];

function show(
  opts: {
    name?: string | null;
    hasLocation?: boolean | null;
    root?: Partial<Parameters<typeof aggregate>[0]> | null;
    zoneId?: string | null;
    page?: Parameters<typeof TopBar>[0]["page"];
    onMenu?: () => void;
  } = {},
) {
  useZones.setState({ tree: tree() });
  useHomeSettings.setState({
    name: opts.name === undefined ? "Annex" : opts.name,
    hasLocation: opts.hasLocation === undefined ? null : opts.hasLocation,
  });
  if (opts.root !== null) {
    useZoneAggregation.setState({ data: { [ROOT_ZONE_ID]: aggregate(opts.root ?? {}) } });
  }
  return render(
    <TopBar
      onMenu={opts.onMenu ?? (() => {})}
      zoneId={opts.zoneId === undefined ? "garage" : opts.zoneId}
      page={opts.page ?? null}
    />,
  );
}

/** The breadcrumb (from md up) and the phone's title and line under it. */
const breadcrumb = (c: Element) => c.querySelector(".md\\:flex .text-small")!.textContent;
const title = (c: Element) => c.querySelector(".text-lead")!.textContent;
const sub = (c: Element) => c.querySelector(".text-micro")!.textContent;

describe("TopBar, left side", () => {
  it("says the home's name and the room: a breadcrumb from md, a title and a line on a phone", () => {
    const { container } = show();
    expect(breadcrumb(container)).toBe("Annex›Ground floor›Garage");
    expect(title(container)).toBe("Annex");
    expect(sub(container)).toBe("Ground floor › Garage");
    expect(container.querySelector("b")!.textContent).toBe("Garage");
  });

  it("truncates the breadcrumb, so a long name and path never wrap the bar", () => {
    const { container } = show({ name: "A very long home name ".repeat(5) });
    const crumb = container.querySelector("[data-testid=topbar-breadcrumb]")!;
    expect(crumb.className.split(/\s+/)).toContain("truncate");
    expect(crumb.parentElement!.className.split(/\s+/)).toContain("min-w-0");
  });

  it("says the root zone's page as the home's name alone, and Home under it on a phone", () => {
    const { container } = show({ zoneId: ROOT_ZONE_ID });
    expect(breadcrumb(container)).toBe("Annex");
    expect(sub(container)).toBe("Home");
  });

  it("says the administration page's label", () => {
    const { container } = show({ page: "devices" });
    expect(breadcrumb(container)).toBe("Annex›Devices");
    expect(sub(container)).toBe("Devices");
  });

  it("shows a placeholder while the settings load, never the default name", () => {
    const { container } = show({ name: null });
    expect(screen.getAllByTestId("home-name-skeleton").length).toBeGreaterThan(0);
    expect(container.textContent).not.toContain("Home›");
    expect(title(container)).toBe("");
  });

  it("shows the default name when the name is empty after a read", () => {
    const { container } = show({ name: "" });
    expect(title(container)).toBe("Home");
  });

  it("shows Maison in French", () => {
    setLanguage("fr");
    const { container } = show({ name: "" });
    expect(title(container)).toBe("Maison");
  });

  it("calls onMenu from the menu button", () => {
    const onMenu = vi.fn();
    show({ onMenu });
    fireEvent.click(screen.getByRole("button", { name: "Menu" }));
    expect(onMenu).toHaveBeenCalledTimes(1);
  });
});

describe("TopBar, right side", () => {
  it("shows the sunrise and sunset of the root zone, with a sun by day", () => {
    const { container } = show({ root: { sunrise: "08:14", sunset: "17:00", isDaylight: true } });
    expect(screen.getAllByText(/^08:14\s–\s17:00$/).length).toBeGreaterThan(0);
    expect(container.querySelector("svg.text-active")).toBeTruthy();
    expect(screen.queryByRole("link", { name: "Set the location" })).toBeNull();
  });

  it("shows no sun at night: a moon", () => {
    const { container } = show({ root: { sunrise: "08:14", sunset: "17:00", isDaylight: false } });
    expect(container.querySelector("svg.text-active")).toBeNull();
    expect(screen.getAllByText(/^08:14\s–\s17:00$/).length).toBeGreaterThan(0);
  });

  it("follows the root zone's new data", () => {
    show({ root: { sunrise: "08:14", sunset: "17:00", isDaylight: true } });
    act(() =>
      useZoneAggregation
        .getState()
        .handleZoneDataChanged(
          ROOT_ZONE_ID,
          aggregate({ sunrise: "07:00", sunset: "18:00", isDaylight: true }),
        ),
    );
    expect(screen.getAllByText(/^07:00\s–\s18:00$/).length).toBeGreaterThan(0);
  });

  it("shows a link to Settings when no location is set and the root has no sunrise", () => {
    show({ hasLocation: false });
    const links = screen.getAllByRole("link", { name: "Set the location" });
    expect(links.length).toBeGreaterThan(0);
    expect(links[0].getAttribute("href")).toBe("#/settings");
  });

  it("shows nothing when a location is set and the root has no sunrise yet (clock not set)", () => {
    const { container } = show({ hasLocation: true });
    expect(screen.queryByRole("link")).toBeNull();
    expect(container.textContent).not.toMatch(/\d{2}:\d{2}/);
  });

  it("shows no link while the settings are not read", () => {
    show({ hasLocation: null });
    expect(screen.queryByRole("link")).toBeNull();
  });

  it("shows no link before the root zone's aggregate arrives", () => {
    show({ hasLocation: false, root: null });
    expect(screen.queryByRole("link")).toBeNull();
  });
});
