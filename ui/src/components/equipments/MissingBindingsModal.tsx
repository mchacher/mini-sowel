// Ported from Sowel ui/src/components/equipments/MissingBindingsModal.tsx (Sowel commit 5da84dc1, UI
// 1.87.0). Changes (spec 030, R6): on Solite's `Modal` (Escape and a click outside close it); the
// history marks are cut (Solite has no history); the first refusal stops the writing and shows the
// host's text, and the items already written leave the list (a second "Add" sends only the rest),
// where Sowel's carries on and reports the aliases that failed; the device name comes from the
// devices read.
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Loader2 } from "lucide-react";
import { addDataBinding, addOrderBinding } from "../../api/equipments";
import type { DataBindingWithValue, OrderBindingWithDetails } from "../../types";
import { formErrorText } from "../../lib/form-error";
import { BUTTON_PRIMARY, BUTTON_SECONDARY, Modal } from "../ui/Modal";
import { computeMissingBindings, fetchDevices, type PlannedBinding } from "./bindingUtils";

interface MissingBindingsModalProps {
  equipmentId: string;
  equipmentType: string;
  dataBindings: DataBindingWithValue[];
  orderBindings: OrderBindingWithDetails[];
  onClose: () => void;
  /** Called once everything ticked was added, so the page can read again. */
  onAdded: () => void;
}

/**
 * Offers the data and orders the bound devices give, that the type uses and the equipment does not
 * bind. Everything is proposed ticked; unticking leaves one out. Nothing is written without the
 * confirmation, so what was removed on purpose never comes back by itself.
 */
export function MissingBindingsModal({
  equipmentId,
  equipmentType,
  dataBindings,
  orderBindings,
  onClose,
  onAdded,
}: MissingBindingsModalProps) {
  const { t } = useTranslation();
  const [loading, setLoading] = useState(true);
  const [missing, setMissing] = useState<PlannedBinding[]>([]);
  const [deviceNames, setDeviceNames] = useState<Record<string, string>>({});
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Computed once, when the dialog opens: the bindings arrive as a new array at each value update
  // of the equipment, and what is on offer cannot change while the dialog is open.
  useEffect(() => {
    let cancelled = false;
    const deviceIds = [
      ...new Set([...dataBindings.map((b) => b.deviceId), ...orderBindings.map((b) => b.deviceId)]),
    ];
    void fetchDevices(deviceIds).then((devices) => {
      if (cancelled) return;
      const found = computeMissingBindings(devices, equipmentType, dataBindings, orderBindings);
      setDeviceNames(Object.fromEntries(devices.map((d) => [d.id, d.name])));
      setMissing(found);
      setSelected(new Set(found.map((m) => m.sourceId)));
      setLoading(false);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const toggle = (sourceId: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(sourceId)) next.delete(sourceId);
      else next.add(sourceId);
      return next;
    });
  };

  const chosen = missing.filter((m) => selected.has(m.sourceId));

  const handleConfirm = async () => {
    if (submitting || chosen.length === 0) return;
    setSubmitting(true);
    setError(null);
    for (const item of chosen) {
      try {
        if (item.kind === "data") {
          await addDataBinding(equipmentId, { deviceDataId: item.sourceId, alias: item.alias });
        } else {
          await addOrderBinding(equipmentId, { deviceOrderId: item.sourceId, alias: item.alias });
        }
      } catch (err) {
        setError(formErrorText(err, t));
        setSubmitting(false);
        return;
      }
      // Written: it leaves the list, a second "Add" sends only the rest.
      setMissing((prev) => prev.filter((m) => m.sourceId !== item.sourceId));
    }
    onAdded();
  };

  return (
    <Modal title={t("binding.missingTitle")} onClose={onClose} maxWidth="max-w-[520px]">
      <div className="flex flex-col flex-1 overflow-hidden">
        <div className="flex-1 overflow-y-auto px-5 py-4 space-y-3">
          {loading ? (
            <div className="flex justify-center py-8">
              <Loader2 size={20} className="animate-spin text-text-tertiary" />
            </div>
          ) : missing.length === 0 ? (
            <p className="text-small text-text-secondary py-4">{t("binding.missingNone")}</p>
          ) : (
            <>
              <p className="text-small text-text-secondary">{t("binding.missingIntro")}</p>
              <div className="space-y-1">
                {missing.map((item) => (
                  <label
                    key={item.sourceId}
                    className="flex items-center gap-2.5 px-2.5 py-2 rounded-sm bg-border-light/50 hover:bg-border-light transition-colors duration-150 cursor-pointer"
                  >
                    <input
                      type="checkbox"
                      checked={selected.has(item.sourceId)}
                      onChange={() => toggle(item.sourceId)}
                      aria-label={item.alias}
                      className="accent-primary-fill"
                    />
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        <span
                          className={`font-mono text-caption font-medium ${
                            item.kind === "data" ? "text-primary" : "text-text-secondary"
                          }`}
                        >
                          {item.alias}
                        </span>
                        <span className="text-micro text-text-tertiary">
                          ({item.kind === "data" ? t("binding.kindData") : t("binding.kindOrder")})
                        </span>
                      </div>
                      <div className="text-micro text-text-tertiary truncate">
                        {deviceNames[item.deviceId] ?? item.deviceId} · {item.key}
                      </div>
                    </div>
                  </label>
                ))}
              </div>
            </>
          )}
          {error && (
            <p role="alert" className="text-small text-error">
              {error}
            </p>
          )}
        </div>

        <div className="flex items-center justify-end gap-2 px-5 py-4 border-t border-border-light flex-shrink-0">
          <button type="button" onClick={onClose} className={BUTTON_SECONDARY}>
            {t("common.cancel")}
          </button>
          <button
            type="button"
            onClick={() => void handleConfirm()}
            disabled={submitting || chosen.length === 0}
            className={BUTTON_PRIMARY}
          >
            {submitting ? (
              <Loader2 size={14} className="animate-spin" />
            ) : (
              t("binding.missingConfirm", { count: chosen.length })
            )}
          </button>
        </div>
      </div>
    </Modal>
  );
}
