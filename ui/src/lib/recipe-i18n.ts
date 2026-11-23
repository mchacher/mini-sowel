// Ported from Sowel ui/src/lib/recipe-i18n.ts (Sowel commit 5da84dc1, UI 1.87.0). Cuts: the group label
// (Solite's form has no slot groups).
import type { RecipeInfo, RecipeSlotDef } from "../types";

/** A recipe's name in the given language; the English name embedded in the recipe otherwise. */
export function recipeName(recipe: RecipeInfo, lang: string): string {
  return recipe.i18n?.[lang]?.name ?? recipe.name;
}

export function recipeDescription(recipe: RecipeInfo, lang: string): string {
  return recipe.i18n?.[lang]?.description ?? recipe.description;
}

export function recipeSlotName(recipe: RecipeInfo, slot: RecipeSlotDef, lang: string): string {
  return recipe.i18n?.[lang]?.slots?.[slot.id]?.name ?? slot.name;
}

export function recipeSlotDescription(
  recipe: RecipeInfo,
  slot: RecipeSlotDef,
  lang: string,
): string {
  return recipe.i18n?.[lang]?.slots?.[slot.id]?.description ?? slot.description;
}

/** A `select` option's label: the language's, the option's English one, the raw value. */
export function recipeSlotOptionLabel(
  recipe: RecipeInfo,
  slot: RecipeSlotDef,
  value: string,
  lang: string,
): string {
  return (
    recipe.i18n?.[lang]?.slots?.[slot.id]?.options?.[value] ??
    slot.options?.find((o) => o.value === value)?.label ??
    value
  );
}
