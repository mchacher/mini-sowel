// Ported from Sowel ui/src/components/devices/DeviceList.tsx (Sowel commit 5da84dc1, UI 1.87.0),
// cut (spec 019, R3): the table (status, name with its editor and the "New" mark, manufacturer,
// model, the Equipment column, the actions); no sort buttons, source, battery, LQI or last-seen
// columns; the rows come in the order given. A click on a row opens the device's page
// (`#/devices/<id>`, spec 030, R2), except on the name editor, the buttons and the links (the
// equipment and room links of the Equipment column go to their own page). Added: delete with
// confirmation (a refusal shows its text under the row for 5 s, a 404 removes the row). No
// equipment is made from a row (spec 037, R1; Sowel's `EquipmentCell` shows "—" and no action).
import { useEffect, useMemo, useRef, useState } from "react";
import { Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { apiErrorStatus, apiErrorText } from "../../lib/api-error";
import { useDevices } from "../../store/useDevices";
import { useEquipments } from "../../store/useEquipments";
import { useZones } from "../../store/useZones";
import type { Device } from "../../types";
import { hashForDetail } from "../layout/admin-nav-items";
import { DeviceNameEditor } from "./DeviceNameEditor";
import { EquipmentCell } from "./EquipmentCell";
import { deviceEquipmentMap, type DeviceEquipment } from "./deviceEquipments";

/** How long a refusal stays under the row. */
const ERROR_MS = 5000;

const DOT: Record<Device["status"], string> = {
  online: "bg-success",
  offline: "bg-error",
  unknown: "bg-text-tertiary",
};

function Row({
  device,
  isNew,
  equipments,
}: {
  device: Device;
  isNew: boolean;
  equipments: DeviceEquipment[] | undefined;
}) {
  const { t } = useTranslation();
  const [error, setError] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );

  const fail = (err: unknown) => {
    setError(apiErrorText(err));
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => setError(null), ERROR_MS);
  };

  const remove = async () => {
    if (!window.confirm(t("devices.deleteConfirm"))) return;
    try {
      await useDevices.getState().deleteDevice(device.id);
    } catch (err) {
      if (apiErrorStatus(err) === 404) useDevices.getState().removeDevice(device.id);
      else fail(err);
    }
  };

  return (
    <>
      <tr
        tabIndex={0}
        onClick={(e) => {
          // A button or a link inside the row (the editor's stopped before) keeps its own action.
          if ((e.target as Element).closest("a, button, input")) return;
          window.location.hash = hashForDetail("devices", device.id);
        }}
        onKeyDown={(e) => {
          if (e.key !== "Enter" && e.key !== " ") return;
          if ((e.target as Element).closest("a, button, input")) return;
          e.preventDefault();
          window.location.hash = hashForDetail("devices", device.id);
        }}
        className={`${error ? "" : "border-b border-border-light"} last:border-b-0 cursor-pointer hover:bg-border-light/40`}
      >
        <td className="py-2.5 px-3 text-center">
          <span
            role="img"
            aria-label={t(`devices.status.${device.status}`)}
            title={t(`devices.status.${device.status}`)}
            className={`inline-block w-2 h-2 rounded-full ${DOT[device.status]}`}
          />
        </td>
        <td className="py-2.5 px-3">
          <div className="flex items-center gap-2 min-w-0">
            {/* The editor keeps its clicks: they never open the page. */}
            <div className="min-w-0" onClick={(e) => e.stopPropagation()}>
              <DeviceNameEditor
                name={device.name}
                onSave={(name) => useDevices.getState().updateDeviceName(device.id, name)}
              />
            </div>
            {isNew && (
              <span className="inline-flex items-center px-1.5 py-0.5 rounded-sm bg-primary-light text-caption font-medium text-primary">
                {t("devices.new")}
              </span>
            )}
          </div>
        </td>
        <td className="py-2.5 px-3 hidden lg:table-cell text-caption text-text-secondary">
          {device.manufacturer ?? "—"}
        </td>
        <td className="py-2.5 px-3 hidden md:table-cell text-caption text-text-secondary">
          {device.model ?? "—"}
        </td>
        <td className="py-2.5 px-3">
          <EquipmentCell equipments={equipments} />
        </td>
        <td className="py-2.5 px-3">
          <div className="flex items-center justify-end gap-1">
            <button
              type="button"
              onClick={() => void remove()}
              title={t("devices.delete")}
              aria-label={t("devices.delete")}
              className="p-1.5 rounded-sm text-text-tertiary hover:text-error hover:bg-error/10 transition-colors duration-150 ease-out"
            >
              <Trash2 size={16} strokeWidth={1.5} />
            </button>
          </div>
        </td>
      </tr>
      {error && (
        <tr className="border-b border-border-light last:border-b-0">
          <td />
          <td colSpan={5} className="px-3 pb-2 text-caption text-error">
            {error}
          </td>
        </tr>
      )}
    </>
  );
}

export function DeviceList({ devices, newIds = [] }: { devices: Device[]; newIds?: string[] }) {
  const { t } = useTranslation();
  const equipments = useEquipments((s) => s.equipments);
  const tree = useZones((s) => s.tree);
  // The column is live: it follows the equipments the interface already holds.
  const byDevice = useMemo(() => deviceEquipmentMap(equipments, tree), [equipments, tree]);

  return (
    <div className="bg-surface border border-border rounded-md overflow-hidden">
      <table className="w-full">
        <thead>
          <tr className="border-b border-border bg-border-light/40">
            <th className="py-2 px-3 w-[42px]" />
            <th className="py-2 px-3 text-left text-caption font-semibold uppercase tracking-[0.04em] text-text-tertiary">
              {t("devices.col.name")}
            </th>
            <th className="py-2 px-3 text-left text-caption font-semibold uppercase tracking-[0.04em] text-text-tertiary hidden lg:table-cell">
              {t("devices.col.manufacturer")}
            </th>
            <th className="py-2 px-3 text-left text-caption font-semibold uppercase tracking-[0.04em] text-text-tertiary hidden md:table-cell">
              {t("devices.col.model")}
            </th>
            <th className="py-2 px-3 text-left text-caption font-semibold uppercase tracking-[0.04em] text-text-tertiary">
              {t("devices.col.equipment")}
            </th>
            <th className="py-2 px-3" />
          </tr>
        </thead>
        <tbody>
          {devices.map((device) => (
            <Row
              key={device.id}
              device={device}
              isNew={newIds.includes(device.id)}
              equipments={byDevice.get(device.id)}
            />
          ))}
        </tbody>
      </table>
    </div>
  );
}
