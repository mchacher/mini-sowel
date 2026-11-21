import { cleanup, render, screen } from "@testing-library/preact";
import { afterEach, describe, expect, it } from "vitest";
import { MASTER, SMALL } from "../../lib/logo";
import { WORDMARK_PATH } from "../../lib/wordmark";
import { SoliteLogo } from "./SoliteLogo";

afterEach(cleanup);

const strokeWidths = (el: HTMLElement) =>
  [...el.querySelectorAll("path")].map(
    (p) => p.getAttribute("stroke-width") ?? p.getAttribute("strokeWidth"),
  );

describe("SoliteLogo", () => {
  it("draws the master weight from 32 px", () => {
    render(<SoliteLogo size={32} />);
    const widths = strokeWidths(screen.getByRole("img", { name: "Solite" }));
    expect(widths).toContain(String(MASTER.house));
    expect(widths).not.toContain(String(SMALL.house));
  });

  it("draws the small weight below 32 px", () => {
    render(<SoliteLogo size={31} />);
    const widths = strokeWidths(screen.getByRole("img", { name: "Solite" }));
    expect(widths).toContain(String(SMALL.house));
    expect(widths).not.toContain(String(MASTER.house));
  });

  it("has the antenna: a mast, a ball and a wave on each side", () => {
    render(<SoliteLogo size={64} />);
    const logo = screen.getByRole("img", { name: "Solite" });
    expect(logo.querySelector("circle")).not.toBeNull();
    for (const d of MASTER.antenna.waves.paths) {
      expect(logo.querySelector(`path[d="${d}"]`)).not.toBeNull();
    }
    expect(logo.querySelector(`path[d="${MASTER.antenna.mast.path}"]`)).not.toBeNull();
  });

  it("draws the wordmark as a path, only when asked, under one accessible name", () => {
    const { rerender } = render(<SoliteLogo size={36} />);
    expect(document.querySelector(`path[d="${WORDMARK_PATH}"]`)).toBeNull();
    rerender(<SoliteLogo size={36} showWordmark />);
    expect(document.querySelector(`path[d="${WORDMARK_PATH}"]`)).not.toBeNull();
    expect(screen.getAllByRole("img", { name: "Solite" })).toHaveLength(1);
    expect(document.querySelector("text")).toBeNull();
  });

  it("winks only when animated", () => {
    const { rerender } = render(<SoliteLogo size={64} />);
    expect(document.querySelector("animateTransform")).toBeNull();
    rerender(<SoliteLogo size={64} animated />);
    expect(document.querySelector("animateTransform")).not.toBeNull();
  });
});
