// Ported from Sowel ui/src/pages/DeviceDetailPage.tsx (Sowel commit 5da84dc1, UI 1.87.0), cut
// (spec 030, R7): no router (`deviceId` comes from the address, the back link is `#/devices`), no
// raw expose section, no source badge, no admin test (Solite's host has one role). Kept: the
// header with the name editor and the status badge, the information bar, the data and orders
// tables, "Delete". As Sowel, the name, status and last seen come from `useDevices` (live); the
// data and orders come from the page's own store, `useDeviceDetail`, which the live events update.
// Changes: a device never seen reads "never"; the orders table has no category column (key, type,
// range); the information bar gives the manufacturer and the model; the page shows a read failure
// with "Retry" and a missing device with its back link; a refused delete shows its text for 5 s.
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowLeft, Loader2, Radio, Trash2, Zap } from "lucide-react";
import { apiErrorStatus, apiErrorText } from "../../lib/api-error";
import { useDeviceDetail } from "../../store/useDeviceDetail";
import { useDevices } from "../../store/useDevices";
import type { DeviceOrder } from "../../types";
import { RelativeTime } from "../RelativeTime";
import { HashLink } from "../ui/HashLink";
import { DeviceDataTable } from "./DeviceDataTable";
import { DeviceNameEditor } from "./DeviceNameEditor";

/** How long a refusal stays on screen. */
const ERROR_MS = 5000;

const BACK_LINK =
  "inline-flex items-center gap-1.5 text-small text-text-secondary hover:text-text transition-colors duration-150 ease-out mb-6";

export function DeviceDetailPage({ deviceId }: { deviceId: string }) {
  const { t } = useTranslation();
  const device = useDevices((s) => s.devices[deviceId]);
  const data = useDeviceDetail((s) => s.data);
  const orders = useDeviceDetail((s) => s.orders);
  const loading = useDeviceDetail((s) => s.loading);
  const notFound = useDeviceDetail((s) => s.notFound);
  const detailError = useDeviceDetail((s) => s.error);
  // The list read next to the device failed and the device is not in it: the same error and Retry.
  const listError = useDevices((s) => (s.devices[deviceId] ? null : s.error));
  const error = detailError ?? listError;
  const [deleting, setDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    void useDeviceDetail.getState().load(deviceId);
    return () => {
      useDeviceDetail.getState().clear();
      if (timer.current) clearTimeout(timer.current);
    };
  }, [deviceId]);

  const handleDelete = async () => {
    if (!window.confirm(t("devices.deleteConfirm"))) return;
    setDeleting(true);
    try {
      await useDevices.getState().deleteDevice(deviceId);
      window.location.hash = "#/devices";
    } catch (err) {
      setDeleting(false);
      if (apiErrorStatus(err) === 404) {
        // Already gone: the same as a delete that worked.
        useDevices.getState().removeDevice(deviceId);
        window.location.hash = "#/devices";
        return;
      }
      setDeleteError(apiErrorText(err));
      if (timer.current) clearTimeout(timer.current);
      timer.current = setTimeout(() => setDeleteError(null), ERROR_MS);
    }
  };

  if (error) {
    return (
      <div className="p-4 sm:p-6">
        <HashLink href="#/devices" className={BACK_LINK}>
          <ArrowLeft size={16} strokeWidth={1.5} />
          {t("devices.backToDevices")}
        </HashLink>
        <div className="flex flex-col items-center justify-center py-20 text-center">
          <p className="text-small text-text-secondary max-w-[320px] mb-4">{error}</p>
          <button
            type="button"
            onClick={() => {
              void useDeviceDetail.getState().load(deviceId);
              void useDevices.getState().fetchDevices();
            }}
            className="px-4 py-2 bg-primary-fill text-white text-small font-medium rounded-sm hover:bg-primary-fill-hover transition-colors duration-150 ease-out"
          >
            {t("common.retry")}
          </button>
        </div>
      </div>
    );
  }

  if (notFound) {
    // Our own delete went through: the page is leaving, no flash of "not found".
    if (deleting) return null;
    return (
      <div className="p-4 sm:p-6">
        <HashLink href="#/devices" className={BACK_LINK}>
          <ArrowLeft size={16} strokeWidth={1.5} />
          {t("devices.backToDevices")}
        </HashLink>
        <div className="flex flex-col items-center justify-center py-20 text-center">
          <h3 className="text-lead font-medium text-text mb-1">{t("devices.notFound.title")}</h3>
          <p className="text-small text-text-secondary">{t("devices.notFound.message")}</p>
        </div>
      </div>
    );
  }

  // The devices list is read next to the device: wait for both.
  if (loading || !device) {
    return (
      <div className="flex items-center justify-center py-20">
        <Loader2 size={24} className="animate-spin text-text-tertiary" />
      </div>
    );
  }

  return (
    <div className="p-4 sm:p-6 max-w-[960px]">
      <HashLink href="#/devices" className={BACK_LINK}>
        <ArrowLeft size={16} strokeWidth={1.5} />
        {t("devices.backToDevices")}
      </HashLink>

      <div className="flex items-start justify-between mb-8">
        <div className="flex items-start gap-4">
          <div className="flex-shrink-0 w-12 h-12 rounded-md bg-primary-light flex items-center justify-center">
            <Radio size={24} strokeWidth={1.5} className="text-primary" />
          </div>
          <div>
            <DeviceNameEditor
              name={device.name}
              onSave={(name) => useDevices.getState().updateDeviceName(device.id, name)}
            />
            <div className="flex items-center gap-3 mt-1.5">
              <StatusBadge status={device.status} />
            </div>
          </div>
        </div>
        <div className="flex flex-col items-end gap-1">
          <button
            type="button"
            onClick={() => void handleDelete()}
            disabled={deleting}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-sm text-caption font-medium text-error hover:bg-error/10 transition-colors duration-150 cursor-pointer disabled:opacity-50"
          >
            <Trash2 size={14} strokeWidth={1.5} />
            {deleting ? t("common.deleting") : t("common.delete")}
          </button>
          {deleteError && <p className="text-caption text-error">{deleteError}</p>}
        </div>
      </div>

      <div className="card flex flex-wrap gap-4 mb-8 p-4">
        {!!device.manufacturer && (
          <InfoItem label={t("devices.col.manufacturer")} value={device.manufacturer} />
        )}
        {!!device.model && <InfoItem label={t("devices.col.model")} value={device.model} />}
        <InfoItem label={t("devices.sourceId")} value={device.sourceDeviceId} mono />
        {!!device.ieeeAddress && (
          <InfoItem label={t("devices.ieeeAddress")} value={device.ieeeAddress} mono />
        )}
        <InfoItem
          label={t("devices.lastSeen")}
          value={device.lastSeen ? <RelativeTime iso={device.lastSeen} /> : t("common.never")}
        />
        <InfoItem label={t("devices.created")} value={<RelativeTime iso={device.createdAt} />} />
      </div>

      <section className="mb-8">
        <h2 className="text-title font-semibold text-text leading-[28px] mb-4">
          {t("devices.data")}
        </h2>
        <div className="card overflow-hidden">
          <DeviceDataTable data={data} />
        </div>
      </section>

      {orders.length > 0 && (
        <section className="mb-8">
          <h2 className="text-title font-semibold text-text leading-[28px] mb-4">
            {t("devices.orders")}
          </h2>
          <div className="card overflow-hidden">
            <OrdersTable orders={orders} />
          </div>
        </section>
      )}
    </div>
  );
}

