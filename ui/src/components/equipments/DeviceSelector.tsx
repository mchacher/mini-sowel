// Ported from Sowel ui/src/components/equipments/DeviceSelector.tsx (Sowel commit 5da84dc1, UI
// 1.87.0). Cut (spec 018): the candidate channels, contracts (thermostat, charger, car), water-valve
// and pool rules, `boundDeviceIds` and the bound keys. Added: real check boxes, at most 64
// devices chosen (the API's bound on `deviceIds`), the warning when the chosen devices give more
// than 16 values (only 16 are bound, spec 010 R7.5), the error text when the devices cannot be read.
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Radio, ChevronDown, ChevronUp, Search } from "lucide-react";
import type { EquipmentType } from "../../types";
import { getDevices, type DeviceWithData } from "../../api";
import { apiErrorText } from "../../lib/api-error";
import { EQUIPMENT_TYPE_CATEGORIES } from "./equipment-type-meta";

/** The API's bound on `deviceIds`. */
export const MAX_DEVICES = 64;
/** The API's bound on the data bindings (and on the order bindings) of one equipment. */
export const MAX_BINDINGS = 16;

/** Distinct data keys, and distinct order keys, of these devices: the most the core would bind. */
export function bindableCount(devices: DeviceWithData[]): number {
  const data = new Set<string>();
  const orders = new Set<string>();
  for (const d of devices) {
    for (const x of d.data ?? []) data.add(x.key);
    for (const o of d.orders ?? []) orders.add(o.key);
  }
  return Math.max(data.size, orders.size);
}

interface DeviceSelectorProps {
  equipmentType: EquipmentType;
  selectedDeviceIds: string[];
  onSelectionChange: (deviceIds: string[]) => void;
}

