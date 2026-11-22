// Solite's own file: one row of the devices page (spec 017, R5): the name editor, the maker and
// model, the status, the room select with "New room…", the delete button.
import { useEffect, useRef, useState } from "react";
import { Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { apiErrorStatus, apiErrorText } from "../../lib/api-error";
import { createZone, deleteZone } from "../../api";
import { useDevices } from "../../store/useDevices";
import { useZones } from "../../store/useZones";
import type { Device } from "../../types";
import type { FlatZone } from "../../zones";
import { DeviceNameEditor } from "./DeviceNameEditor";

/** How long a refusal stays under the row. */
const ERROR_MS = 5000;
const NEW_ROOM = "__new__";
const MAX_NAME = 64;

const DOT: Record<Device["status"], string> = {
  online: "bg-success",
  offline: "bg-error",
  unknown: "bg-text-tertiary",
};

export function DeviceRow({
  device,
  zones,
  isNew,
}: {
  device: Device;
  /** Every zone except the home's root, in tree order. */
  zones: FlatZone[];
  isNew: boolean;
}) {
  const { t } = useTranslation();
  const [error, setError] = useState<string | null>(null);
  const [naming, setNaming] = useState(false);
  const [roomName, setRoomName] = useState("");
  const [busy, setBusy] = useState(false);
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

  const place = async (zoneId: string | null) => {
    try {
      await useDevices.getState().updateDeviceZone(device.id, zoneId);
    } catch (err) {
      fail(err);
    }
  };

  const onRoom = (value: string) => {
    if (value === NEW_ROOM) {
      setNaming(true);
      return;
    }
    setNaming(false);
    void place(value === "" ? null : value);
  };

  const createRoom = async () => {
    const name = roomName.trim();
    if (name === "" || name.length > MAX_NAME || busy) return;
    setBusy(true);
    try {
      let created;
      try {
        created = await createZone({ name });
      } catch (err) {
        fail(err);
        return;
      }
      void useZones.getState().fetchZones();
      try {
        await useDevices.getState().updateDeviceZone(device.id, created.id);
        setNaming(false);
        setRoomName("");
      } catch (err) {
        // Best effort: the zone made for this device goes away again.
        await deleteZone(created.id).catch(() => undefined);
        void useZones.getState().fetchZones();
        fail(err);
      }
    } finally {
      setBusy(false);
    }
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

  const maker = [device.manufacturer, device.model].filter(Boolean).join(" ");

  return (
    <li className="bg-surface border border-border rounded-md px-4 py-3">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <div className="flex items-center gap-2 min-w-0 flex-1 basis-48">
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
        <span className="text-caption text-text-secondary truncate max-w-[220px]">{maker}</span>
        <span className="inline-flex items-center gap-1.5 text-caption text-text-secondary">
          <span className={`w-2 h-2 rounded-full ${DOT[device.status]}`} />
          {t(`devices.status.${device.status}`)}
        </span>
        <select
          aria-label={t("devices.room")}
          value={device.zoneId ?? ""}
          onChange={(e) => onRoom((e.target as HTMLSelectElement).value)}
          className="text-small text-text bg-surface border border-border rounded-sm px-2 py-1 max-w-[200px]"
        >
          <option value="">{t("devices.noRoom")}</option>
          {zones.map(({ zone, depth }) => (
            <option key={zone.id} value={zone.id}>
              {"  ".repeat(depth - 1)}
              {zone.name}
            </option>
          ))}
          <option value={NEW_ROOM}>{t("devices.newRoom")}</option>
        </select>
        <button
          onClick={() => void remove()}
          title={t("devices.delete")}
          aria-label={t("devices.delete")}
          className="p-1.5 rounded-sm text-text-tertiary hover:text-error hover:bg-error/10 transition-colors duration-150 ease-out"
        >
          <Trash2 size={16} strokeWidth={1.5} />
        </button>
      </div>
      {naming && (
        <div className="flex items-center gap-2 mt-2">
          <input
            type="text"
            autoFocus
            maxLength={MAX_NAME}
            aria-label={t("devices.newRoomName")}
            placeholder={t("devices.newRoomName")}
            value={roomName}
            onChange={(e) => setRoomName((e.target as HTMLInputElement).value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void createRoom();
              if (e.key === "Escape") setNaming(false);
            }}
            className="text-small text-text bg-surface border border-border rounded-sm px-2 py-1 min-w-[200px]"
          />
          <button
            onClick={() => void createRoom()}
            disabled={busy}
            className="px-3 py-1 rounded-sm bg-primary text-white text-small font-medium hover:bg-primary-hover disabled:opacity-50"
          >
            {t("devices.createRoom")}
          </button>
          <button
            onClick={() => setNaming(false)}
            className="px-3 py-1 rounded-sm text-small text-text-secondary hover:bg-border-light"
          >
            {t("common.cancel")}
          </button>
        </div>
      )}
      {error && <p className="text-caption text-error mt-1">{error}</p>}
    </li>
  );
}
