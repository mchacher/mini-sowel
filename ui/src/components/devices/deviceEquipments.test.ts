import { describe, expect, it } from "vitest";
import { dataBinding, equipment, light, sensor, zone } from "../../test-utils";
import { deviceEquipmentMap } from "./deviceEquipments";

const tree = [zone("root", "Maison", [zone("garage", "Garage", [], "root")])];
const bound = (id: string, deviceId: string) =>
  dataBinding("x", "temperature", 1, { deviceId, id });

describe("deviceEquipmentMap", () => {
  it("maps the relay to its light and the TH01 and MS01 to their sensor", () => {
    const lamp = light("l", "Lumière", "garage", true);
    lamp.dataBindings = [bound("b1", "relay")];
    const probe = sensor("s", "Climat", "garage");
    probe.dataBindings = [bound("b2", "th"), bound("b3", "ms")];
    const map = deviceEquipmentMap([lamp, probe], tree);
    expect(map.get("relay")).toEqual([
      { id: "l", name: "Lumière", zoneId: "garage", zoneLabel: "Garage" },
    ]);
    expect(map.get("th")?.[0].name).toBe("Climat");
    expect(map.get("ms")?.[0].name).toBe("Climat");
    expect(map.has("other")).toBe(false);
  });

  it("lists a device in two equipments twice", () => {
    const a = light("a", "A", "garage", true);
    const b = light("b", "B", "root", true);
    a.dataBindings = [bound("b1", "relay")];
    b.dataBindings = [bound("b2", "relay")];
    expect(deviceEquipmentMap([a, b], tree).get("relay")).toHaveLength(2);
  });

  it("does not map the device of an equipment with only order bindings", () => {
    const eq = equipment("e", "Only orders", "light_onoff", "garage", {
      orderBindings: [
        {
          id: "o",
          equipmentId: "e",
          deviceOrderId: "do",
          alias: "state",
          deviceId: "relay",
          deviceName: "Relay",
          key: "state",
          type: "boolean",
        },
      ],
    });
    expect(deviceEquipmentMap([eq], tree).size).toBe(0);
  });
});
