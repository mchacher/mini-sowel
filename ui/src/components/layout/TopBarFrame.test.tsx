// Ported from Sowel ui/src/components/layout/TopBarFrame.test.tsx (Sowel commit 2d360726). Changed:
// `lg` is `md`; the shadow and takeover cases are cut (Solite has neither).
import { cleanup, render, screen } from "@testing-library/preact";
import { afterEach, describe, expect, it } from "vitest";
import { TopBarFrame } from "./TopBarFrame";

// PR #1136 — below md the top bar is fixed so iOS 26 does not blur it, with a
// spacer holding its place.

afterEach(cleanup);

describe("TopBarFrame", () => {
  it("fixes the bar below md and holds its place with a spacer", () => {
    render(
      <TopBarFrame>
        <header>Grange Neuve</header>
      </TopBarFrame>,
    );
    expect(screen.getByTestId("topbar-frame").className).toContain("max-md:fixed");
    expect(screen.getByTestId("topbar-spacer").style.height).toContain("var(--spacing-topbar)");
    expect(screen.getByText("Grange Neuve")).toBeTruthy();
  });
});
