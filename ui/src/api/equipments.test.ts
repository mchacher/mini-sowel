import { afterEach, describe, expect, it, vi } from "vitest";
import { equipment, light, mockApi } from "../test-utils";
import { createEquipment, deleteEquipment, updateEquipment } from "./equipments";

afterEach(() => vi.unstubAllGlobals());

describe("equipments api", () => {
  it("createEquipment posts the name, type, zone and devices, and parses the answer", async () => {
    const api = mockApi({
      "POST /api/v1/equipments": { status: 201, body: light("l", "Garage light", "g", false) },
    });
    const body = {
      name: "Garage light",
      type: "light_onoff" as const,
      zoneId: "g",
      deviceIds: ["relay"],
    };
    const created = await createEquipment(body);
    expect(created.id).toBe("l");
    expect(created.dataBindings).toHaveLength(1);
    expect(api.calls).toEqual([{ key: "POST /api/v1/equipments", body }]);
  });

  it("updateEquipment puts the name and the zone", async () => {
    const api = mockApi({
      "PUT /api/v1/equipments/l": { body: equipment("l", "Lamp", "light_onoff", "root") },
    });
    await updateEquipment("l", { name: "Lamp", zoneId: "root" });
    expect(api.calls).toEqual([
      { key: "PUT /api/v1/equipments/l", body: { name: "Lamp", zoneId: "root" } },
    ]);
  });

  it("deleteEquipment sends a DELETE", async () => {
    const api = mockApi({ "DELETE /api/v1/equipments/l": { status: 204 } });
    await deleteEquipment("l");
    expect(api.calls.map((c) => c.key)).toEqual(["DELETE /api/v1/equipments/l"]);
  });
});
