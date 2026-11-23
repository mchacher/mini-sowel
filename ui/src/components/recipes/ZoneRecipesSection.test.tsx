import { cleanup, fireEvent, render, screen } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { light, motionLightRecipe, recipeInstance, resetStores } from "../../test-utils";
import { useRecipes } from "../../store/useRecipes";
import { ZoneRecipesSection } from "./ZoneRecipesSection";

afterEach(() => {
  cleanup();
  resetStores();
  vi.unstubAllGlobals();
});

function renderSection() {
  render(
    <ZoneRecipesSection
      zoneId="garage"
      zoneName="Garage"
      equipments={[light("l1", "Garage light", "garage", false)]}
      aggregate={undefined}
    />,
  );
}

describe("ZoneRecipesSection", () => {
  it("shows the instances of the room only, with their count", () => {
    useRecipes.setState({
      recipes: [motionLightRecipe()],
      instances: [
        recipeInstance("a", { zone: "garage", lights: ["l1"] }),
        recipeInstance("b", { zone: "kitchen", lights: ["l9"] }),
        recipeInstance("c", { zone: "garage", lights: [] }),
      ],
    });
    renderSection();
    expect(screen.getAllByTestId("recipe-row")).toHaveLength(2);
    expect(screen.getByRole("heading", { name: "Recipes" })).toBeTruthy();
    expect(screen.getByText("2")).toBeTruthy(); // the count beside the title
  });

  it("an empty room says so and offers to add a recipe, which opens the form", () => {
    useRecipes.setState({
      recipes: [motionLightRecipe()],
      instances: [recipeInstance("b", { zone: "kitchen" })],
    });
    renderSection();
    expect(screen.getByText("No recipe in Garage")).toBeTruthy();
    // The link opens the list of the recipes; picking one opens the form under the panel.
    fireEvent.click(screen.getByText("Add a recipe"));
    fireEvent.click(screen.getByText("Motion Light"));
    expect(screen.getByRole("button", { name: "Create" })).toBeTruthy();
    expect(screen.getByRole("checkbox", { name: "Garage light" })).toBeTruthy();
  });

  it("no recipe on the host: no panel", () => {
    useRecipes.setState({ recipes: [], instances: [recipeInstance("a", { zone: "garage" })] });
    renderSection();
    expect(screen.queryByRole("heading", { name: "Recipes" })).toBeNull();
    expect(screen.queryByTestId("recipe-row")).toBeNull();
  });
});