function StatusBadge({ status }: { status: string }) {
  const { t } = useTranslation();
  const config = {
    online: {
      color: "bg-success/10 text-success",
      dot: "bg-success",
      label: t("devices.status.online"),
    },
    offline: {
      color: "bg-error/10 text-error",
      dot: "bg-error",
      label: t("devices.status.offline"),
    },
    unknown: {
      color: "bg-border-light text-text-tertiary",
      dot: "bg-text-tertiary",
      label: t("devices.status.unknown"),
    },
  }[status] ?? {
    color: "bg-border-light text-text-tertiary",
    dot: "bg-text-tertiary",
    label: status,
  };

  return (
    <span
      className={`inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-micro font-semibold ${config.color}`}
    >
      <span className={`w-1.5 h-1.5 rounded-full ${config.dot}`} />
      {config.label}
    </span>
  );
}

function InfoItem({
  label,
  value,
  mono,
}: {
  label: string;
  value: React.ReactNode;
  mono?: boolean;
}) {
  return (
    <div className="flex flex-col gap-0.5 min-w-[120px]">
      <span className="text-caption text-text-tertiary">{label}</span>
      <span className={`text-small text-text ${mono ? "font-mono" : ""}`}>{value}</span>
    </div>
  );
}

function OrdersTable({ orders }: { orders: DeviceOrder[] }) {
  const { t } = useTranslation();
  return (
    <table className="w-full">
      <thead>
        <tr className="border-b border-border">
          <th className="text-left py-2.5 px-3 text-caption font-semibold uppercase tracking-[0.04em] text-text-tertiary">
            {t("devices.col.key")}
          </th>
          <th className="text-left py-2.5 px-3 text-caption font-semibold uppercase tracking-[0.04em] text-text-tertiary">
            {t("common.type")}
          </th>
          <th className="text-left py-2.5 px-3 text-caption font-semibold uppercase tracking-[0.04em] text-text-tertiary">
            {t("devices.col.range")}
          </th>
        </tr>
      </thead>
      <tbody>
        {orders.map((order) => (
          <tr
            key={order.id}
            className="border-b border-border-light hover:bg-border-light/50 transition-colors duration-100"
          >
            <td className="py-2.5 px-3">
              <div className="flex items-center gap-1.5">
                <Zap size={14} strokeWidth={1.5} className="text-text-tertiary" />
                <span className="font-mono text-small text-text">{order.key}</span>
              </div>
            </td>
            <td className="py-2.5 px-3">
              <span className="text-caption text-text-secondary">{order.type}</span>
            </td>
            <td className="py-2.5 px-3">
              <OrderRange order={order} />
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function OrderRange({ order }: { order: DeviceOrder }) {
  if (order.enumValues && order.enumValues.length > 0) {
    return (
      <div className="flex flex-wrap gap-1">
        {order.enumValues.map((v) => (
          <span
            key={v}
            className="px-1.5 py-0.5 rounded-sm bg-border-light text-micro font-mono text-text-secondary"
          >
            {v}
          </span>
        ))}
      </div>
    );
  }

  if (order.min !== undefined || order.max !== undefined) {
    return (
      <span className="tabular-nums text-caption text-text-secondary">
        {order.min ?? "—"}–{order.max ?? "—"}
        {order.unit ? ` ${order.unit}` : ""}
      </span>
    );
  }

  return <span className="text-caption text-text-tertiary">—</span>;
}
