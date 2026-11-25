import { afterEach, describe, expect, it, vi } from "vitest";
import { mockApi } from "../test-utils";
import { useHomeSettings } from "./useHomeSettings";

afterEach(() => {
  useHomeSettings.setState({ name: null, hasLocation: null });
  vi.unstubAllGlobals();
});

const read = async (settings: Record<string, string>) => {
  mockApi({ "GET /api/v1/settings": { body: settings } });
  await useHomeSettings.getState().fetch();
  return useHomeSettings.getState();
};

describe("useHomeSettings", () => {
  it("reads the name and a location set with both coordinates", async () => {
    const s = await read({
      "home.name": " Annex ",
      "home.latitude": "48.8566",
      "home.longitude": "2.3522",
    });
    expect(s.name).toBe("Annex");
    expect(s.hasLocation).toBe(true);
  });

  it("has no location when one coordinate is missing, and an empty name when none is set", async () => {
    const s = await read({ "home.latitude": "48.8566" });
    expect(s.name).toBe("");
    expect(s.hasLocation).toBe(false);
  });

  it("falls back to the empty name, the location unknown, when the first read fails", async () => {
    mockApi({ "GET /api/v1/settings": { status: 403 } });
    await useHomeSettings.getState().fetch();
    expect(useHomeSettings.getState().name).toBe("");
    expect(useHomeSettings.getState().hasLocation).toBeNull();
  });

  it("keeps the previous values when a later read fails", async () => {
    await read({ "home.name": "Annex", "home.latitude": "1", "home.longitude": "2" });
    mockApi({ "GET /api/v1/settings": { status: 500 } });
    await useHomeSettings.getState().fetch();
    expect(useHomeSettings.getState().name).toBe("Annex");
    expect(useHomeSettings.getState().hasLocation).toBe(true);
  });
});
