// Ported from Sowel ui/src/components/equipments/AddBindingModal.tsx (Sowel commit 5da84dc1, UI
// 1.87.0). Changes (spec 030, R4): on Solite's `Modal` (it closes on Escape and on a click
// outside); the suggested alias is `resolveAlias` with the creation form's category names; checked
// before sending: an empty alias (Add disabled), an alias already used in that kind, an alias over
// 32 characters counted as the host counts them (Unicode characters); the host's refusal shows its
// text and the dialog stays open; a failed read of the devices says so.
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Radio, Loader2, ChevronRight } from "lucide-react";
import { getDevices, getDevice, type DeviceWithData } from "../../api";
import type { DeviceData, DeviceOrder } from "../../types";
import { formErrorText } from "../../lib/form-error";
import { BUTTON_PRIMARY, BUTTON_SECONDARY, Modal } from "../ui/Modal";
import {
  DATA_CATEGORY_ALIASES,
  MAX_ALIAS,
  ORDER_CATEGORY_ALIASES,
  resolveAlias,
} from "./bindingUtils";

type BindingMode = "data" | "order";

interface AddBindingModalProps {
  mode: BindingMode;
  onAdd: (params: { id: string; alias: string }) => Promise<void>;
  onClose: () => void;
  /** Aliases already used in that kind: prevents duplicates. */
  existingAliases: string[];
  /** Equipment type: used to suggest Sowel's aliases. */
  equipmentType?: string;
}

