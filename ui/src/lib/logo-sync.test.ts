import favicon from "../assets/favicon.svg?raw";
import { describe, expect, it } from "vitest";
import { HOUSE, SMALL, VIEW_BOX } from "./logo";

// The favicon is a static copy of the small weight (it holds hex colours: it is an image, not a
// component): it must follow logo.ts.

describe("favicon.svg", () => {
  it("draws the small weight of logo.ts", () => {
    expect(favicon).toContain(`viewBox="${VIEW_BOX}"`);
    for (const d of [
      HOUSE,
      SMALL.leftEye,
      SMALL.rightEye,
      SMALL.smilePath,
      SMALL.antenna.mast.path,
      ...SMALL.antenna.waves.paths,
    ]) {
      expect(favicon).toContain(`d="${d}"`);
    }
    const ball = SMALL.antenna.ball;
    expect(favicon).toContain(`<circle cx="${ball.cx}" cy="${ball.cy}" r="${ball.r}"`);
    for (const w of [SMALL.house, SMALL.eye, SMALL.smile, SMALL.antenna.mast.width]) {
      expect(favicon).toContain(`stroke-width="${w}"`);
    }
  });
});
