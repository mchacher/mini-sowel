import { afterEach, describe, expect, it, vi } from "vitest";
import { mockApi } from "../test-utils";
import {
  createRecipeInstance,
  deleteRecipeInstance,
  disableRecipeInstance,
  enableRecipeInstance,
  getRecipeInstanceLog,
  getRecipeInstances,
  getRecipes,
  updateRecipeInstance,
} from "./recipes";

afterEach(() => vi.unstubAllGlobals());

describe("recipes api", () => {
  it("each call uses the route, method and body of Sowel's", async () => {
    const api = mockApi({
      "GET /api/v1/recipes": { body: [] },
      "GET /api/v1/recipe-instances": { body: [] },
      "POST /api/v1/recipe-instances": { status: 201, body: {} },
      "PUT /api/v1/recipe-instances/i1": { body: {} },
      "DELETE /api/v1/recipe-instances/i1": { status: 204 },
      "POST /api/v1/recipe-instances/i1/enable": { status: 204 },
      "POST /api/v1/recipe-instances/i1/disable": { status: 204 },
      "GET /api/v1/recipe-instances/i1/log?limit=50": { body: [] },
    });
    await getRecipes();
    await getRecipeInstances();
    await createRecipeInstance("motion-light", { zone: "z", lights: ["l"] });
    await updateRecipeInstance("i1", { timeout: "5m" });
    await deleteRecipeInstance("i1");
    await enableRecipeInstance("i1");
    await disableRecipeInstance("i1");
    await getRecipeInstanceLog("i1");
    expect(api.calls).toEqual([
      { key: "GET /api/v1/recipes", body: undefined },
      { key: "GET /api/v1/recipe-instances", body: undefined },
      {
        key: "POST /api/v1/recipe-instances",
        body: { recipeId: "motion-light", params: { zone: "z", lights: ["l"] } },
      },
      { key: "PUT /api/v1/recipe-instances/i1", body: { params: { timeout: "5m" } } },
      { key: "DELETE /api/v1/recipe-instances/i1", body: undefined },
      { key: "POST /api/v1/recipe-instances/i1/enable", body: undefined },
      { key: "POST /api/v1/recipe-instances/i1/disable", body: undefined },
      { key: "GET /api/v1/recipe-instances/i1/log?limit=50", body: undefined },
    ]);
  });
});
