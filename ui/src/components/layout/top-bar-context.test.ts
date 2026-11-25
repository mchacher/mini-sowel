import { describe, expect, it } from "vitest";
import { zone } from "../../test-utils";
import { topBarContext } from "./top-bar-context";

const tree = [
  zone("root", "Home", [
    zone("floor", "Ground floor", [zone("garage", "Garage", [], "floor")], "root"),
  ]),
];
const t = (key: string) => key.toUpperCase();

describe("topBarContext", () => {
  it("gives the path of a room two levels below the root, the root left out", () => {
    expect(topBarContext({ page: null, zoneId: "garage", tree, t })).toEqual([
      "Ground floor",
      "Garage",
    ]);
  });

  it("gives nothing for the root zone's own page, nor for no zone", () => {
    expect(topBarContext({ page: null, zoneId: "root", tree, t })).toEqual([]);
    expect(topBarContext({ page: null, zoneId: null, tree, t })).toEqual([]);
    expect(topBarContext({ page: null, zoneId: "nowhere", tree, t })).toEqual([]);
  });

  it.each([
    ["devices", "NAV.DEVICES"],
    ["equipments", "NAV.EQUIPMENTS"],
    ["zones", "NAV.ZONES"],
    ["discovery", "NAV.DISCOVERY"],
    ["settings", "NAV.SETTINGS"],
  ] as const)("gives the label of the %s page, whatever the zone", (page, label) => {
    expect(topBarContext({ page, zoneId: "garage", tree, t })).toEqual([label]);
  });
});
