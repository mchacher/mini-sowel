// Ported from Sowel ui/src/components/recipes/RecipeInstanceRow.tsx and RecipeIconTile.tsx (Sowel
// commit 5da84dc1, UI 1.87.0). Cuts: the mobile bottom sheet, duplicate dialog, mode-cycle pills,
// marquee name, alarm dot and summary line, groups. The tile is lucide's `Lightbulb` for the icon
// `Lightbulb` and `ChefHat` otherwise (Sowel resolves the manifest's icon name from a larger set).
// Settings opens a `Modal` (the form of `RecipeForm`). Without the recipe (its id is not in
// `GET /recipes`; there are no slots to build a form from) the row says so: switch off and
// disabled, a "Recipe not installed" label and a Delete beside it (spec 046, R10).
import { useEffect, useRef, useState } from "react";
import { ChefHat, Lightbulb, Settings, ShieldOff } from "lucide-react";
import { useTranslation } from "react-i18next";
import type {
  EquipmentWithDetails,
  RecipeInfo,
  RecipeInstance,
  ZoneAggregatedData,
} from "../../types";
import { useEquipments } from "../../store/useEquipments";
import { useRecipes } from "../../store/useRecipes";
import { apiErrorStatus, apiErrorText } from "../../lib/api-error";
import { instanceEquipmentNames, shortNameList } from "../../lib/recipe-form";
import { recipeName } from "../../lib/recipe-i18n";
import { Modal } from "../ui/Modal";
import { Switch } from "../ui/Switch";
import { CountdownTimer } from "./recipe-fields";
import { RecipeForm } from "./RecipeForm";
import { RecipeJournal } from "./RecipeJournal";

/** How long a refusal stays under the row. */
const ERROR_MS = 5000;

export function RecipeInstanceRow({
  instance,
  recipe,
  zoneId,
  zoneEquipments,
  aggregate,
}: {
  instance: RecipeInstance;
  /** Undefined when the host no longer has the recipe. */
  recipe: RecipeInfo | undefined;
  zoneId: string;
  zoneEquipments: EquipmentWithDetails[];
  aggregate: ZoneAggregatedData | undefined;
}) {
  const { t, i18n } = useTranslation();
  const equipments = useEquipments((s) => s.equipments);
  const [showLog, setShowLog] = useState(false);
  const [editing, setEditing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );

  const refuse = (err: unknown) => {
    // Deleted meanwhile by another browser: the list is read again.
    if (apiErrorStatus(err) === 404) void useRecipes.getState().fetchInstances();
    setError(apiErrorText(err, t));
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => setError(null), ERROR_MS);
  };

  const displayName = recipe ? recipeName(recipe, i18n.language) : instance.recipeId;
  const names = recipe ? instanceEquipmentNames(recipe, instance.params, equipments) : [];
  const list = shortNameList(names);

  const toggle = async () => {
    setBusy(true);
    try {
      if (instance.enabled) await useRecipes.getState().disableInstance(instance.id);
      else await useRecipes.getState().enableInstance(instance.id);
    } catch (err) {
      refuse(err);
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    if (!window.confirm(t("recipes.deleteConfirm"))) return;
    try {
      await useRecipes.getState().deleteInstance(instance.id);
    } catch (err) {
      refuse(err);
    }
  };

  // The instance is `enabled` in the saved home but cannot run without its recipe.
  const missing = recipe === undefined;
  const running = instance.enabled && !missing;

  const timerExpiresAt = instance.state?.timerExpiresAt;
  const showTimer = running && typeof timerExpiresAt === "string" && timerExpiresAt !== "";
  const showOverride = running && !!instance.state?.overrideMode;

  const headerAction = recipe ? (
    <button
      type="button"
      onClick={() => setEditing(true)}
      className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-sm text-caption font-medium text-primary hover:bg-primary-light transition-colors duration-150"
    >
      <Settings size={13} strokeWidth={1.5} aria-hidden="true" />
      {t("recipes.settings")}
    </button>
  ) : undefined;

  const Icon = recipe?.icon === "Lightbulb" ? Lightbulb : ChefHat;

  return (
    <>
      <div className={running ? "" : "opacity-50"} data-testid="recipe-row">
        <div className="grid grid-cols-[32px_1fr_auto] gap-x-[0.85rem] gap-y-0.5 items-center px-[1.1rem] py-2 min-h-[52px]">
          <div
            className={`w-8 h-8 rounded-md flex items-center justify-center row-span-2 self-center ${running ? "bg-primary-light" : "bg-background"}`}
          >
            <Icon
              size={16}
              strokeWidth={1.5}
              aria-hidden="true"
              className={running ? "text-primary" : "text-text-tertiary"}
            />
          </div>
          <button
            type="button"
            aria-expanded={showLog}
            onClick={() => setShowLog((open) => !open)}
            className="col-start-2 row-start-1 text-left min-w-0 text-body font-medium text-text truncate cursor-pointer"
          >
            {displayName}
          </button>
          <Switch
            checked={running}
            onChange={() => void toggle()}
            disabled={busy || missing}
            title={
              missing
                ? t("recipes.notInstalled")
                : running
                  ? t("recipes.disable")
                  : t("recipes.enable")
            }
            label={running ? t("recipes.disable") : t("recipes.enable")}
            className="col-start-3 row-start-1 row-span-2 justify-self-end"
          />
          <div className="col-start-2 row-start-2 flex items-center gap-x-2 min-w-0 text-caption text-text-secondary">
            {names.length > 0 && (
              <span className="flex min-w-0" title={names.join(", ")}>
                <span className="truncate">{list.text}</span>
                {list.more > 0 && (
                  <span className="flex-shrink-0 whitespace-pre"> +{list.more}</span>
                )}
              </span>
            )}
            {missing && (
              <>
                <span className="text-error font-medium">{t("recipes.notInstalled")}</span>
                <button
                  type="button"
                  onClick={() => void remove()}
                  className="px-1.5 py-0.5 rounded-sm text-caption font-medium text-error hover:bg-border-light transition-colors duration-150"
                >
                  {t("common.delete")}
                </button>
              </>
            )}
            {showTimer && <CountdownTimer expiresAt={timerExpiresAt} />}
            {showOverride && (
              <span
                className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-full text-micro font-medium flex-shrink-0 bg-primary-light text-primary"
                title={t("recipes.overrideActive")}
              >
                <ShieldOff size={10} strokeWidth={2} />
                {t("recipes.overrideBadge")}
              </span>
            )}
          </div>
        </div>
        {error && (
          <p role="alert" className="px-[1.1rem] pb-2 text-caption text-error">
            {error}
          </p>
        )}
        {showLog && (
          <div className="pb-3 px-[1.1rem]">
            <div className="bg-border-light/40 rounded-sm px-2.5 pt-1.5 pb-2">
              <RecipeJournal instanceId={instance.id} action={headerAction} />
            </div>
          </div>
        )}
      </div>
      {/* Outside the dimmed row: its opacity would reach the dialog. */}
      {editing && recipe && (
        <Modal
          title={t("recipes.settingsOf", { name: displayName })}
          onClose={() => setEditing(false)}
        >
          <div className="px-6 py-4 overflow-y-auto">
            <RecipeForm
              recipe={recipe}
              zoneId={zoneId}
              equipments={zoneEquipments}
              aggregate={aggregate}
              instance={instance}
              onClose={() => setEditing(false)}
            />
          </div>
        </Modal>
      )}
    </>
  );
}
