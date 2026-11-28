// Ported from Sowel ui/src/components/recipes/AddRecipeForm.tsx and the form of RecipeSettings.tsx
// (Sowel commit 5da84dc1, UI 1.87.0), as one form for create and edit (spec 024, R2). Cuts: slot
// groups, the zone-first equipment pickers, `data-key` and `time` pickers (a text field), the
// bottom sheet and the "changed" counters. The params logic is in `lib/recipe-form.ts`.
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type {
  EquipmentWithDetails,
  RecipeInfo,
  RecipeInstance,
  RecipeSlotDef,
  ZoneAggregatedData,
} from "../../types";
import { useEquipments } from "../../store/useEquipments";
import { useRecipes } from "../../store/useRecipes";
import { apiErrorText } from "../../lib/api-error";
import {
  defaultValues,
  equipmentCandidates,
  missingSlots,
  paramsFromValues,
  shownSlots,
  valuesFromParams,
  type FormContext,
  type FormValue,
  type FormValues,
} from "../../lib/recipe-form";
import {
  recipeDescription,
  recipeSlotDescription,
  recipeSlotName,
  recipeSlotOptionLabel,
} from "../../lib/recipe-i18n";
import { BUTTON_PRIMARY, BUTTON_SECONDARY, FIELD_INPUT, FIELD_LABEL } from "../ui/Modal";
import { DurationInput, EquipmentCheckboxList } from "./recipe-fields";

