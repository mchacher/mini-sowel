// Solite's own file (spec 024): the params logic of Sowel's AddRecipeForm and RecipeSettings
// (ui/src/components/recipes/, Sowel commit 5da84dc1), cut down to pure functions that the form
// calls: which slots show, which equipments a slot offers, the form's values from the slots'
// defaults or from an instance's params, and the params from the values. `hiddenWhen` is kept
// (spec 055); no groups, no cross-zone equipments: the room's own equipments only (spec 024, R2).
import type { EquipmentType, EquipmentWithDetails, RecipeInfo, RecipeSlotDef } from "../types";

/** One field's value: a checkbox (boolean), the ticks of a list, or the text of any other field. */
export type FormValue = string | boolean | string[];
export type FormValues = Record<string, FormValue>;

/** What decides whether the motion-light's `luxThreshold` and `buttons` fields show. */
export interface FormContext {
  /** The room's aggregation has a `luminosity` (not null, not undefined). */
  hasLuminosity: boolean;
  /** The home has a `button` equipment. */
  hasButton: boolean;
}

/** A slot that is not shown as a field (the room's, or a field the room has nothing for). */
export function isSlotShown(slot: RecipeSlotDef, ctx: FormContext): boolean {
  if (slot.type === "zone") return false;
  if (slot.id === "luxThreshold") return ctx.hasLuminosity;
  if (slot.id === "buttons") return ctx.hasButton;
  return true;
}

/**
 * Whether a slot is hidden by its `hiddenWhen` (Sowel's `isSlotHidden`, `ui/src/lib/recipe-slots.ts`):
 * the sibling slot's value in the form, or its `defaultValue` while the form has none, is one of
 * `equals`.
 */
export function isSlotHidden(
  slot: RecipeSlotDef,
  values: FormValues,
  all: readonly RecipeSlotDef[],
): boolean {
  const rule = slot.hiddenWhen;
  if (!rule) return false;
  const typed = values[rule.slot];
  const value =
    typeof typed === "string" && typed !== ""
      ? typed
      : all.find((s) => s.id === rule.slot)?.defaultValue;
  const expected = Array.isArray(rule.equals) ? rule.equals : [rule.equals];
  return expected.includes(value as string);
}

/** The slots shown as fields, in the recipe's order (`values`: the form's, for `hiddenWhen`). */
export function shownSlots(
  recipe: RecipeInfo,
  ctx: FormContext,
  values: FormValues = {},
): RecipeSlotDef[] {
  return recipe.slots.filter(
    (slot) => isSlotShown(slot, ctx) && !isSlotHidden(slot, values, recipe.slots),
  );
}

/**
 * The data aliases a `data-key` slot offers (Sowel's RecipeSettings / AddRecipeForm): the data
 * bindings of the equipment chosen in the recipe's first single `equipment` slot; none until one is
 * chosen.
 */
export function dataKeyAliases(
  recipe: RecipeInfo,
  values: FormValues,
  equipments: readonly Pick<EquipmentWithDetails, "id" | "dataBindings">[],
): string[] {
  const slot = recipe.slots.find((s) => s.type === "equipment" && !s.list);
  const chosen = slot ? values[slot.id] : undefined;
  const found = typeof chosen === "string" ? equipments.find((e) => e.id === chosen) : undefined;
  return (found?.dataBindings ?? []).map((b) => b.alias);
}

/** The room's equipments whose type matches the slot's `constraints.equipmentType` (any when none). */
export function equipmentCandidates<T extends { type: EquipmentType | string }>(
  equipments: readonly T[],
  slot: RecipeSlotDef,
): T[] {
  const constraint = slot.constraints?.equipmentType;
  if (!constraint) return [...equipments];
  const types: string[] = Array.isArray(constraint) ? constraint : [constraint];
  return equipments.filter((eq) => types.includes(eq.type));
}

function emptyValue(slot: RecipeSlotDef): FormValue {
  if (slot.type === "boolean") return false;
  if (slot.list) return [];
  return "";
}

function defaultValue(slot: RecipeSlotDef, zoneId: string): FormValue {
  if (slot.type === "zone") return zoneId;
  const value = slot.defaultValue;
  if (value === undefined || value === null) return emptyValue(slot);
  if (slot.type === "boolean") return value === true || value === "true";
  if (slot.list) return Array.isArray(value) ? value.map(String) : [];
  return String(value);
}

/** The form at creation: the zone slot is the room, a slot with a `defaultValue` starts with it. */
export function defaultValues(recipe: RecipeInfo, zoneId: string): FormValues {
  const values: FormValues = {};
  for (const slot of recipe.slots) values[slot.id] = defaultValue(slot, zoneId);
  return values;
}

/**
 * The form of an instance being edited, from its `params`. An equipment id that is not among
 * `candidateIds` (the form passes every equipment the home has) is dropped; a missing param falls back to the slot's default. A duration
 * stays the text it is (`"20s"`): `durationMinutesText` shows it, and it goes back unchanged
 * unless edited.
 */