export function AddBindingModal({
  mode,
  onAdd,
  onClose,
  existingAliases,
  equipmentType,
}: AddBindingModalProps) {
  const { t } = useTranslation();
  const [devices, setDevices] = useState<DeviceWithData[]>([]);
  const [loading, setLoading] = useState(true);

  // Step 1: pick a device. Step 2: pick a data or an order, and its alias.
  const [selectedDevice, setSelectedDevice] = useState<DeviceWithData | null>(null);
  const [loadingDevice, setLoadingDevice] = useState(false);

  const [selectedItemId, setSelectedItemId] = useState<string | null>(null);
  const [alias, setAlias] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    getDevices()
      .then((all) => {
        if (!alive) return;
        setDevices(all);
        setLoading(false);
      })
      .catch((err) => {
        if (!alive) return;
        setError(formErrorText(err, t));
        setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, []);

  const handleSelectDevice = async (deviceId: string) => {
    setLoadingDevice(true);
    setError(null);
    try {
      const detail = await getDevice(deviceId);
      setSelectedDevice(detail);
      setSelectedItemId(null);
      setAlias("");
    } catch (err) {
      setError(formErrorText(err, t));
    } finally {
      setLoadingDevice(false);
    }
  };

  const handleSelectItem = (item: DeviceData | DeviceOrder) => {
    setSelectedItemId(item.id);
    setAlias(
      equipmentType
        ? resolveAlias(
            item.key,
            equipmentType,
            mode === "data" ? DATA_CATEGORY_ALIASES : ORDER_CATEGORY_ALIASES,
            item.category,
            item.type,
          )
        : item.key,
    );
    setError(null);
  };

  const handleSubmit = async () => {
    const trimmed = alias.trim();
    if (!selectedItemId || !trimmed || submitting) return;

    if (existingAliases.includes(trimmed)) {
      setError(t("binding.aliasDuplicate", { alias: trimmed }));
      return;
    }
    // The host counts Unicode characters, not UTF-16 units.
    if ([...trimmed].length > MAX_ALIAS) {
      setError(t("binding.aliasTooLong"));
      return;
    }

    setSubmitting(true);
    setError(null);
    try {
      await onAdd({ id: selectedItemId, alias: trimmed });
      onClose();
    } catch (err) {
      setError(formErrorText(err, t));
      setSubmitting(false);
    }
  };

  const items: (DeviceData | DeviceOrder)[] = selectedDevice
    ? mode === "data"
      ? (selectedDevice.data ?? [])
      : (selectedDevice.orders ?? [])
    : [];

  return (
    <Modal
      title={mode === "data" ? t("binding.addData") : t("binding.addOrder")}
      onClose={onClose}
      maxWidth="max-w-[480px]"
    >
      <div className="flex flex-col flex-1 overflow-hidden">
        <div className="flex-1 overflow-y-auto px-5 py-4 space-y-4">
          {loading ? (
            <div className="flex justify-center py-8">
              <Loader2 size={20} className="animate-spin text-text-tertiary" />
            </div>
          ) : !selectedDevice ? (
            /* Step 1: pick a device */
            <>
              <p className="text-small text-text-secondary">
                {mode === "data" ? t("binding.selectDevice") : t("binding.selectDeviceOrder")}
              </p>
              {loadingDevice && (
                <div className="flex justify-center py-2">
                  <Loader2 size={20} className="animate-spin text-text-tertiary" />
                </div>
              )}
              {devices.length === 0 ? (
                // A failed read is not "no device": the error below says it.
                error ? null : (
                  <p className="text-small text-text-tertiary py-4">{t("binding.noDevices")}</p>
                )
              ) : (
                <div className="space-y-1 max-h-[400px] overflow-y-auto">
                  {devices.map((device) => (
                    <button
                      type="button"
                      key={device.id}
                      onClick={() => void handleSelectDevice(device.id)}
                      className="flex items-center gap-3 w-full px-3 py-2.5 rounded-sm bg-border-light/50 hover:bg-border-light transition-colors duration-150 text-left"
                    >
                      <Radio
                        size={16}
                        strokeWidth={1.5}
                        className="text-text-tertiary flex-shrink-0"
                      />
                      <div className="flex-1 min-w-0">
                        <span className="text-small font-medium text-text">{device.name}</span>
                        {device.manufacturer && (
                          <span className="text-micro text-text-tertiary ml-2">
                            {device.manufacturer} {device.model ?? ""}
                          </span>
                        )}
                      </div>
                      <div
                        className={`w-2 h-2 rounded-full flex-shrink-0 ${
                          device.status === "online"
                            ? "bg-success"
                            : device.status === "offline"
                              ? "bg-error"
                              : "bg-text-tertiary"
                        }`}
                      />
                      <ChevronRight size={14} strokeWidth={1.5} className="text-text-tertiary" />
                    </button>
                  ))}
                </div>
              )}
            </>
          ) : (
            /* Step 2: pick an item and its alias */
            <>
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  onClick={() => {
                    setSelectedDevice(null);
                    setSelectedItemId(null);
                    setError(null);
                  }}
                  className="text-small text-primary hover:underline"
                >
                  {t("common.back")}
                </button>
                <ChevronRight size={12} className="text-text-tertiary" />
                <span className="text-small font-medium text-text">{selectedDevice.name}</span>
              </div>

              {items.length === 0 ? (
                <p className="text-small text-text-tertiary py-4">{t("binding.nothingToBind")}</p>
              ) : (
                <div className="space-y-1 max-h-[250px] overflow-y-auto">
                  {items.map((item) => {
                    const isSelected = selectedItemId === item.id;
                    const dataItem = mode === "data" ? (item as DeviceData) : null;
                    const orderItem = mode === "order" ? (item as DeviceOrder) : null;

                    return (
                      <button
                        type="button"
                        key={item.id}
                        onClick={() => handleSelectItem(item)}
                        aria-pressed={isSelected}
                        className={`flex items-center gap-3 w-full px-3 py-2.5 rounded-sm transition-colors duration-150 text-left ${
                          isSelected
                            ? "bg-primary-light border border-primary/30"
                            : "bg-border-light/50 hover:bg-border-light border border-transparent"
                        }`}
                      >
                        <div className="flex-1 min-w-0">
                          <span className="text-small font-mono font-medium text-text">
                            {item.key}
                          </span>
                          <span className="text-micro text-text-tertiary ml-2">
                            ({dataItem ? dataItem.category : item.type})
                          </span>
                          {orderItem?.min !== undefined && orderItem?.max !== undefined && (
                            <span className="text-micro text-text-tertiary ml-1">
                              · [{orderItem.min}–{orderItem.max}]
                            </span>
                          )}
                        </div>
                        {dataItem && (
                          <span className="text-caption tabular-nums text-text-secondary">
                            {dataItem.value !== null && dataItem.value !== undefined
                              ? String(dataItem.value)
                              : "—"}
                          </span>
                        )}
                      </button>
                    );
                  })}
                </div>
              )}

              {selectedItemId && (
                <div>
                  <label
                    htmlFor="binding-alias"
                    className="block text-caption font-medium text-text-secondary mb-1"
                  >
                    {t("binding.alias")}
                  </label>
                  <input
                    id="binding-alias"
                    type="text"
                    value={alias}
                    onChange={(e) => {
                      setAlias((e.target as HTMLInputElement).value);
                      setError(null);
                    }}
                    placeholder={t("binding.aliasPlaceholder")}
                    className="w-full px-3 py-2 text-small border border-border bg-surface rounded-sm outline-none placeholder:text-text-tertiary focus:border-primary transition-colors duration-150"
                  />
                </div>
              )}
            </>
          )}

          {error && (
            <p role="alert" className="text-caption text-error">
              {error}
            </p>
          )}
        </div>

        <div className="flex justify-end gap-2 px-5 py-3 border-t border-border-light flex-shrink-0">
          <button type="button" onClick={onClose} className={BUTTON_SECONDARY}>
            {t("common.cancel")}
          </button>
          <button
            type="button"
            onClick={() => void handleSubmit()}
            disabled={!selectedItemId || !alias.trim() || submitting}
            className={BUTTON_PRIMARY}
          >
            {submitting ? t("common.adding") : t("binding.addBinding")}
          </button>
        </div>
      </div>
    </Modal>
  );
}
