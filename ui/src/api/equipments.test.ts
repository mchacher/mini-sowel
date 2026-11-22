import { afterEach, describe, expect, it, vi } from "vitest";
import { equipment, light, mockApi } from "../test-utils";
import {
  addDataBinding,
  addOrderBinding,
  createEquipment,
  deleteEquipment,
  updateEquipment,
} from "./equipments";

afterEach(() => vi.unstubAllGlobals());

describe("equipments api", () => {
  it("createEquipment posts the name, type and zone, no deviceIds, and parses the answer", async () => {
    const api = mockApi({
      "POST /api/v1/equipments": { status: 201, body: light("l", "Garage light", "g", false) },
    });
    const body = {
      name: "Garage light",
      type: "light_onoff" as const,
      zoneId: "g",
    };
    const created = await createEquipment(body);
    expect(created.id).toBe("l");
    expect(api.calls).toEqual([{ key: "POST /api/v1/equipments", body }]);
  });

  it("addDataBinding and addOrderBinding post the source id and the alias", async () => {
    const api = mockApi({
      "POST /api/v1/equipments/l/data-bindings": { status: 201, body: { id: "b" } },
      "POST /api/v1/equipments/l/order-bindings": { status: 201, body: { id: "o" } },
    });
    await addDataBinding("l", { deviceDataId: "dd", alias: "state" });
    await addOrderBinding("l", { deviceOrderId: "do", alias: "state" });
    expect(api.calls).toEqual([
      {
        key: "POST /api/v1/equipments/l/data-bindings",
        body: { deviceDataId: "dd", alias: "state" },
      },
      {
        key: "POST /api/v1/equipments/l/order-bindings",
        body: { deviceOrderId: "do", alias: "state" },
      },
    ]);
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
