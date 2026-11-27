// Ported from Sowel ui/src/components/equipments/DeviceSelector.tsx (Sowel commit 5da84dc1, UI
// 1.87.0). Cut (spec 018): the candidate channel picker, the contracts of chargers and cars,
// water-valve and pool rules, `boundDeviceIds` and the bound keys. Added: real check boxes, at most
// 64 devices chosen (the API's bound on the form's devices), the warning when the chosen devices
// give more than 8 values (only 8 are bound, spec 010 R7.5), the error text when the devices
// cannot be read. Spec 019: the candidates for lights, switches and shutters and the thermostat
// contract are ported (`isCompatible`); the devices ticked are listed first, compatible or not; the
// warning counts the binding plan; the devices read are given back to the form, and read again
// every 5 s while the form says its plan is empty.
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Radio, ChevronDown, ChevronUp, Search } from "lucide-react";
import type { EquipmentType } from "../../types";
import { getDevices, type DeviceWithData } from "../../api";
import { apiErrorText } from "../../lib/api-error";
import { MAX_BINDINGS, countPlanned } from "./bindingUtils";
import { isCompatible } from "./equipment-type-meta";

/** The API's bound on the devices of one creation (spec 018). */
export const MAX_DEVICES = 64;
/** How often the devices are read again while the plan is empty. */
export const REREAD_MS = 5000;

/** The devices ticked, in the order they were ticked, among the devices read. */
export function pickDevices(
  all: readonly DeviceWithData[],
  ids: readonly string[],
): DeviceWithData[] {
  return ids.flatMap((id) => all.find((d) => d.id === id) ?? []);
}

interface DeviceSelectorProps {
  equipmentType: EquipmentType;
  selectedDeviceIds: string[];
  onSelectionChange: (deviceIds: string[]) => void;
  /** Called with every read of the devices: the form computes its plan from the last one. */
  onDevicesRead?: (devices: DeviceWithData[]) => void;
  /** Read the devices again every `REREAD_MS` while true. */
  reread?: boolean;
}

export function DeviceSelector({
  equipmentType,
  selectedDeviceIds,
  onSelectionChange,
  onDevicesRead,
  reread = false,
}: DeviceSelectorProps) {
  const { t } = useTranslation();
  const [allDevices, setAllDevices] = useState<DeviceWithData[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [expandedDevice, setExpandedDevice] = useState<string | null>(null);
  const [showAll, setShowAll] = useState(false);
  const [filter, setFilter] = useState("");

  // Read when the step opens.
  useEffect(() => {
    let alive = true;
    setLoading(true);
    getDevices()
      .then((all) => {
        if (!alive) return;
        setAllDevices(all);
        onDevicesRead?.(all);
        setLoading(false);
      })
      .catch((err) => {
        if (!alive) return;
        setLoadError(apiErrorText(err, t));
        setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, []);

  // Read again while the form's plan is empty; a failed read is tried again at the next tick.
  useEffect(() => {
    if (!reread) return;
    let alive = true;
    const timer = setInterval(() => {
      getDevices()
        .then((all) => {
          if (!alive) return;
          setAllDevices(all);
          onDevicesRead?.(all);
        })
        .catch(() => undefined);
    }, REREAD_MS);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [reread]);

  const compatible: DeviceWithData[] = allDevices.filter((d) => isCompatible(equipmentType, d));

  // The devices ticked come first, compatible or not, then the others.
  const ticked = pickDevices(allDevices, selectedDeviceIds);
  const baseDevices = showAll ? allDevices : compatible;
  const rest = baseDevices.filter((d) => !selectedDeviceIds.includes(d.id));
  const filterLower = filter.toLowerCase();
  const devices = [...ticked, ...rest].filter(
    (d) => !filterLower || d.name.toLowerCase().includes(filterLower),
  );

  const atMax = selectedDeviceIds.length >= MAX_DEVICES;
  const toggleDevice = (deviceId: string) => {
    if (selectedDeviceIds.includes(deviceId)) {
      onSelectionChange(selectedDeviceIds.filter((id) => id !== deviceId));
    } else if (!atMax) {
      onSelectionChange([...selectedDeviceIds, deviceId]);
    }
  };

  const planned = countPlanned(ticked, equipmentType);
  const bindable = Math.max(planned.data, planned.orders);

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
            const fits = compatible.includes(device);

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
                        className={`text-small font-medium ${fits ? "text-text" : "text-text-secondary"}`}
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
