// Ported from Sowel ui/src/components/equipments/EquipmentForm.tsx (Sowel commit 5da84dc1, UI 1.87.0).
// Changes (specs 018, 027): the eight types that have Sowel's binding rules, and an equipment of
// another type of Sowel's keeps it when edited, shown as written; names capped at 64 (the API's
// bound, Sowel 100);
// edit is one step with the type locked (Sowel's type change among shutter, awning and door is
// out of scope); the pool heat pump rule, `excludeTypes` and the bound keys cut; the modal closes
// on Escape and on a click outside. Spec 019 (decision D30): the form returns the devices ticked
// and the devices read, and `NewEquipmentForm` sends the plan's requests; at least one device, and
// a device that gives nothing the type uses blocks "Create" (the plan is empty); changing the type
// keeps the devices ticked; the type, name and devices can be given (a device's "Create
// equipment"); step 2 reads the devices again every 5 s while the plan is empty (also with no device
// ticked); a type guessed without data is guessed again from each read.
import { useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { EquipmentType, ZoneWithChildren } from "../../types";
import type { DeviceWithData } from "../../api";
import { DeviceSelector, pickDevices } from "./DeviceSelector";
import { computeBindingPlan } from "./bindingUtils";
import { SOLITE_EQUIPMENT_TYPES, TYPE_LABELS, deduceType } from "./equipment-type-meta";
import { flattenZonesWithPath } from "../../lib/zone-path";
import { formErrorText } from "../../lib/form-error";
import { BUTTON_PRIMARY, BUTTON_SECONDARY, FIELD_INPUT, FIELD_LABEL, Modal } from "../ui/Modal";

interface EquipmentFormProps {
  title: string;
  /** If provided, we're editing this equipment: one step, the type locked. */
  initial?: {
    name: string;
    type: EquipmentType;
    zoneId: string;
  };
  /** The zone chosen when the form opens (a room's page). */
  defaultZoneId?: string;
  /** The type, name and devices the form opens with (a device's "Create equipment"). */
  initialType?: EquipmentType;
  /** `initialType` is the fallback, guessed without data: guess again from each read until the user picks a type. */
  typeIsGuess?: boolean;
  initialName?: string;
  initialDeviceIds?: string[];
  zones: ZoneWithChildren[];
  onSubmit: (data: {
    name: string;
    type: EquipmentType;
    zoneId: string;
    /** The devices ticked, in the order they were ticked (empty when editing). */
    deviceIds: string[];
    /** The same devices as last read, with their data and orders (empty when editing). */
    devices: DeviceWithData[];
  }) => Promise<void>;
  onClose: () => void;
}

const SELECT =
  "w-full px-3 py-2 text-body border border-border bg-surface rounded-sm outline-none focus:border-primary transition-colors duration-150";

export function EquipmentForm({
  title,
  initial,
  defaultZoneId,
  initialType,
  typeIsGuess = false,
  initialName,
  initialDeviceIds,
  zones,
  onSubmit,
  onClose,
}: EquipmentFormProps) {
  const { t } = useTranslation();
  const [step, setStep] = useState<"info" | "devices">("info");
  const [name, setName] = useState(initial?.name ?? initialName ?? "");
  const [type, setType] = useState<EquipmentType>(initial?.type ?? initialType ?? "light_onoff");
  const [zoneId, setZoneId] = useState(initial?.zoneId ?? defaultZoneId ?? "");
  const [deviceIds, setDeviceIds] = useState<string[]>(initialDeviceIds ?? []);
  // `null` until the first read of the devices.
  const [devicesRead, setDevicesRead] = useState<DeviceWithData[] | null>(null);
  // True while the type is the fallback guessed without data and the user has not chosen one.
  const guessing = useRef(typeIsGuess);
  const [saving, setSaving] = useState(false);
  // A second click before the first one's render must not send a second request.
  const busy = useRef(false);
  const [error, setError] = useState<string | null>(null);

  const flatZones = useMemo(() => flattenZonesWithPath(zones), [zones]);
  // The types offered, plus the type of the equipment being edited when it is none of them (a
  // `door`): it stays selected, its name as written, and is sent back unchanged.
  const editedType = initial?.type;
  const sortedTypes = useMemo(
    () =>
      [
        ...SOLITE_EQUIPMENT_TYPES,
        ...(editedType && !SOLITE_EQUIPMENT_TYPES.includes(editedType) ? [editedType] : []),
      ].sort((a, b) => t(TYPE_LABELS[a] ?? a).localeCompare(t(TYPE_LABELS[b] ?? b))),
    [t, editedType],
  );

  // What the type would bind from the devices ticked, as last read.
  const ticked = useMemo(() => pickDevices(devicesRead ?? [], deviceIds), [devicesRead, deviceIds]);
  const planEmpty = useMemo(() => computeBindingPlan(ticked, type).length === 0, [ticked, type]);
  const noDevice = deviceIds.length === 0;
  const blocked = !initial && (noDevice || planEmpty);

  const handleDevicesRead = (all: DeviceWithData[]) => {
    setDevicesRead(all);
    if (!guessing.current) return;
    const guessed = pickDevices(all, deviceIds)
      .map(deduceType)
      .find((x) => x !== null);
    if (guessed) {
      setType(guessed);
      guessing.current = false;
    }
  };

  const handleCreate = async () => {
    if (!name.trim() || !zoneId || busy.current || (step === "devices" && blocked)) return;

    busy.current = true;
    setSaving(true);
    setError(null);
    try {
      await onSubmit(
        initial
          ? { name: name.trim(), type, zoneId, deviceIds: [], devices: [] }
          : { name: name.trim(), type, zoneId, deviceIds, devices: ticked },
      );
      onClose();
    } catch (err) {
      setError(formErrorText(err, t));
    } finally {
      busy.current = false;
      setSaving(false);
    }
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
    }
  };

  return (
    <Modal title={title} onClose={onClose} maxWidth="max-w-[520px]">
      <div className="flex flex-col flex-1 overflow-hidden">
        <div className="p-6 space-y-4 overflow-y-auto flex-1">
          {step === "info" && (
            <>
              {/* Type */}
              <div>
                <label htmlFor="equipment-type" className={FIELD_LABEL}>
                  {t("equipments.form.type")}
                </label>
                <select
                  id="equipment-type"
                  value={type}
                  onChange={(e) => {
                    guessing.current = false;
                    setType((e.target as HTMLSelectElement).value as EquipmentType);
                  }}
                  className={SELECT}
                  disabled={initial !== undefined}
                >
                  {sortedTypes.map((value) => (
                    <option key={value} value={value}>
                      {t(TYPE_LABELS[value] ?? value)}
                    </option>
                  ))}
                </select>
              </div>

              {/* Name */}
              <div>
                <label htmlFor="equipment-name" className={FIELD_LABEL}>
                  {t("equipments.form.name")}
                </label>
                <input
                  id="equipment-name"
                  type="text"
                  value={name}
                  onChange={(e) => setName((e.target as HTMLInputElement).value)}
                  onKeyDown={handleKeyDown}
                  placeholder={t("equipments.form.namePlaceholder")}
                  className={FIELD_INPUT}
                  autoFocus
                  maxLength={64}
                />
              </div>

              {/* Zone */}
              <div>
                <label htmlFor="equipment-zone" className={FIELD_LABEL}>
                  {t("equipments.form.zone")}
                </label>
                <select
                  id="equipment-zone"
                  value={zoneId}
                  onChange={(e) => setZoneId((e.target as HTMLSelectElement).value)}
                  className={SELECT}
                >
                  <option value="">{t("equipments.form.selectZone")}</option>
                  {flatZones.map((z) => (
                    <option key={z.id} value={z.id}>
                      {z.label}
                    </option>
                  ))}
                </select>
              </div>
            </>
          )}

          {step === "devices" && (
            <>
              <p className="text-small text-text-secondary">
                {t("equipments.form.deviceInstruction")}
              </p>
              <DeviceSelector
                equipmentType={type}
                selectedDeviceIds={deviceIds}
                onSelectionChange={setDeviceIds}
                onDevicesRead={handleDevicesRead}
                reread={planEmpty}
              />
              {noDevice && (
                <p className="text-small text-text-secondary">
                  {t("equipments.form.deviceRequired")}
                </p>
              )}
              {!noDevice && planEmpty && devicesRead !== null && (
                <p className="text-small text-text-secondary">
                  {t("equipments.form.nothingToBind")}
                </p>
              )}
            </>
          )}

          {/* Error */}
          {error && <p className="text-small text-error">{error}</p>}
        </div>

        {/* Actions */}
        <div className="flex justify-between gap-3 px-6 py-4 border-t border-border-light flex-shrink-0">
          {step === "devices" ? (
            <>
              <button type="button" onClick={() => setStep("info")} className={BUTTON_SECONDARY}>
                {t("common.back")}
              </button>
              <button
                type="button"
                onClick={() => void handleCreate()}
                disabled={!name.trim() || !zoneId || saving || blocked}
                className={BUTTON_PRIMARY}
              >
                {saving ? t("common.creating") : t("common.create")}
              </button>
            </>
          ) : (
            <>
              <button type="button" onClick={onClose} className={BUTTON_SECONDARY}>
                {t("common.cancel")}
              </button>
              {initial ? (
                <button
                  type="button"
                  onClick={() => void handleCreate()}
                  disabled={!name.trim() || !zoneId || saving}
                  className={BUTTON_PRIMARY}
                >
                  {saving ? t("common.saving") : t("common.save")}
                </button>
              ) : (
                <button
                  type="button"
                  onClick={() => setStep("devices")}
                  disabled={!name.trim() || !zoneId}
                  className={BUTTON_PRIMARY}
                >
                  {t("equipments.form.nextDevices")}
                </button>
              )}
            </>
          )}
        </div>
      </div>
    </Modal>
  );
}
