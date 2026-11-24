import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "../../api/client";
import { deviceWithData, fixtures, mockApi } from "../../test-utils";
import { applyBindingPlan, computeBindingPlan, countPlanned } from "./bindingUtils";

afterEach(() => vi.unstubAllGlobals());

const summary = (plan: ReturnType<typeof computeBindingPlan>) =>
  plan.map((b) => `${b.kind}:${b.key}>${b.alias}`);

describe("computeBindingPlan", () => {
  it("binds the ZBMINIR2's state data and order under state for a light", () => {
    const plan = computeBindingPlan([fixtures.zbminir2()], "light_onoff");
    expect(summary(plan)).toEqual(["data:state>state", "order:state>state"]);
    expect(plan[0]).toMatchObject({
      deviceId: "zb",
      sourceId: "zb-state",
      category: "light_state",
    });
    expect(plan[1].sourceId).toBe("zb-o-state");
  });

  it("binds the TH01's temperature, humidity and battery for a sensor, not its voltage", () => {
    expect(summary(computeBindingPlan([fixtures.th01()], "sensor"))).toEqual([
      "data:temperature>temperature",
      "data:humidity>humidity",
      "data:battery>battery",
    ]);
  });

  it("names the second battery battery_2 when the TH01 and the MS01 are one sensor", () => {
    expect(summary(computeBindingPlan([fixtures.th01(), fixtures.ms01()], "sensor"))).toEqual([
      "data:temperature>temperature",
      "data:humidity>humidity",
      "data:battery>battery",
      "data:occupancy>occupancy",
      "data:battery>battery_2",
    ]);
  });

  it("binds a dimmable bulb's state and brightness, data and orders", () => {
    expect(summary(computeBindingPlan([fixtures.bulb()], "light_dimmable"))).toEqual([
      "data:state>state",
      "data:brightness>brightness",
      "order:state>state",
      "order:brightness>brightness",
    ]);
  });

  it("binds a colour bulb's state, brightness and colour temperature for light_color", () => {
    expect(summary(computeBindingPlan([fixtures.colorBulb()], "light_color"))).toEqual([
      "data:state>state",
      "data:brightness>brightness",
      "order:state>state",
      "order:brightness>brightness",
      "order:color_temp>color_temp",
    ]);
  });

  it("binds a thermostat's data by category and every order", () => {
    const thermostat = deviceWithData(
      "t",
      "Thermostat",
      [
        ["targetTemperature", "setpoint"],
        ["insideTemperature", "temperature"],
        ["power", "power", "boolean"],
      ],
      {},
      [
        { key: "power_switch", category: "toggle_power" },
        { key: "eco", category: "generic" as never },
      ],
    );
    expect(summary(computeBindingPlan([thermostat], "thermostat"))).toEqual([
      "data:targetTemperature>setpoint",
      "data:insideTemperature>temperature",
      "data:power>state",
      "order:power_switch>power",
      "order:eco>eco",
    ]);
  });

  it("gives a relay with key R1 the alias state, by the per-type key map", () => {
    const relay = deviceWithData("r", "Relay", [["R1", "generic"]], {}, [
      { key: "R1", type: "enum", enumValues: ["ON", "OFF"] },
    ]);
    expect(summary(computeBindingPlan([relay], "light_onoff"))).toEqual([
      "data:R1>state",
      "order:R1>state",
    ]);
  });

  it("binds a shutter's position and its orders", () => {
    const shutter = deviceWithData("s", "Shutter", [["position", "shutter_position"]], {}, [
      { key: "position", category: "set_shutter_position", type: "number" },
      { key: "move", category: "shutter_move", type: "enum" },
    ]);
    expect(summary(computeBindingPlan([shutter], "shutter"))).toEqual([
      "data:position>position",
      "order:position>position",
      "order:move>state",
    ]);
  });

  it("gives a button's action and battery, no order", () => {
    const button = deviceWithData("b", "Button", [
      ["action", "action"],
      ["battery", "battery"],
      ["voltage", "voltage"],
    ]);
    expect(summary(computeBindingPlan([button], "button"))).toEqual([
      "data:action>action",
      "data:battery>battery",
    ]);
  });

  it("gives an empty plan for a device with only voltage, whatever the type", () => {
    for (const type of ["light_onoff", "sensor", "shutter", "button", "switch", "thermostat"]) {
      expect(computeBindingPlan([fixtures.voltageOnly()], type)).toEqual([]);
    }
  });

  it("keeps 16 data of 18 for six TH01 in one sensor, and counts the 18", () => {
    const six = Array.from({ length: 6 }, (_, i) => fixtures.th01(`th${i}`));
    const plan = computeBindingPlan(six, "sensor");
    expect(plan).toHaveLength(16);
    expect(plan[15].alias).toBe("temperature_6");
    expect(countPlanned(six, "sensor")).toEqual({ data: 18, orders: 0 });
  });

  it("leaves out an alias past 32 characters, and does not let it take the name", () => {
    const key = "k".repeat(31);
    const devices = [0, 1].map((i) => deviceWithData(`d${i}`, `D${i}`, [[key, "temperature"]]));
    const plan = computeBindingPlan(devices, "sensor");
    expect(plan.map((b) => b.alias)).toEqual([key]);
  });

  it("numbers 30-character aliases up to the limit of 32", () => {
    const key = "k".repeat(30);
    const devices = Array.from({ length: 11 }, (_, i) =>
      deviceWithData(`d${i}`, `D${i}`, [[key, "temperature"]]),
    );
    const aliases = computeBindingPlan(devices, "sensor").map((b) => b.alias);
    expect(aliases).toHaveLength(9);
    expect(aliases.slice(0, 3)).toEqual([key, `${key}_2`, `${key}_3`]);
  });

  it("follows the order the devices are given in", () => {
    const aliases = computeBindingPlan([fixtures.ms01(), fixtures.th01()], "sensor").map(
      (b) => b.alias,
    );
    expect(aliases).toEqual(["occupancy", "battery", "temperature", "humidity", "battery_2"]);
  });
});