export function RecipeForm({
  recipe,
  zoneId,
  equipments,
  aggregate,
  instance,
  onClose,
}: {
  recipe: RecipeInfo;
  zoneId: string;
  /** The equipments of the room (not of its sub-rooms). */
  equipments: EquipmentWithDetails[];
  aggregate: ZoneAggregatedData | undefined;
  /** Set to edit this instance; absent to create one. */
  instance?: RecipeInstance;
  onClose: () => void;
}) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const homeEquipments = useEquipments((s) => s.equipments);
  const hasButton = homeEquipments.some((eq) => eq.type === "button");
  const ctx: FormContext = {
    hasLuminosity: aggregate?.luminosity !== undefined && aggregate?.luminosity !== null,
    hasButton,
  };

  const candidates = (slot: RecipeSlotDef) => equipmentCandidates(equipments, slot);
  const [values, setValues] = useState<FormValues>(() =>
    instance
      ? valuesFromParams(
          recipe,
          instance.params,
          zoneId,
          // Kept: every equipment the home still has (one outside the room, such as a button
          // set through the API, goes back unchanged); dropped: an id the home no longer has.
          () => new Set([...homeEquipments, ...equipments].map((eq) => eq.id)),
        )
      : defaultValues(recipe, zoneId),
  );
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const set = (slot: RecipeSlotDef, value: FormValue) =>
    setValues((previous) => ({ ...previous, [slot.id]: value }));

  const submit = async (event: Event) => {
    event.preventDefault();
    const missing = missingSlots(recipe, values, ctx)[0];
    if (missing) {
      setError(t("recipes.slotRequired", { name: recipeSlotName(recipe, missing, lang) }));
      return;
    }
    setError("");
    setBusy(true);
    const params = paramsFromValues(recipe, values, zoneId, ctx);
    try {
      if (instance) await useRecipes.getState().updateInstance(instance.id, params);
      else await useRecipes.getState().createInstance(recipe.id, params);
      onClose();
    } catch (err) {
      setError(apiErrorText(err, t));
      setBusy(false);
    }
  };

  const remove = async () => {
    if (!instance || !window.confirm(t("recipes.deleteConfirm"))) return;
    setBusy(true);
    try {
      await useRecipes.getState().deleteInstance(instance.id);
      onClose();
    } catch (err) {
      setError(apiErrorText(err, t));
      setBusy(false);
    }
  };

  const field = (slot: RecipeSlotDef) => {
    const id = `recipe-slot-${slot.id}`;
    const value = values[slot.id];
    const text = typeof value === "string" ? value : "";
    if (slot.type === "equipment" && slot.list) {
      return (
        <EquipmentCheckboxList
          equipments={candidates(slot)}
          value={Array.isArray(value) ? value : []}
          onChange={(ids) => set(slot, ids)}
        />
      );
    }
    if (slot.type === "equipment") {
      return (
        <select
          id={id}
          value={text}
          onChange={(e) => set(slot, (e.target as HTMLSelectElement).value)}
          className={FIELD_INPUT}
        >
          <option value="">{t("recipes.select")}</option>
          {candidates(slot).map((eq) => (
            <option key={eq.id} value={eq.id}>
              {eq.name}
            </option>
          ))}
        </select>
      );
    }
    if (slot.type === "duration") {
      return (
        <DurationInput
          id={id}
          value={text}
          onChange={(v) => set(slot, v)}
          placeholder={
            slot.defaultValue !== undefined ? durationPlaceholder(slot.defaultValue) : undefined
          }
        />
      );
    }
    if (slot.type === "boolean") {
      return (
        <label className="flex items-center gap-2 text-small text-text cursor-pointer">
          <input
            id={id}
            type="checkbox"
            checked={value === true}
            onChange={(e) => set(slot, (e.target as HTMLInputElement).checked)}
          />
          {recipeSlotName(recipe, slot, lang)}
        </label>
      );
    }
    if (slot.type === "select") {
      return (
        <select
          id={id}
          value={text}
          onChange={(e) => set(slot, (e.target as HTMLSelectElement).value)}
          className={FIELD_INPUT}
        >
          {(slot.options ?? []).map((o) => (
            <option key={o.value} value={o.value}>
              {recipeSlotOptionLabel(recipe, slot, o.value, lang)}
            </option>
          ))}
        </select>
      );
    }
    if (slot.type === "number") {
      return (
        <input
          id={id}
          type="number"
          min={slot.constraints?.min}
          max={slot.constraints?.max}
          step="any"
          value={text}
          onInput={(e) => set(slot, (e.target as HTMLInputElement).value)}
          className={FIELD_INPUT}
        />
      );
    }
    return (
      <input
        id={id}
        type="text"
        value={text}
        onInput={(e) => set(slot, (e.target as HTMLInputElement).value)}
        className={FIELD_INPUT}
      />
    );
  };

  return (
    <form onSubmit={(e) => void submit(e)} noValidate className="space-y-3">
      <p className="text-small text-text-secondary">{recipeDescription(recipe, lang)}</p>
      {shownSlots(recipe, ctx, values).map((slot) => (
        <div key={slot.id}>
          {slot.type !== "boolean" && (
            <label htmlFor={`recipe-slot-${slot.id}`} className={FIELD_LABEL}>
              {recipeSlotName(recipe, slot, lang)}
              {slot.required && <span className="text-error"> *</span>}
            </label>
          )}
          {slot.type === "boolean" && slot.required && <span className="sr-only">*</span>}
          {field(slot)}
          {recipeSlotDescription(recipe, slot, lang) && (
            <p className="mt-1 text-caption text-text-tertiary">
              {recipeSlotDescription(recipe, slot, lang)}
            </p>
          )}
        </div>
      ))}
      {error && (
        <p role="alert" className="text-small text-error">
          {error}
        </p>
      )}
      <div className="flex items-center gap-2 pt-1">
        {instance && (
          <button
            type="button"
            onClick={() => void remove()}
            disabled={busy}
            className="px-4 py-2 text-small font-medium text-error border border-border rounded-sm hover:bg-border-light transition-colors duration-150 disabled:opacity-50"
          >
            {t("common.delete")}
          </button>
        )}
        <div className="ml-auto flex items-center gap-2">
          <button type="button" onClick={onClose} className={BUTTON_SECONDARY}>
            {t("common.cancel")}
          </button>
          <button type="submit" disabled={busy} className={BUTTON_PRIMARY}>
            {instance ? t("common.save") : t("common.create")}
          </button>
        </div>
      </div>
    </form>
  );
}

/** The default `"10m"` of a duration slot, as the minutes shown in the empty field. */
function durationPlaceholder(defaultValue: unknown): string | undefined {
  const text = typeof defaultValue === "string" ? defaultValue : "";
  const match = /^(\d+)m$/.exec(text);
  return match ? match[1] : undefined;
}
