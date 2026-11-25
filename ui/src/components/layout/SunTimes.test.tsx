// Ported from Sowel ui/src/components/layout/SunTimes.test.tsx (Sowel commit 2d360726), with the tests of
// `SunLocationHint` (Solite's) added.
import { cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { setLanguage } from "../../i18n";
import { MobileSunLocationHint, MobileSunTimes, SunLocationHint, SunTimes } from "./SunTimes";
import type { ZoneAggregatedData } from "../../types";

// #1133 — the sun is back in the top bar on phones and tablets, on every page.

const root = {
  sunrise: "07:22",
  sunset: "17:17",
  isDaylight: true,
} as ZoneAggregatedData;

afterEach(() => {
  cleanup();
  setLanguage("en");
});

describe("SunTimes", () => {
  beforeEach(() => {
    setLanguage("fr");
  });

  it("shows sunrise and sunset, named for screen readers", () => {
    render(<SunTimes data={root} />);
    const range = screen.getByText(/^07:22\s–\s17:17$/);
    expect(range.parentElement!.textContent).toContain("Soleil");
  });

  it("shows nothing without sun times", () => {
    const { container } = render(<SunTimes data={{ ...root, sunrise: null, sunset: null }} />);
    expect(container.textContent).toBe("");
  });
});

describe("MobileSunTimes", () => {
  beforeEach(() => {
    setLanguage("fr");
  });

  const wrapperClasses = (headerPills: number): string[] => {
    const { container } = render(<MobileSunTimes data={root} headerPills={headerPills} />);
    return (container.firstElementChild as HTMLElement).className.split(/\s+/);
  };

  it("shows on a phone beside no pill or one", () => {
    expect(wrapperClasses(0)).toEqual(["flex"]);
    expect(wrapperClasses(1)).toEqual(["flex"]);
  });

  it("gives way to two pills or more below sm, so the bar never wraps", () => {
    expect(wrapperClasses(2)).toEqual(["hidden", "sm:flex"]);
    expect(wrapperClasses(3)).toEqual(["hidden", "sm:flex"]);
  });
});

describe("SunLocationHint", () => {
  it("links to Settings, named by the words, in a 44 px tap area on a phone", () => {
    render(<SunLocationHint />);
    const link = screen.getByRole("link", { name: "Set the location" });
    expect(link.getAttribute("href")).toBe("#/settings");
    expect(link.className).toContain("min-h-11");
    expect(link.className).toContain("min-w-11");
    // The words from sm up, the sun alone below.
    expect(link.querySelector("span")!.className).toContain("hidden sm:inline");
  });

  it("follows the rule of MobileSunTimes below md", () => {
    const classes = (headerPills: number): string[] => {
      const { container } = render(<MobileSunLocationHint headerPills={headerPills} />);
      const names = (container.firstElementChild as HTMLElement).className.split(/\s+/);
      cleanup();
      return names;
    };
    expect(classes(0)).toEqual(["flex"]);
    expect(classes(1)).toEqual(["flex"]);
    expect(classes(2)).toEqual(["hidden", "sm:flex"]);
  });
});
