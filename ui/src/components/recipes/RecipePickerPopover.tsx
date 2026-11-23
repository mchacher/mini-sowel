// Ported from Sowel ui/src/components/recipes/RecipePickerPopover.tsx (Sowel commit 5da84dc1,
// UI 1.87.0). Cuts: the mobile bottom sheet, and the filter by what the room has (a plain list of
// the host's recipes, spec 024, R1).
import { ChefHat } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { RecipeInfo } from "../../types";
import { recipeDescription, recipeName } from "../../lib/recipe-i18n";

export function RecipePickerPopover({
  recipes,
  onPick,
}: {
  recipes: RecipeInfo[];
  onPick: (recipeId: string) => void;
}) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  return (
    <div className="absolute right-0 top-full mt-1 z-20 w-[320px] max-w-[90vw] max-h-[360px] overflow-y-auto bg-surface border border-border rounded-md shadow-lg p-1">
      {recipes.length === 0 ? (
        <div className="px-3 py-4 text-center text-caption text-text-tertiary">
          {t("recipes.noRecipesAvailable")}
        </div>
      ) : (
        recipes.map((recipe) => (
          <button
            key={recipe.id}
            type="button"
            onClick={() => onPick(recipe.id)}
            className="w-full flex items-center gap-2.5 px-2.5 py-2 rounded-sm hover:bg-border-light/60 transition-colors duration-150 text-left"
          >
            <div className="w-7 h-7 rounded-sm bg-border-light text-text-secondary flex items-center justify-center flex-shrink-0">
              <ChefHat size={14} strokeWidth={1.5} />
            </div>
            <div className="min-w-0 flex-1">
              <div className="text-small font-medium text-text leading-tight">
                {recipeName(recipe, lang)}
              </div>
              <div className="text-micro text-text-tertiary leading-snug mt-0.5">
                {recipeDescription(recipe, lang)}
              </div>
            </div>
          </button>
        ))
      )}
    </div>
  );
}