export function DeviceSelector({
  equipmentType,
  selectedDeviceIds,
  onSelectionChange,
}: DeviceSelectorProps) {
  const { t } = useTranslation();
  const [allDevices, setAllDevices] = useState<DeviceWithData[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [expandedDevice, setExpandedDevice] = useState<string | null>(null);
  const [showAll, setShowAll] = useState(false);
  const [filter, setFilter] = useState("");

  useEffect(() => {
    setLoading(true);
    getDevices()
      .then((all) => {
        setAllDevices(all);
        setLoading(false);
      })
      .catch((err) => {
        setLoadError(apiErrorText(err, t));
        setLoading(false);
      });
    // Read once, when the step opens.
  }, []);

  const categories = EQUIPMENT_TYPE_CATEGORIES[equipmentType];
  const compatible: DeviceWithData[] =
    categories && categories.length > 0
      ? allDevices.filter((device) =>
          (device.data ?? []).some((d) => categories.includes(d.category)),
        )
      : allDevices;

  const baseDevices = showAll ? allDevices : compatible;
  const filterLower = filter.toLowerCase();
  const devices = filterLower
    ? baseDevices.filter((d) => d.name.toLowerCase().includes(filterLower))
    : baseDevices;

  const atMax = selectedDeviceIds.length >= MAX_DEVICES;
  const toggleDevice = (deviceId: string) => {
    if (selectedDeviceIds.includes(deviceId)) {
      onSelectionChange(selectedDeviceIds.filter((id) => id !== deviceId));
    } else if (!atMax) {
      onSelectionChange([...selectedDeviceIds, deviceId]);
    }
  };

  const bindable = bindableCount(allDevices.filter((d) => selectedDeviceIds.includes(d.id)));

  if (loading) {
    return <p className="text-small text-text-tertiary py-4">{t("deviceSelector.loading")}</p>;
  }

  if (allDevices.length === 0) {
    return (
      <>
        {loadError && <p className="text-small text-error py-2">{loadError}</p>}
        <p className="text-small text-text-tertiary py-4">{t("deviceSelector.noDevices")}</p>
      </>
    );
  }

  return (
    <div>
      {/* Show all toggle when filter is active and hides some devices */}
      {compatible.length < allDevices.length && (
        <div className="flex items-center justify-between mb-2">
          <span className="text-caption text-text-tertiary">
            {t("deviceSelector.compatible", {
              count: compatible.length,
              total: allDevices.length,
            })}
          </span>
          <button
            type="button"
            onClick={() => setShowAll(!showAll)}
            className="text-caption text-primary hover:underline"
          >
            {showAll ? t("deviceSelector.showCompatible") : t("deviceSelector.showAll")}
          </button>
        </div>
      )}

      {/* Filter by name */}
      <div className="relative mb-2">
        <Search
          size={14}
          strokeWidth={1.5}
          className="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-tertiary"
        />
        <input
          type="text"
          value={filter}
          onChange={(e) => setFilter((e.target as HTMLInputElement).value)}
          placeholder={t("devices.filterPlaceholder")}
          aria-label={t("devices.filterPlaceholder")}
          className="w-full pl-8 pr-3 py-1.5 text-caption border border-border bg-surface rounded-sm outline-none placeholder:text-text-tertiary focus:border-primary transition-colors duration-150"
        />
      </div>

      {devices.length === 0 ? (
        <p className="text-small text-text-tertiary py-4">{t("deviceSelector.noCompatible")}</p>
      ) : (
        <div className="space-y-1 max-h-[300px] overflow-y-auto">
          {devices.map((device) => {
            const isSelected = selectedDeviceIds.includes(device.id);
            const isExpanded = expandedDevice === device.id;
            const isCompatible = compatible.includes(device);

            return (
              <div key={device.id}>
                <div
                  className={`
                    flex items-center gap-3 px-3 py-2.5 rounded-sm
                    transition-colors duration-150
                    ${isSelected ? "bg-primary-light border border-primary/30" : "bg-border-light/50 hover:bg-border-light border border-transparent"}
                  `}
                >
                  <label className="flex flex-1 items-center gap-3 min-w-0 cursor-pointer">
                    <input
                      type="checkbox"
                      checked={isSelected}
                      disabled={!isSelected && atMax}
                      onChange={() => toggleDevice(device.id)}
                      aria-label={device.name}
                      className="w-4 h-4 flex-shrink-0 accent-primary-fill"
                    />
                    <Radio
                      size={16}
                      strokeWidth={1.5}
                      className="text-text-tertiary flex-shrink-0"
                    />
                    <span className="flex-1 min-w-0">
                      <span
                        className={`text-small font-medium ${isCompatible ? "text-text" : "text-text-secondary"}`}
                      >
                        {device.name}
                      </span>
                      {device.manufacturer && (
                        <span className="text-micro text-text-tertiary ml-2">
                          {device.manufacturer} {device.model ?? ""}
                        </span>
                      )}
                    </span>
                  </label>
                  <div
                    className={`
                      w-2 h-2 rounded-full flex-shrink-0
                      ${device.status === "online" ? "bg-success" : device.status === "offline" ? "bg-error" : "bg-text-tertiary"}
                    `}
                  />
                  <button
                    type="button"
                    onClick={() => setExpandedDevice(isExpanded ? null : device.id)}
                    aria-label={t("deviceSelector.data")}
                    aria-expanded={isExpanded}
                    className="p-1 text-text-tertiary hover:text-text-secondary"
                  >
                    {isExpanded ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
                  </button>
                </div>

                {/* Device data details */}
                {isExpanded && (
                  <div className="ml-11 mt-1 mb-2 text-micro text-text-tertiary space-y-0.5">
                    {(device.data ?? []).map((d) => (
                      <div key={d.id} className="flex items-center gap-2">
                        <span className="font-mono">{d.key}</span>
                        <span className="text-text-tertiary">({d.category})</span>
                        <span className="text-text-secondary">
                          {d.value !== null && d.value !== undefined ? String(d.value) : "—"}
                        </span>
                        {d.unit && <span>{d.unit}</span>}
                      </div>
                    ))}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {atMax && <p className="text-caption text-text-secondary mt-2">{t("deviceSelector.max")}</p>}
      {bindable > MAX_BINDINGS && (
        <p className="text-caption text-warning-ink mt-2">
          {t("equipments.form.tooManyBindings", { count: bindable })}
        </p>
      )}
    </div>
  );
}
