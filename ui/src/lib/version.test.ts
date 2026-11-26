import { describe, expect, it } from "vitest";
import { compareVersions } from "./version";

describe("compareVersions", () => {
  it("compares the numbers, not the text: 0.10.0 is newer than 0.9.0", () => {
    expect(compareVersions("0.10.0", "0.9.0")).toBeGreaterThan(0);
    expect(compareVersions("0.9.0", "0.10.0")).toBeLessThan(0);
    expect(compareVersions("1.0.0", "0.99.99")).toBeGreaterThan(0);
  });

  it("is zero for equal versions, with or without a v", () => {
    expect(compareVersions("0.3.1", "0.3.1")).toBe(0);
    expect(compareVersions("v0.3.1", "0.3.1")).toBe(0);
  });
});
