// Ported from Sowel ui/src/components/recipes/ZoneRecipesSection.tsx (Sowel commit 5da84dc1,
// UI 1.87.0). Cuts: the admin check (the box has no accounts), recipe groups, the mount fetch (the
// live connection reads the recipes and the instances when it opens). The section is a Panel of its
// own (the « + » in its lid) under the room's equipments, and the form to create opens under it.
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { EquipmentWithDetails, ZoneAggregatedData } from "../../types";
import { useRecipes } from "../../store/useRecipes";
import { Panel, SliceAddButton } from "../home/Panel";
import { RecipeForm } from "./RecipeForm";
import { RecipeInstanceRow } from "./RecipeInstanceRow";
import { RecipePickerPopover } from "./RecipePickerPopover";

export function ZoneRecipesSection({
  zoneId,
  zoneName,
  equipments,
  aggregate,
}: {
  zoneId: string;
  zoneName: string;
  /** The equipments of the room (not of its sub-rooms). */
  equipments: EquipmentWithDetails[];
  aggregate: ZoneAggregatedData | undefined;
}) {
  const { t } = useTranslation();
  const recipes = useRecipes((s) => s.recipes);
  const instances = useRecipes((s) => s.instances);
  const [showPicker, setShowPicker] = useState(false);
  const [pickedId, setPickedId] = useState<string | null>(null);
  const pickerWrap = useRef<HTMLDivElement>(null);

  // A click outside the list closes it.
  useEffect(() => {
    if (!showPicker) return;
    const onDown = (e: MouseEvent) => {
      if (pickerWrap.current && !pickerWrap.current.contains(e.target as Node)) {
        setShowPicker(false);
      }
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [showPicker]);

  if (recipes.length === 0) return null;

  const mine = instances.filter((instance) => instance.params.zone === zoneId);
  const picked = recipes.find((r) => r.id === pickedId);

  return (
    <Panel
      title={t("recipes.title")}
      count={mine.length}
      headerRight={
        <div ref={pickerWrap} className="relative">
          <SliceAddButton
            onClick={() => setShowPicker((open) => !open)}
            title={t("recipes.addRecipe")}
          />
          {showPicker && (
            <RecipePickerPopover
              recipes={recipes}
              onPick={(id) => {
                setShowPicker(false);
                setPickedId(id);
              }}
            />
          )}
        </div>
      }
    >
      <div>
        {mine.length > 0 && (
          <div className="slice-rows">
            {mine.map((instance) => (
              <RecipeInstanceRow
                key={instance.id}
                instance={instance}
                recipe={recipes.find((r) => r.id === instance.recipeId)}
                zoneId={zoneId}
                zoneEquipments={equipments}
                aggregate={aggregate}
              />
            ))}
          </div>
        )}
        {mine.length === 0 && !picked && (
          <div className="flex items-center justify-center gap-2 px-4 py-3 text-caption text-text-tertiary">
            <span>{t("recipes.noActiveRecipes", { name: zoneName })}</span>
            <button
              type="button"
              onClick={() => setShowPicker(true)}
              className="text-primary hover:text-primary-hover transition-colors duration-150"
            >
              {t("recipes.addRecipe")}
            </button>
          </div>
        )}
        {picked && (
          <div className="border-t border-border-light px-4 py-3 bg-border-light/20">
            <RecipeForm
              key={picked.id}
              recipe={picked}
              zoneId={zoneId}
              equipments={equipments}
              aggregate={aggregate}
              onClose={() => setPickedId(null)}
            />
          </div>
        )}
      </div>
    </Panel>
  );
}
