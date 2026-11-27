import { describe, expect, it } from "vitest";
import { adminPageFromHash, detailIdFromHash, hashForDetail } from "./admin-nav-items";

describe("admin-nav-items addresses (spec 030, R1)", () => {
  it("reads the id of an equipment's and a device's page, and the list stays marked", () => {
    expect(detailIdFromHash("#/equipments/abc-1", "equipments")).toBe("abc-1");
    expect(detailIdFromHash("#/devices/dev%2F1", "devices")).toBe("dev/1");
    expect(adminPageFromHash("#/equipments/abc-1")).toBe("equipments");
    expect(adminPageFromHash("#/devices/dev-1")).toBe("devices");
    expect(hashForDetail("devices", "dev/1")).toBe("#/devices/dev%2F1");
  });

  it("gives no id for a malformed escape or an empty id, and the page is still the list's", () => {
    for (const [hash, list] of [
      ["#/equipments/%", "equipments"],
      ["#/equipments/", "equipments"],
      ["#/devices/%", "devices"],
      ["#/devices/", "devices"],
    ] as const) {
      expect(detailIdFromHash(hash, list)).toBeNull();
      expect(adminPageFromHash(hash)).toBe(list);
    }
  });

  it("does not read an id from the other addresses, and keeps the others as before", () => {
    expect(detailIdFromHash("#/equipments", "equipments")).toBeNull();
    expect(detailIdFromHash("#/devices/x", "equipments")).toBeNull();
    expect(adminPageFromHash("#/equipments")).toBe("equipments");
    expect(adminPageFromHash("#/devices")).toBe("devices");
    expect(adminPageFromHash("#/zone/x")).toBeNull();
    expect(adminPageFromHash("#/discovery/solite")).toBe("discovery");
    expect(adminPageFromHash("#/settings")).toBe("settings");
    expect(adminPageFromHash("#/plugins")).toBe("plugins");
  });
});