export function valuesFromParams(
  recipe: RecipeInfo,
  params: Record<string, unknown>,
  zoneId: string,
  candidateIds: (slot: RecipeSlotDef) => ReadonlySet<string>,
): FormValues {
  const values: FormValues = {};
  for (const slot of recipe.slots) {
    const raw = params[slot.id];
    if (slot.type === "zone") {
      values[slot.id] = zoneId;
    } else if (raw === undefined || raw === null) {
      values[slot.id] = defaultValue(slot, zoneId);
    } else if (slot.type === "boolean") {
      values[slot.id] = raw === true || raw === "true";
    } else if (slot.type === "equipment") {
      const ids = candidateIds(slot);
      if (slot.list) {
        const list = Array.isArray(raw) ? raw : typeof raw === "string" ? raw.split(",") : [];
        values[slot.id] = list.filter((id): id is string => typeof id === "string" && ids.has(id));
      } else {
        values[slot.id] = typeof raw === "string" && ids.has(raw) ? raw : "";
      }
    } else if (slot.list) {
      values[slot.id] = Array.isArray(raw) ? raw.map(String) : [];
    } else {
      values[slot.id] = typeof raw === "string" || typeof raw === "number" ? String(raw) : "";
    }
  }
  return values;
}

/** The minutes a duration text such as `"10m"`, `"30s"` or `"1h"` stands for; null when it is none. */
export function durationMinutes(text: string): number | null {
  const match = /^(\d+)\s*(s|m|h)$/.exec(text);
  if (!match) return null;
  const n = parseInt(match[1], 10);
  return match[2] === "s" ? n / 60 : match[2] === "h" ? n * 60 : n;
}

/** What the minutes field shows: the minutes rounded to 2 decimals, empty when the text is no duration. */
export function durationMinutesText(text: string): string {
  const minutes = durationMinutes(text);
  return minutes === null ? "" : String(Math.round(minutes * 100) / 100);
}

/** What the minutes field stores when the user types `minutes`: `"<n>m"`, or `""` when emptied. */
export function durationFromMinutes(minutes: string): string {
  return minutes === "" ? "" : `${minutes}m`;
}

function isEmpty(slot: RecipeSlotDef, value: FormValue | undefined): boolean {
  if (slot.type === "boolean") return false;
  if (Array.isArray(value)) return value.length === 0;
  return value === undefined || value === "";
}

/** The shown, required slots left empty (a checkbox is never empty; the room's slot is never shown). */
export function missingSlots(
  recipe: RecipeInfo,
  values: FormValues,
  ctx: FormContext,
): RecipeSlotDef[] {
  return shownSlots(recipe, ctx, values).filter(
    (slot) => slot.required && isEmpty(slot, values[slot.id]),
  );
}

/**
 * The params to send. The zone slot is the room's id; a slot that is not shown, or hidden by its `hiddenWhen`, sends `""` (`[]` for
 * a list); a checkbox sends `true`/`false`, a number slot its number (`""` when empty), a list its array, anything else its text.
 */
export function paramsFromValues(
  recipe: RecipeInfo,
  values: FormValues,
  zoneId: string,
  ctx: FormContext,
): Record<string, unknown> {
  const params: Record<string, unknown> = {};
  for (const slot of recipe.slots) {
    const value = values[slot.id];
    if (slot.type === "zone") {
      params[slot.id] = zoneId;
    } else if (!isSlotShown(slot, ctx) || isSlotHidden(slot, values, recipe.slots)) {
      params[slot.id] = slot.list ? [] : "";
    } else if (slot.type === "boolean") {
      params[slot.id] = value === true;
    } else if (slot.list) {
      params[slot.id] = Array.isArray(value) ? value : [];
    } else if (slot.type === "number") {
      const text = typeof value === "string" ? value.trim() : "";
      const n = Number(text);
      params[slot.id] = text !== "" && Number.isFinite(n) ? n : text;
    } else {
      params[slot.id] = typeof value === "string" ? value : "";
    }
  }
  return params;
}

/**
 * The names of the equipments of the recipe's first `equipment` slot, in the order the settings
 * list them; an id no equipment has any more is skipped (Sowel's `instanceEquipmentNames`).
 */
export function instanceEquipmentNames(
  recipe: RecipeInfo,
  params: Record<string, unknown>,
  equipments: readonly { id: string; name: string }[],
): string[] {
  const slot = recipe.slots.find((s) => s.type === "equipment");
  if (!slot) return [];
  const raw = params[slot.id];
  const ids = Array.isArray(raw) ? raw : typeof raw === "string" ? raw.split(",") : [];
  const byId = new Map(equipments.map((e) => [e.id, e.name]));
  return ids.flatMap((id) => {
    const name = typeof id === "string" ? byId.get(id.trim()) : undefined;
    return name ? [name] : [];
  });
}

/** `A, B` and how many more: the first `shown` names, then the count (Sowel's `shortNameList`). */
export function shortNameList(names: readonly string[], shown = 3): { text: string; more: number } {
  return { text: names.slice(0, shown).join(", "), more: Math.max(0, names.length - shown) };
}
