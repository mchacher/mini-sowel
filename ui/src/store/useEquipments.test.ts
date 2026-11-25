import { afterEach, describe, expect, it, vi } from "vitest";
import { mockApi, resetStores } from "../test-utils";
import { useEquipments } from "./useEquipments";

afterEach(() => {
  resetStores();
  vi.unstubAllGlobals();
});

const EQ = "/api/v1/equipments";

describe("useEquipments binding actions (spec 030, R3, R4)", () => {
  it("removeDataBinding deletes the binding, then reads the list again", async () => {
    const api = mockApi({
      [`DELETE ${EQ}/e/data-bindings/b1`]: { status: 204 },
      [`GET ${EQ}`]: { body: [] },
    });
    await useEquipments.getState().removeDataBinding("e", "b1");
    expect(api.calls.map((c) => c.key)).toEqual([`DELETE ${EQ}/e/data-bindings/b1`, `GET ${EQ}`]);
  });

  it("removeOrderBinding deletes the binding, then reads the list again", async () => {
    const api = mockApi({
      [`DELETE ${EQ}/e/order-bindings/b2`]: { status: 204 },
      [`GET ${EQ}`]: { body: [] },
    });
    await useEquipments.getState().removeOrderBinding("e", "b2");
    expect(api.calls.map((c) => c.key)).toEqual([`DELETE ${EQ}/e/order-bindings/b2`, `GET ${EQ}`]);
  });

  it("addOrderBinding posts {deviceOrderId, alias}, then reads the list again", async () => {
    const api = mockApi({
      [`POST ${EQ}/e/order-bindings`]: { status: 201, body: {} },
      [`GET ${EQ}`]: { body: [] },
    });
    await useEquipments.getState().addOrderBinding("e", "do-1", "state");
    expect(api.calls).toEqual([
      { key: `POST ${EQ}/e/order-bindings`, body: { deviceOrderId: "do-1", alias: "state" } },
      { key: `GET ${EQ}`, body: undefined },
    ]);
  });

  it("addDataBinding posts {deviceDataId, alias}, and a refusal is thrown with no read", async () => {
    const api = mockApi({
      [`POST ${EQ}/e/data-bindings`]: [
        { status: 201, body: {} },
        { status: 400, body: { error: "Too many data bindings" } },
      ],
      [`GET ${EQ}`]: { body: [] },
    });
    await useEquipments.getState().addDataBinding("e", "dd-1", "state");
    await expect(useEquipments.getState().addDataBinding("e", "dd-2", "x")).rejects.toThrow(
      "Too many data bindings",
    );
    expect(api.count(`GET ${EQ}`)).toBe(1);
  });

  it("updateEquipment sends enabled", async () => {
    const api = mockApi({ [`PUT ${EQ}/e`]: { body: {} }, [`GET ${EQ}`]: { body: [] } });
    await useEquipments.getState().updateEquipment("e", { enabled: false });
    expect(api.calls[0]).toEqual({ key: `PUT ${EQ}/e`, body: { enabled: false } });
  });

  it("is loaded once the equipments were read, and not before", async () => {
    mockApi({ [`GET ${EQ}`]: { body: [] } });
    expect(useEquipments.getState().loaded).toBe(false);
    await useEquipments.getState().fetchEquipments();
    expect(useEquipments.getState().loaded).toBe(true);
  });
});
