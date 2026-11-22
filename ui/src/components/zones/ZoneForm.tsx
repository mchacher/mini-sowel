// Ported from Sowel ui/src/components/zones/ZoneForm.tsx (Sowel commit 5da84dc1, UI 1.87.0). Changes
// (spec 018): the modal shell is `ui/Modal` (Escape and a click outside close it); name capped at
// 64 characters (the API's bound, Sowel 100); no "None (root zone)" parent (Solite's tree has one
// root); the root's form has no parent select and sends no `parentId`; `icon` cut; the
// description is always sent on edit, so that it can be cleared.
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { ZoneWithChildren } from "../../types";
import { flattenZonesWithPath, type ZoneOption } from "../../lib/zone-path";
import { apiErrorText } from "../../lib/api-error";
import { BUTTON_PRIMARY, BUTTON_SECONDARY, FIELD_INPUT, FIELD_LABEL, Modal } from "../ui/Modal";

interface ZoneFormProps {
  /** If provided, we're editing this zone. Otherwise creating a new one. */
  initial?: { name: string; description?: string };
  /** The root has no parent: no select, no `parentId`. */
  isRoot?: boolean;
  /** Available parent zones for the dropdown, the root first. */
  parentZones: ZoneOption[];
  /** Pre-selected parent zone ID (the root when absent). */
  defaultParentId?: string | null;
  onSubmit: (data: { name: string; parentId?: string; description: string }) => Promise<void>;
  onClose: () => void;
  title: string;
}

export function ZoneForm({
  initial,
  isRoot,
  parentZones,
  defaultParentId,
  onSubmit,
  onClose,
  title,
}: ZoneFormProps) {
  const { t } = useTranslation();
  const [name, setName] = useState(initial?.name ?? "");
  const [parentId, setParentId] = useState<string>(defaultParentId ?? parentZones[0]?.id ?? "");
  const [description, setDescription] = useState(initial?.description ?? "");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async (e: { preventDefault: () => void }) => {
    e.preventDefault();
    if (!name.trim() || saving) return;

    setSaving(true);
    setError(null);
    try {
      await onSubmit({
        name: name.trim(),
        ...(isRoot ? {} : { parentId }),
        description: description.trim(),
      });
      onClose();
    } catch (err) {
      setError(apiErrorText(err, t));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal title={title} onClose={onClose}>
      <form onSubmit={handleSubmit} className="p-6 space-y-4 overflow-y-auto">
        {/* Name */}
        <div>
          <label htmlFor="zone-name" className={FIELD_LABEL}>
            {t("zones.form.name")}
          </label>
          <input
            id="zone-name"
            type="text"
            value={name}
            onChange={(e) => setName((e.target as HTMLInputElement).value)}
            placeholder={t("zones.form.namePlaceholder")}
            className={FIELD_INPUT}
            autoFocus
            maxLength={64}
          />
        </div>

        {/* Parent zone */}
        {!isRoot && (
          <div>
            <label htmlFor="zone-parent" className={FIELD_LABEL}>
              {t("zones.form.parent")}
            </label>
            <select
              id="zone-parent"
              value={parentId}
              onChange={(e) => setParentId((e.target as HTMLSelectElement).value)}
              className="w-full px-3 py-2 text-body border border-border bg-surface rounded-sm outline-none focus:border-primary transition-colors duration-150"
            >
              {parentZones.map((z) => (
                <option key={z.id} value={z.id}>
                  {z.label}
                </option>
              ))}
            </select>
          </div>
        )}

        {/* Description */}
        <div>
          <label htmlFor="zone-description" className={FIELD_LABEL}>
            {t("zones.form.description")}
            <span className="text-text-tertiary font-normal ml-1">({t("common.optional")})</span>
          </label>
          <input
            id="zone-description"
            type="text"
            value={description}
            onChange={(e) => setDescription((e.target as HTMLInputElement).value)}
            placeholder={t("zones.form.descriptionPlaceholder")}
            className={FIELD_INPUT}
            maxLength={500}
          />
        </div>

        {/* Error */}
        {error && <p className="text-small text-error">{error}</p>}

        {/* Actions */}
        <div className="flex justify-end gap-3 pt-2">
          <button type="button" onClick={onClose} className={BUTTON_SECONDARY}>
            {t("common.cancel")}
          </button>
          <button type="submit" disabled={!name.trim() || saving} className={BUTTON_PRIMARY}>
            {saving ? t("common.saving") : initial ? t("common.save") : t("common.create")}
          </button>
        </div>
      </form>
    </Modal>
  );
}

/** Flatten the zone tree for the parent dropdown, labelled with their path; `excludeId` drops that
 *  zone and its whole subtree (a zone cannot become its own descendant). Sowel's
 *  `flattenZoneTree`, unchanged. */
export function flattenZoneTree(zones: ZoneWithChildren[], excludeId?: string): ZoneOption[] {
  const excluded = new Set<string>();
  if (excludeId) {
    const collect = (z: ZoneWithChildren) => {
      excluded.add(z.id);
      z.children.forEach(collect);
    };
    const find = (nodes: ZoneWithChildren[]): ZoneWithChildren | null => {
      for (const z of nodes) {
        if (z.id === excludeId) return z;
        const found = find(z.children);
        if (found) return found;
      }
      return null;
    };
    const node = find(zones);
    if (node) collect(node);
  }
  return flattenZonesWithPath(zones).filter((z) => !excluded.has(z.id));
}
