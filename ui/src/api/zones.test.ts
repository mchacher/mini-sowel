import { afterEach, describe, expect, it, vi } from "vitest";
import { mockApi, zone } from "../test-utils";
import { createZone, deleteZone, updateZone } from "./zones";

afterEach(() => vi.unstubAllGlobals());

describe("zones api", () => {
  it("createZone posts the name, the parent and the description", async () => {
    const api = mockApi({ "POST /api/v1/zones": { status: 201, body: zone("g", "Garage") } });
    const created = await createZone({ name: "Garage", parentId: "root", description: "Box" });
    expect(created.name).toBe("Garage");
    expect(api.calls).toEqual([
      {
        key: "POST /api/v1/zones",
        body: { name: "Garage", parentId: "root", description: "Box" },
      },
    ]);
  });

  it("updateZone puts the fields to the zone's route", async () => {
    const api = mockApi({ "PUT /api/v1/zones/g": { body: zone("g", "Garage 2") } });
    await updateZone("g", { name: "Garage 2", parentId: "root", description: "" });
    expect(api.calls).toEqual([
      {
        key: "PUT /api/v1/zones/g",
        body: { name: "Garage 2", parentId: "root", description: "" },
      },
    ]);
  });

  it("deleteZone sends a DELETE", async () => {
    const api = mockApi({ "DELETE /api/v1/zones/g": { status: 204 } });
    await deleteZone("g");
    expect(api.calls.map((c) => c.key)).toEqual(["DELETE /api/v1/zones/g"]);
  });
});
