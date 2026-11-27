import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { light, mockApi, motionLightRecipe, recipeInstance, resetStores } from "../../test-utils";
import { useEquipments } from "../../store/useEquipments";
import { useRecipes } from "../../store/useRecipes";
import type { RecipeInfo, RecipeInstance } from "../../types";
import { RecipeInstanceRow } from "./RecipeInstanceRow";

afterEach(() => {
  cleanup();
  resetStores();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

const lights = [light("l1", "Garage light", "garage", false)];

function renderRow(instance: RecipeInstance, recipe: RecipeInfo | null = motionLightRecipe()) {
  useEquipments.setState({ equipments: lights });
  useRecipes.setState({ instances: [instance] });
  render(
    <RecipeInstanceRow
      instance={instance}
      recipe={recipe ?? undefined}
      zoneId="garage"
      zoneEquipments={lights}
      aggregate={undefined}
    />,
  );
}

describe("RecipeInstanceRow", () => {
  it("the switch disables an enabled instance, enables a disabled one", async () => {
    const api = mockApi({
      "POST /api/v1/recipe-instances/i1/disable": { status: 204 },
      "POST /api/v1/recipe-instances/i1/enable": { status: 204 },
      "GET /api/v1/recipe-instances": { body: [] },
    });
    renderRow(recipeInstance("i1", { zone: "garage", lights: ["l1"] }));
    fireEvent.click(screen.getByRole("switch", { name: "Disable recipe" }));
    await waitFor(() => expect(api.count("POST /api/v1/recipe-instances/i1/disable")).toBe(1));
    cleanup();
    renderRow(recipeInstance("i1", { zone: "garage", lights: ["l1"] }, { enabled: false }));
    const row = screen.getByTestId("recipe-row");
    expect(row.className).toContain("opacity-50"); // dimmed
    fireEvent.click(screen.getByRole("switch", { name: "Enable recipe" }));
    await waitFor(() => expect(api.count("POST /api/v1/recipe-instances/i1/enable")).toBe(1));
  });

  it("a refusal shows its text for 5 s, and the switch stays as the host has it", async () => {
    mockApi({
      "POST /api/v1/recipe-instances/i1/enable": {
        status: 400,
        body: { error: "Invalid params: At least one light is required" },
      },
    });
    renderRow(recipeInstance("i1", { zone: "garage", lights: [] }, { enabled: false }));
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      fireEvent.click(screen.getByRole("switch"));
      await waitFor(() =>
        expect(screen.getByRole("alert").textContent).toContain("At least one light is required"),
      );
      expect(screen.getByRole("switch").getAttribute("aria-checked")).toBe("false");
      await vi.advanceTimersByTimeAsync(5000);
      await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
    } finally {
      vi.useRealTimers();
    }
  });

  it("an instance deleted in another browser: the 404 is shown and the list is read again", async () => {
    const api = mockApi({
      "POST /api/v1/recipe-instances/i1/disable": {
        status: 404,
        body: { error: "Instance not found" },
      },
      "GET /api/v1/recipe-instances": { body: [] },
    });
    renderRow(recipeInstance("i1", { zone: "garage", lights: ["l1"] }));
    fireEvent.click(screen.getByRole("switch"));
    await waitFor(() =>
      // The interface's own text for a 404 (`apiErrorText`), as the other pages show it.
      expect(screen.getByRole("alert").textContent).toContain("does not exist"),
    );
    await waitFor(() => expect(api.count("GET /api/v1/recipe-instances")).toBe(1));
    await waitFor(() => expect(useRecipes.getState().instances).toEqual([]));
  });

  it("shows the name, the equipments of the first equipment slot, the timer and override pills while enabled", () => {
    const soon = new Date(Date.now() + 90_000).toISOString();
    renderRow(
      recipeInstance(
        "i1",
        { zone: "garage", lights: ["l1"] },
        { state: { timerExpiresAt: soon, overrideMode: true } },
      ),
    );
    expect(screen.getByText("Motion Light")).toBeTruthy();
    expect(screen.getByText("Garage light")).toBeTruthy();
    expect(screen.getByTestId("recipe-countdown")).toBeTruthy();
    expect(screen.getByText("Override")).toBeTruthy();
  });

  it("shows no pill while disabled", () => {
    const soon = new Date(Date.now() + 90_000).toISOString();
    renderRow(
      recipeInstance(
        "i1",
        { zone: "garage", lights: ["l1"] },
        { enabled: false, state: { timerExpiresAt: soon, overrideMode: true } },
      ),
    );
    expect(screen.queryByTestId("recipe-countdown")).toBeNull();
    expect(screen.queryByText("Override")).toBeNull();
  });

  it("an instance whose recipe is not installed: switch off and disabled, a label, a working Delete", async () => {
    const api = mockApi({
      "DELETE /api/v1/recipe-instances/i1": { status: 204 },
      "GET /api/v1/recipe-instances": { body: [] },
    });
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    renderRow(recipeInstance("i1", { zone: "garage" }, { recipeId: "old-recipe" }), null);
    expect(screen.getByText("old-recipe")).toBeTruthy();
    expect(screen.getByText("Recipe not installed")).toBeTruthy();
    const sw = screen.getByRole("switch") as HTMLButtonElement;
    expect(sw.getAttribute("aria-checked")).toBe("false");
    expect(sw.disabled).toBe(true);
    expect(screen.getByTestId("recipe-row").className).toContain("opacity-50");
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(confirm).toHaveBeenCalled();
    await waitFor(() => expect(api.count("DELETE /api/v1/recipe-instances/i1")).toBe(1));
  });

  it("a row whose recipe exists has no missing-recipe label or Delete, its switch is on", () => {
    renderRow(recipeInstance("i1", { zone: "garage", lights: ["l1"] }));
    expect(screen.queryByText("Recipe not installed")).toBeNull();
    expect(screen.queryByRole("button", { name: "Delete" })).toBeNull();
    const sw = screen.getByRole("switch") as HTMLButtonElement;
    expect(sw.getAttribute("aria-checked")).toBe("true");
    expect(sw.disabled).toBe(false);
  });

  it("the journal of an instance without recipe opens and offers no Settings", async () => {
    mockApi({ "GET /api/v1/recipe-instances/i1/log?limit=50": { body: [] } });
    renderRow(recipeInstance("i1", { zone: "garage" }, { recipeId: "old-recipe" }), null);
    fireEvent.click(screen.getByText("old-recipe"));
    await waitFor(() => expect(screen.getByText("No entries")).toBeTruthy());
    expect(screen.queryByRole("button", { name: "Settings" })).toBeNull();
  });

  it("the journal's Settings button opens the form in a dialog, filled with the instance", async () => {
    mockApi({ "GET /api/v1/recipe-instances/i1/log?limit=50": { body: [] } });
    renderRow(recipeInstance("i1", { zone: "garage", lights: ["l1"], timeout: "20m" }));
    fireEvent.click(screen.getByText("Motion Light"));
    fireEvent.click(await screen.findByRole("button", { name: "Settings" }));
    expect(screen.getByRole("dialog", { name: "Settings of Motion Light" })).toBeTruthy();
    expect((screen.getByLabelText("Timeout") as HTMLInputElement).value).toBe("20");
    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });
});
