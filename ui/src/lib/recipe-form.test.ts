import { describe, expect, it } from "vitest";
import { equipment, light, motionLightRecipe } from "../test-utils";
import type { RecipeInfo } from "../types";
import {
  defaultValues,
  durationMinutesText,
  isSlotHidden,
  equipmentCandidates,
  instanceEquipmentNames,
  missingSlots,
  paramsFromValues,
  shortNameList,
  shownSlots,
  valuesFromParams,
  type FormContext,
} from "./recipe-form";

const recipe = motionLightRecipe();
const both: FormContext = { hasLuminosity: true, hasButton: true };
const neither: FormContext = { hasLuminosity: false, hasButton: false };
const slot = (id: string) => recipe.slots.find((s) => s.id === id)!;

describe("recipe-form", () => {
  it("defaults: zone is the room, timeout is 10m, the others are empty", () => {
    expect(defaultValues(recipe, "garage")).toEqual({
      zone: "garage",
      lights: [],
      timeout: "10m",
      luxThreshold: "",
      buttons: [],
    });
  });

  it("form to params: booleans, lists, durations and text", () => {
    const withBoolean = {
      ...recipe,
      slots: [
        ...recipe.slots,
        { id: "dark", name: "Dark", description: "", type: "boolean" as const, required: false },
        { id: "note", name: "Note", description: "", type: "text" as const, required: false },
      ],
    };
    const values = {
      ...defaultValues(withBoolean, "garage"),
      lights: ["a", "b"],
      timeout: "20m",
      luxThreshold: "50",
      dark: true,
      note: "hi",
    };
    expect(paramsFromValues(withBoolean, values, "garage", both)).toEqual({
      zone: "garage",
      lights: ["a", "b"],
      timeout: "20m",
      luxThreshold: "50",
      buttons: [],
      dark: true,
      note: "hi",
    });
    const empty = { ...values, lights: [], timeout: "", dark: false };
    const params = paramsFromValues(withBoolean, empty, "garage", both);
    expect(params.lights).toEqual([]);
    expect(params.timeout).toBe("");
    expect(params.dark).toBe(false);
  });

  it("a hidden slot sends an empty text, or an empty list", () => {
    const values = { ...defaultValues(recipe, "g"), luxThreshold: "50", buttons: ["x"] };
    const params = paramsFromValues(recipe, values, "g", neither);
    expect(params.luxThreshold).toBe("");
    expect(params.buttons).toEqual([]);
  });

  it("params to form: ticks, checkbox, 20s as 0.33 min, ids no longer in the room dropped", () => {
    const withBoolean = {
      ...recipe,
      slots: [
        ...recipe.slots,
        { id: "dark", name: "Dark", description: "", type: "boolean" as const, required: false },
      ],
    };
    const values = valuesFromParams(
      withBoolean,
      { zone: "other", lights: ["a", "gone"], timeout: "20s", luxThreshold: 40, dark: true },
      "garage",
      () => new Set(["a", "b"]),
    );
    expect(values.zone).toBe("garage");
    expect(values.lights).toEqual(["a"]);
    expect(values.timeout).toBe("20s"); // sent back unchanged unless edited
    expect(durationMinutesText("20s")).toBe("0.33");
    expect(durationMinutesText("10m")).toBe("10");
    expect(durationMinutesText("1h")).toBe("60");
    expect(durationMinutesText("soon")).toBe("");
    expect(values.luxThreshold).toBe("40");
    expect(values.dark).toBe(true);
  });

  it("a required slot left empty is returned as missing, and only it", () => {
    const values = defaultValues(recipe, "g");
    expect(missingSlots(recipe, values, both).map((s) => s.name)).toEqual(["Lights"]);
    expect(missingSlots(recipe, { ...values, lights: ["a"] }, both)).toEqual([]);
  });

  it("luxThreshold and buttons show only with the room's luminosity and a button", () => {
    const ids = (ctx: FormContext) => shownSlots(recipe, ctx).map((s) => s.id);
    expect(ids(neither)).toEqual(["lights", "timeout"]);
    expect(ids({ hasLuminosity: true, hasButton: false })).toEqual([
      "lights",
      "timeout",
      "luxThreshold",
    ]);
    expect(ids(both)).toEqual(["lights", "timeout", "luxThreshold", "buttons"]);
  });

  it("equipment candidates are the room's equipments of the slot's type", () => {
    const room = [
      light("l1", "Light", "g", false),
      equipment("b1", "Button", "button", "g"),
      equipment("s1", "Sensor", "sensor", "g"),
    ];
    expect(equipmentCandidates(room, slot("lights")).map((e) => e.id)).toEqual(["l1"]);
    expect(equipmentCandidates(room, slot("buttons")).map((e) => e.id)).toEqual(["b1"]);
  });

  it("names the equipments of the first equipment slot, three at most then the count", () => {
    const all = ["a", "b", "c", "d", "e"].map((id) => ({ id, name: id.toUpperCase() }));
    const names = instanceEquipmentNames(recipe, { lights: ["a", "b", "c", "d", "gone"] }, all);
    expect(names).toEqual(["A", "B", "C", "D"]);
    expect(shortNameList(names)).toEqual({ text: "A, B, C", more: 1 });
  });
});

// A recipe with Sowel's `hiddenWhen`, as `schedule-on-off` has it: a fixed-time picker hidden
// by sunrise or sunset, an offset hidden by a fixed time.
const scheduled: RecipeInfo = {
  id: "sched",
  name: "Sched",
  description: "",
  slots: [
    { id: "zone", name: "Zone", description: "", type: "zone", required: true },
    {
      id: "kind",
      name: "Start",
      description: "",
      type: "select",
      required: true,
      defaultValue: "time",
      options: [
        { value: "time", label: "Fixed time" },
        { value: "sunrise", label: "Sunrise" },
      ],
    },
    {
      id: "at",
      name: "Time",
      description: "",
      type: "time",
      required: true,
      hiddenWhen: { slot: "kind", equals: ["sunrise", "sunset"] },
    },
    {
      id: "offset",
      name: "Offset",
      description: "",
      type: "number",
      required: false,
      hiddenWhen: { slot: "kind", equals: "time" },
    },
  ],
};

describe("recipe-form hiddenWhen", () => {
  it("a slot is hidden when the sibling's value, or its default, is one of equals", () => {
    const ids = (kind: string | null) =>
      shownSlots(scheduled, neither, kind === null ? {} : { kind }).map((s) => s.id);
    expect(ids(null)).toEqual(["kind", "at"]); // the default "time" hides the offset
    expect(ids("sunrise")).toEqual(["kind", "offset"]);
    expect(isSlotHidden(scheduled.slots[2], { kind: "sunrise" }, scheduled.slots)).toBe(true);
    expect(isSlotHidden(scheduled.slots[1], { kind: "sunrise" }, scheduled.slots)).toBe(false);
  });

  it("a hidden slot is never required and goes out empty", () => {
    const values = { ...defaultValues(scheduled, "g"), kind: "sunrise" };
    expect(missingSlots(scheduled, values, neither)).toEqual([]); // `at` is required but hidden
    expect(missingSlots(scheduled, { ...values, kind: "time" }, neither).map((s) => s.id)).toEqual([
      "at",
    ]);
    expect(paramsFromValues(scheduled, { ...values, at: "07:30" }, "g", neither)).toEqual({
      zone: "g",
      kind: "sunrise",
      at: "",
      offset: "",
    });
  });
});