describe("applyBindingPlan", () => {
  it("sends the data then the orders, one request after the previous answer", async () => {
    let inFlight = 0;
    let overlapped = false;
    const answer = async () => {
      inFlight++;
      if (inFlight > 1) overlapped = true;
      await new Promise((r) => setTimeout(r, 5));
      inFlight--;
      return { status: 201, body: {} };
    };
    const api = mockApi({
      "POST /api/v1/equipments/e/data-bindings": answer,
      "POST /api/v1/equipments/e/order-bindings": answer,
    });
    await applyBindingPlan("e", computeBindingPlan([fixtures.bulb()], "light_dimmable"));
    expect(api.calls).toEqual([
      {
        key: "POST /api/v1/equipments/e/data-bindings",
        body: { deviceDataId: "bulb-state", alias: "state" },
      },
      {
        key: "POST /api/v1/equipments/e/data-bindings",
        body: { deviceDataId: "bulb-brightness", alias: "brightness" },
      },
      {
        key: "POST /api/v1/equipments/e/order-bindings",
        body: { deviceOrderId: "bulb-o-state", alias: "state" },
      },
      {
        key: "POST /api/v1/equipments/e/order-bindings",
        body: { deviceOrderId: "bulb-o-brightness", alias: "brightness" },
      },
    ]);
    expect(overlapped).toBe(false);
  });

  it("stops at the first refusal and throws the API's text", async () => {
    const api = mockApi({
      "POST /api/v1/equipments/e/data-bindings": [
        { status: 201, body: {} },
        { status: 404, body: { error: "DeviceData not found: th-humidity" } },
      ],
    });
    const plan = computeBindingPlan([fixtures.th01()], "sensor");
    await expect(applyBindingPlan("e", plan)).rejects.toThrow(ApiError);
    expect(api.calls).toHaveLength(2);
  });
});
