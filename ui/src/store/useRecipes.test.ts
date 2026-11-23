import { afterEach, describe, expect, it, vi } from "vitest";
import { mockApi, motionLightRecipe, recipeInstance, resetStores } from "../test-utils";
import { useRecipes } from "./useRecipes";

afterEach(() => {
  resetStores();
  vi.unstubAllGlobals();
});

describe("useRecipes", () => {
  it("fetches the recipes and the instances", async () => {
    mockApi({
      "GET /api/v1/recipes": { body: [motionLightRecipe()] },
      "GET /api/v1/recipe-instances": { body: [recipeInstance("i1", { zone: "z" })] },
    });
    await useRecipes.getState().fetchRecipes();
    await useRecipes.getState().fetchInstances();
    expect(useRecipes.getState().recipes.map((r) => r.id)).toEqual(["motion-light"]);
    expect(useRecipes.getState().instances.map((i) => i.id)).toEqual(["i1"]);
  });

  it("follows the host's list after create, update, delete, enable and disable", async () => {
    const api = mockApi({
      "POST /api/v1/recipe-instances": { status: 201, body: recipeInstance("i1", {}) },
      "PUT /api/v1/recipe-instances/i1": { body: recipeInstance("i1", {}) },
      "DELETE /api/v1/recipe-instances/i1": { status: 204 },
      "POST /api/v1/recipe-instances/i1/enable": { status: 204 },
      "POST /api/v1/recipe-instances/i1/disable": { status: 204 },
      "GET /api/v1/recipe-instances": [
        { body: [recipeInstance("i1", {})] },
        { body: [recipeInstance("i1", { timeout: "5m" })] },
        { body: [] },
        { body: [recipeInstance("i1", {})] },
        { body: [recipeInstance("i1", {}, { enabled: false })] },
      ],
    });
    const s = useRecipes.getState();
    await s.createInstance("motion-light", {});
    expect(useRecipes.getState().instances).toHaveLength(1);
    await s.updateInstance("i1", { timeout: "5m" });
    expect(useRecipes.getState().instances[0].params).toEqual({ timeout: "5m" });
    await s.deleteInstance("i1");
    expect(useRecipes.getState().instances).toEqual([]);
    await s.enableInstance("i1");
    expect(useRecipes.getState().instances[0].enabled).toBe(true);
    await s.disableInstance("i1");
    expect(useRecipes.getState().instances[0].enabled).toBe(false);
    expect(api.count("GET /api/v1/recipe-instances")).toBe(5);
  });

  it("keeps the list when a request is refused, and rejects with the host's text", async () => {
    mockApi({
      "POST /api/v1/recipe-instances/i1/enable": {
        status: 400,
        body: { error: "Invalid params: At least one light is required" },
      },
    });
    useRecipes.setState({ instances: [recipeInstance("i1", {}, { enabled: false })] });
    await expect(useRecipes.getState().enableInstance("i1")).rejects.toThrow(
      "Invalid params: At least one light is required",
    );
    expect(useRecipes.getState().instances[0].enabled).toBe(false);
  });
});
