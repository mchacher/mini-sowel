import { describe, expect, it } from "vitest";
import { motionLightRecipe } from "../test-utils";
import { recipeName, recipeSlotName } from "./recipe-i18n";

describe("recipe-i18n", () => {
  const recipe = motionLightRecipe();

  it("names the recipe in French from i18n.fr, in English otherwise", () => {
    expect(recipeName(recipe, "fr")).toBe("Lumière automatique");
    expect(recipeName(recipe, "en")).toBe("Motion Light");
  });

  it("falls back to the name when the language has none", () => {
    expect(recipeName({ ...recipe, i18n: undefined }, "fr")).toBe("Motion Light");
    const lights = recipe.slots.find((s) => s.id === "lights")!;
    const timeout = recipe.slots.find((s) => s.id === "timeout")!;
    expect(recipeSlotName(recipe, lights, "fr")).toBe("Lumières");
    expect(recipeSlotName(recipe, timeout, "fr")).toBe("Timeout");
  });
});
