import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { light, mockApi, motionLightRecipe, recipeInstance, resetStores } from "../../test-utils";
import { RecipeForm } from "./RecipeForm";

afterEach(() => {
  cleanup();
  resetStores();
  vi.unstubAllGlobals();
});

const recipe = motionLightRecipe();
const room = [
  light("l1", "Garage light", "garage", false),
  light("l2", "Workshop", "garage", true),
];

function renderForm(
  onClose = vi.fn(),
  instance = undefined as ReturnType<typeof recipeInstance> | undefined,
) {
  render(
    <RecipeForm
      recipe={recipe}
      zoneId="garage"
      equipments={room}
      aggregate={undefined}
      instance={instance}
      onClose={onClose}
    />,
  );
  return onClose;
}

describe("RecipeForm", () => {
  it("create: ticking the light and Create sends Sowel's body, then the form closes", async () => {
    const api = mockApi({
      "POST /api/v1/recipe-instances": { status: 201, body: recipeInstance("i1", {}) },
      "GET /api/v1/recipe-instances": { body: [] },
    });
    const onClose = renderForm();
    expect(screen.getByText("Turns the lights on with motion and off after a delay.")).toBeTruthy();
    expect((screen.getByLabelText("Timeout") as HTMLInputElement).value).toBe("10");
    expect(screen.queryByLabelText("Lux threshold")).toBeNull(); // the room has no luminosity
    fireEvent.click(screen.getByRole("checkbox", { name: "Garage light" }));
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(api.calls[0]).toEqual({
      key: "POST /api/v1/recipe-instances",
      body: {
        recipeId: "motion-light",
        params: { zone: "garage", lights: ["l1"], timeout: "10m", luxThreshold: "", buttons: [] },
      },
    });
  });

  it("a required slot left empty shows its name, and sends nothing", () => {
    const api = mockApi({});
    const onClose = renderForm();
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    expect(screen.getByRole("alert").textContent).toBe("Lights is required");
    expect(api.calls).toEqual([]);
    expect(onClose).not.toHaveBeenCalled();
  });

  it("a refusal shows the host's text under the form, which stays open with what was typed", async () => {
    mockApi({
      "POST /api/v1/recipe-instances": {
        status: 400,
        body: { error: "Invalid params: Timeout must be positive" },
      },
    });
    const onClose = renderForm();
    fireEvent.click(screen.getByRole("checkbox", { name: "Garage light" }));
    fireEvent.input(screen.getByLabelText("Timeout"), { target: { value: "0" } });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain("Timeout must be positive"),
    );
    expect(onClose).not.toHaveBeenCalled();
    expect((screen.getByLabelText("Timeout") as HTMLInputElement).value).toBe("0");
    expect(
      (screen.getByRole("checkbox", { name: "Garage light" }) as HTMLInputElement).checked,
    ).toBe(true);
    expect((screen.getByRole("button", { name: "Create" }) as HTMLButtonElement).disabled).toBe(
      false,
    );
  });

  it("edit: the fields start from the params, and Save sends PUT {params}", async () => {
    const api = mockApi({
      "PUT /api/v1/recipe-instances/i1": { body: recipeInstance("i1", {}) },
      "GET /api/v1/recipe-instances": { body: [] },
    });
    const instance = recipeInstance("i1", {
      zone: "garage",
      lights: ["l2", "gone"],
      timeout: "20m",
    });
    const onClose = renderForm(vi.fn(), instance);
    expect((screen.getByRole("checkbox", { name: "Workshop" }) as HTMLInputElement).checked).toBe(
      true,
    );
    expect(
      (screen.getByRole("checkbox", { name: "Garage light" }) as HTMLInputElement).checked,
    ).toBe(false);
    expect((screen.getByLabelText("Timeout") as HTMLInputElement).value).toBe("20");
    fireEvent.input(screen.getByLabelText("Timeout"), { target: { value: "5" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(api.calls[0]).toEqual({
      key: "PUT /api/v1/recipe-instances/i1",
      body: {
        params: { zone: "garage", lights: ["l2"], timeout: "5m", luxThreshold: "", buttons: [] },
      },
    });
  });
});

describe("RecipeForm, a text slot and a slot hidden by hiddenWhen", () => {
  const info = {
    id: "text-recipe",
    name: "Text recipe",
    description: "Watches a state.",
    slots: [
      { id: "zone", name: "Zone", description: "", type: "zone" as const, required: true },
      {
        id: "stateValue",
        name: "Target value",
        description: "",
        type: "text" as const,
        required: true,
      },
      {
        id: "kind",
        name: "Start",
        description: "",
        type: "select" as const,
        required: true,
        defaultValue: "time",
        options: [
          { value: "time", label: "Fixed time" },
          { value: "sunrise", label: "Sunrise" },
        ],
      },
      {
        id: "at",
        name: "Start time",
        description: "",
        type: "time" as const,
        required: false,
        hiddenWhen: { slot: "kind", equals: ["sunrise", "sunset"] },
      },
    ],
  };

  it("a text slot is a text field, and a slot hides and shows with the select it names", async () => {
    const api = mockApi({
      "POST /api/v1/recipe-instances": { status: 201, body: recipeInstance("i2", {}) },
      "GET /api/v1/recipe-instances": { body: [] },
    });
    const onClose = vi.fn();
    render(
      <RecipeForm
        recipe={info}
        zoneId="garage"
        equipments={room}
        aggregate={undefined}
        onClose={onClose}
      />,
    );
    const target = screen.getByLabelText(/Target value/) as HTMLInputElement;
    expect(target.type).toBe("text");
    fireEvent.input(target, { target: { value: "open" } });
    expect(screen.getByLabelText("Start time")).toBeTruthy();
    fireEvent.change(screen.getByLabelText(/^Start( \*)?$/), { target: { value: "sunrise" } });
    expect(screen.queryByLabelText("Start time")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(api.calls[0].body).toEqual({
      recipeId: "text-recipe",
      params: { zone: "garage", stateValue: "open", kind: "sunrise", at: "" },
    });
  });
});

describe("RecipeForm, a data-key slot", () => {
  const info = {
    id: "watch",
    name: "Watch",
    description: "Watches a data.",
    slots: [
      { id: "zone", name: "Zone", description: "", type: "zone" as const, required: true },
      {
        id: "equipment",
        name: "Equipment",
        description: "",
        type: "equipment" as const,
        required: true,
      },
      {
        id: "dataKey",
        name: "Data key",
        description: "",
        type: "data-key" as const,
        required: true,
      },
    ],
  };

  it("offers the aliases of the chosen equipment and sends the one picked", async () => {
    const api = mockApi({
      "POST /api/v1/recipe-instances": { status: 201, body: recipeInstance("i3", {}) },
      "GET /api/v1/recipe-instances": { body: [] },
    });
    const onClose = vi.fn();
    render(
      <RecipeForm
        recipe={info}
        zoneId="garage"
        equipments={room}
        aggregate={undefined}
        onClose={onClose}
      />,
    );
    const key = screen.getByLabelText(/Data key/) as HTMLSelectElement;
    expect(Array.from(key.options).map((o) => o.value)).toEqual([""]); // no equipment yet
    fireEvent.change(screen.getByLabelText(/^Equipment/), { target: { value: "l1" } });
    expect(Array.from(key.options).map((o) => o.value)).toEqual(["", "state"]);
    fireEvent.change(key, { target: { value: "state" } });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(api.calls[0].body).toEqual({
      recipeId: "watch",
      params: { zone: "garage", equipment: "l1", dataKey: "state" },
    });
  });
});
