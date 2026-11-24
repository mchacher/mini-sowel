import { describe, expect, it } from "vitest";
import { deviceWithData, fixtures } from "../test-utils";
import {
  computeBindingCandidates,
  isOnOffOrder,
  isThermostatDevice,
  type CandidateData,
  type CandidateOrder,
} from "./binding-candidates";

const of = (d: ReturnType<typeof fixtures.zbminir2>) =>
  [d.data as CandidateData[], d.orders as CandidateOrder[]] as const;

describe("computeBindingCandidates", () => {
  it.each(["light_onoff", "switch"] as const)("gives the ZBMINIR2 one candidate for %s", (type) => {
    const [data, orders] = of(fixtures.zbminir2());
    expect(computeBindingCandidates(type, data, orders)).toEqual([
      { id: "state", label: "state", dataKeys: ["state"], orderKeys: ["state"] },
    ]);
  });

  it("gives a dimmable bulb its brightness for light_dimmable", () => {
    const [data, orders] = of(fixtures.bulb());
    const [candidate] = computeBindingCandidates("light_dimmable", data, orders);
    expect(candidate.orderKeys).toEqual(["state", "brightness"]);
    expect(candidate.dataKeys).toEqual(["state", "brightness"]);
  });

  it("gives a colour bulb one candidate for light_color, with its colour temperature order", () => {
    const [data, orders] = of(fixtures.colorBulb());
    const candidates = computeBindingCandidates("light_color", data, orders);
    expect(candidates).toEqual([
      {
        id: "state",
        label: "state",
        dataKeys: ["state", "brightness"],
        orderKeys: ["state", "brightness", "color_temp"],
      },
    ]);
  });

  it("gives the ZBMINIR2 one on/off candidate for light_dimmable, as Sowel", () => {
    const [data, orders] = of(fixtures.zbminir2());
    const candidates = computeBindingCandidates("light_dimmable", data, orders);
    expect(candidates).toHaveLength(1);
    expect(candidates[0].orderKeys).toEqual(["state"]);
  });

  it("gives the TH01 no candidate for a light", () => {
    const [data, orders] = of(fixtures.th01());
    expect(computeBindingCandidates("light_onoff", data, orders)).toEqual([]);
  });

  it("takes an enum order of ON and OFF with no category as an on/off order", () => {
    expect(isOnOffOrder({ key: "power1", type: "enum", enumValues: ["ON", "OFF"] })).toBe(true);
    expect(isOnOffOrder({ key: "x", type: "enum", enumValues: ["ON", "AUTO"] })).toBe(false);
  });

  it("does not take a boolean order of another category as an on/off order", () => {
    expect(isOnOffOrder({ key: "eco", type: "boolean", category: "set_setpoint" })).toBe(false);
    expect(isOnOffOrder({ key: "eco", type: "boolean" })).toBe(false);
  });

  it("groups a shutter's orders and data by category", () => {
    const d = deviceWithData("s", "Shutter", [["position", "shutter_position"]], {}, [
      { key: "position", category: "set_shutter_position", type: "number" },
      { key: "move", category: "shutter_move", type: "enum" },
    ]);
    expect(
      computeBindingCandidates("shutter", d.data as CandidateData[], d.orders as CandidateOrder[]),
    ).toEqual([
      { id: "shutter1", label: "Shutter", dataKeys: ["position"], orderKeys: ["position", "move"] },
    ]);
  });

  it("gives nothing for a type that is not candidate-based", () => {
    const [data, orders] = of(fixtures.th01());
    expect(computeBindingCandidates("sensor", data, orders)).toEqual([]);
  });
});

describe("isThermostatDevice", () => {
  it("is true for a setpoint data, true for a set_setpoint order, false for neither", () => {
    expect(isThermostatDevice([{ category: "setpoint" }], [])).toBe(true);
    expect(isThermostatDevice([], [{ category: "set_setpoint" }])).toBe(true);
    expect(isThermostatDevice([{ category: "temperature" }], [{ category: "light_toggle" }])).toBe(
      false,
    );
  });
});
