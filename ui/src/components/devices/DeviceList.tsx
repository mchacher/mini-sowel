// Ported from Sowel ui/src/components/devices/DeviceList.tsx (Sowel commit 5da84dc1, UI 1.87.0),
// cut (spec 019, R3): the table (status, name with its editor and the "New" mark, manufacturer,
// model, the Equipment column, the actions); no sort buttons, source, battery, LQI or last-seen
// columns, no row link (Solite has no device page); the rows come in the order given. Added:
// "Create equipment" on a row with no equipment, which reads the device and opens the equipment
// form with it ticked; delete with confirmation (a refusal shows its text under the row for 5 s,
// a 404 removes the row).
import { useEffect, useMemo, useRef, useState } from "react";
import { Plus, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { getDevice, type DeviceWithData } from "../../api";
import { apiErrorStatus, apiErrorText } from "../../lib/api-error";
import { useDevices } from "../../store/useDevices";
import { useEquipments } from "../../store/useEquipments";
import { useZones } from "../../store/useZones";
import type { Device } from "../../types";
import { NewEquipmentForm } from "../equipments/NewEquipmentForm";
import { deduceType } from "../equipments/equipment-type-meta";
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
  onCreate,
}: {
  device: Device;
  isNew: boolean;
  equipments: DeviceEquipment[] | undefined;
  onCreate: (device: DeviceWithData) => void;
}) {
  const { t } = useTranslation();
  const [error, setError] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  // A second click before the read is answered must not send a second request.
  const reading = useRef(false);

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

  const create = async () => {
    if (reading.current) return;
    reading.current = true;
    try {
      onCreate(await getDevice(device.id));
    } catch (err) {
      // Deleted meanwhile: the row goes; any other refusal shows its text.
      if (apiErrorStatus(err) === 404) useDevices.getState().removeDevice(device.id);
      fail(err);
    } finally {
      reading.current = false;
    }
  };

  const none = !equipments || equipments.length === 0;

  return (
    <>
      <tr className={`${error ? "" : "border-b border-border-light"} last:border-b-0`}>
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
            <DeviceNameEditor
              name={device.name}
              onSave={(name) => useDevices.getState().updateDeviceName(device.id, name)}
            />
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
            {none && (
              <button
                type="button"
                onClick={() => void create()}
                className="inline-flex items-center gap-1 px-2 py-1 rounded-sm text-caption font-medium text-primary hover:bg-primary-light"
              >
                <Plus size={14} strokeWidth={1.5} />
                {t("devices.createEquipment")}
              </button>
            )}
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
  const [creating, setCreating] = useState<DeviceWithData | null>(null);
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
              onCreate={setCreating}
            />
          ))}
        </tbody>
      </table>
      {creating && (
        <NewEquipmentForm
          initialType={deduceType(creating) ?? "light_onoff"}
          typeIsGuess={deduceType(creating) === null}
          initialName={creating.name}
          initialDeviceIds={[creating.id]}
          onClose={() => setCreating(null)}
        />
      )}
    </div>
  );
}
