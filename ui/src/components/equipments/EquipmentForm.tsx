// Ported from Sowel ui/src/components/equipments/EquipmentForm.tsx (Sowel commit 5da84dc1, UI 1.87.0).
// Changes (spec 018): decision D3's seven types; names capped at 64 (the API's bound, Sowel 100);
// edit is one step with the type locked (Sowel's type change among shutter, awning and door is
// out of scope); the form returns `deviceIds` only (the core binds them in the one `POST`,
// decision D29, instead of Sowel's `candidateByDevice` and one request per binding); the pool
// heat pump rule, `excludeTypes` and the bound keys cut; the modal closes on Escape and on a click
// outside.
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { EquipmentType, ZoneWithChildren } from "../../types";
import { DeviceSelector } from "./DeviceSelector";
import { SOLITE_EQUIPMENT_TYPES, TYPE_LABELS } from "./equipment-type-meta";
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
  zones: ZoneWithChildren[];
  onSubmit: (data: {
    name: string;
    type: EquipmentType;
    zoneId: string;
    deviceIds: string[];
  }) => Promise<void>;
  onClose: () => void;
}

const SELECT =
  "w-full px-3 py-2 text-body border border-border bg-surface rounded-sm outline-none focus:border-primary transition-colors duration-150";

export function EquipmentForm({
  title,
  initial,
  defaultZoneId,
  zones,
  onSubmit,
  onClose,
}: EquipmentFormProps) {
  const { t } = useTranslation();
  const [step, setStep] = useState<"info" | "devices">("info");
  const [name, setName] = useState(initial?.name ?? "");
  const [type, setType] = useState<EquipmentType>(initial?.type ?? "light_onoff");
  const [zoneId, setZoneId] = useState(initial?.zoneId ?? defaultZoneId ?? "");
  const [deviceIds, setDeviceIds] = useState<string[]>([]);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const flatZones = useMemo(() => flattenZonesWithPath(zones), [zones]);
  const sortedTypes = useMemo(
    () =>
      [...SOLITE_EQUIPMENT_TYPES].sort((a, b) =>
        t(TYPE_LABELS[a] ?? a).localeCompare(t(TYPE_LABELS[b] ?? b)),
      ),
    [t],
  );

  const handleCreate = async () => {
    if (!name.trim() || !zoneId || saving) return;

    setSaving(true);
    setError(null);
    try {
      await onSubmit({ name: name.trim(), type, zoneId, deviceIds });
      onClose();
    } catch (err) {
      setError(formErrorText(err, t));
    } finally {
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
                    setType((e.target as HTMLSelectElement).value as EquipmentType);
                    setDeviceIds([]);
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
              />
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
                disabled={!name.trim() || !zoneId || saving}
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
