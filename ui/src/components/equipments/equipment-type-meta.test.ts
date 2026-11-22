import { describe, expect, it } from "vitest";
import { fixtures } from "../../test-utils";
import { deduceType, isCompatible } from "./equipment-type-meta";
import type { EquipmentType } from "../../types";

const TYPES: EquipmentType[] = [
  "light_onoff",
  "light_dimmable",
  "shutter",
  "sensor",
  "switch",
  "thermostat",
  "button",
];
const fits = (device: Parameters<typeof isCompatible>[1]) =>
  TYPES.filter((type) => isCompatible(type, device));

describe("isCompatible", () => {
  it("fits the ZBMINIR2 to light_onoff, switch and light_dimmable", () => {
    expect(fits(fixtures.zbminir2())).toEqual(["light_onoff", "light_dimmable", "switch"]);
  });
  it("fits the TH01 and the MS01 to sensor only", () => {
    expect(fits(fixtures.th01())).toEqual(["sensor"]);
    expect(fits(fixtures.ms01())).toEqual(["sensor"]);
  });
  it("fits a bulb to the three light types", () => {
    expect(fits(fixtures.bulb())).toEqual(["light_onoff", "light_dimmable", "switch"]);
  });
  it("fits a device with only voltage to nothing", () => {
    expect(fits(fixtures.voltageOnly())).toEqual([]);
  });
});

describe("deduceType", () => {
  it("guesses the type of each fixture, null when none fits", () => {
    expect(deduceType(fixtures.zbminir2())).toBe("light_onoff");
    expect(deduceType(fixtures.bulb())).toBe("light_dimmable");
    expect(deduceType(fixtures.th01())).toBe("sensor");
    expect(deduceType(fixtures.ms01())).toBe("sensor");
    expect(deduceType(fixtures.voltageOnly())).toBeNull();
  });
});
