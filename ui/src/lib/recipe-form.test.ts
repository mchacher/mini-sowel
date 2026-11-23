import { describe, expect, it } from "vitest";
import { equipment, light, motionLightRecipe } from "../test-utils";
import {
  defaultValues,
  durationMinutesText,
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
