// Ported from Sowel ui/src/pages/EquipmentDetailPage.tsx (Sowel commit 5da84dc1, UI 1.87.0), with
// its `DevicesSection` and `ChangeDeviceModal` (spec 030, R3 to R6, decision D50). Cut: the history
// section, the panels of the other types (shutter, thermostat, gate, energy, timed command,
// family, button actions...), the type change on edit, the order outcome line, the admin test and
// the router. The page takes the equipment from the store (`useEquipments`, live), not from its own
// read; the back link goes to the equipment's room (Sowel's fallback: Solite has no router state);
// the icon tile is the room row's. Changes: removing a binding shows a refusal for 5 s; a device
// name is a link to its page; the dialogs are on Solite's `Modal`; "Change devices" writes the
// creation form's plan and stops at the first refusal. Added: the Enabled row (`EnabledRow`).
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  ArrowLeft,
  Box,
  ChevronDown,
  ChevronRight,
  Clock,
  Cpu,
  Link2,
  Pencil,
  Plus,
  RefreshCw,
  Settings,
  Sparkles,
  Trash2,
  Unlink,
  Zap,
} from "lucide-react";
import type { DeviceWithData } from "../../api";
import { ApiError } from "../../api/client";
import type { EquipmentWithDetails } from "../../types";
import { useDevices } from "../../store/useDevices";
import { useEquipments } from "../../store/useEquipments";
import { useZones } from "../../store/useZones";
import { apiErrorStatus, apiErrorText } from "../../lib/api-error";
import { formErrorText } from "../../lib/form-error";
import { tileClasses, familyOf } from "../../lib/equipment-family";
import { flattenZonesWithPath } from "../../lib/zone-path";
import { hashForZone } from "../../zones";
import { hashForDetail } from "../layout/admin-nav-items";
import { RelativeTime } from "../RelativeTime";
import { BUTTON_PRIMARY, BUTTON_SECONDARY, Modal } from "../ui/Modal";
import { HashLink } from "../ui/HashLink";
import { AddBindingModal } from "./AddBindingModal";
import { DeviceSelector, pickDevices } from "./DeviceSelector";
import { EnabledRow } from "./EnabledRow";
import { EquipmentForm } from "./EquipmentForm";
import { EquipmentStatusBadge } from "./EquipmentStatusBadge";
import { LightControl } from "./LightControl";
import { MissingBindingsModal } from "./MissingBindingsModal";
import { SensorValues } from "./SensorValues";
import {
  applyBindingPlan,
  computeBindingPlan,
  computeMissingBindings,
  fetchDevices,
  findMainOnOffOrder,
  relayStateOn,
  removeAllBindings,
} from "./bindingUtils";
import { SOLITE_EQUIPMENT_TYPES, TYPE_ICONS, TYPE_LABELS } from "./equipment-type-meta";
import { getAllBatteryBindings, getSensorBindings } from "./sensorUtils";

/** How long a refusal stays on screen. */
const ERROR_MS = 5000;

const BACK_LINK =
  "inline-flex items-center gap-1.5 text-small text-text-secondary hover:text-text mb-4";

/** The types that share Sowel's on/off control surface. */
const SWITCHABLE: readonly string[] = ["light_onoff", "light_dimmable", "light_color", "switch"];

/** A text shown for 5 s, then gone. */
function useFlash(): [string | null, (text: string) => void] {
  const [text, setText] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );
  const show = (next: string) => {
    setText(next);
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => setText(null), ERROR_MS);
  };
  return [text, show];
}

export function EquipmentDetailPage({ equipmentId }: { equipmentId: string }) {
  const { t } = useTranslation();
  const equipments = useEquipments((s) => s.equipments);
  const loaded = useEquipments((s) => s.loaded);
  const loadError = useEquipments((s) => s.error);
  const tree = useZones((s) => s.tree);
  const equipment = equipments.find((e) => e.id === equipmentId);

  const [showEditForm, setShowEditForm] = useState(false);
  const [showAddDataBinding, setShowAddDataBinding] = useState(false);
  const [showAddOrderBinding, setShowAddOrderBinding] = useState(false);
  const [showChangeDevice, setShowChangeDevice] = useState(false);
  const [showBindings, setShowBindings] = useState(false);
  const [showMissingBindings, setShowMissingBindings] = useState(false);
  // What the plan would add to the equipment; counted only once the Bindings section is opened (one
  // read per bound device), `null` while unknown or when a read failed.
  const [missingCount, setMissingCount] = useState<number | null>(null);
  const [deleting, setDeleting] = useState(false);
  // Bindings whose removal is in flight: a second click sends nothing (a second DELETE answers 404).
  const [removing, setRemoving] = useState<ReadonlySet<string>>(new Set());
  const removingRef = useRef<Set<string>>(new Set());
  const [controlError, showControlError] = useFlash();
  const [bindingError, showBindingError] = useFlash();
  const [deleteError, showDeleteError] = useFlash();

  const room = useMemo(() => {
    const zone = equipment
      ? flattenZonesWithPath(tree).find((z) => z.id === equipment.zoneId)
      : undefined;
    return zone ? { id: zone.id, name: zone.name } : null;
  }, [tree, equipment?.zoneId]);

  // Keyed on the bindings, not on the equipment: a reading replaces the equipment object, and the
  // count must not read every device again each time a sensor reports.
  const bindingSignature = equipment
    ? JSON.stringify({
        type: equipment.type,
        data: equipment.dataBindings.map((b) => [b.id, b.deviceId, b.deviceDataId, b.alias]),
        orders: equipment.orderBindings.map((b) => [b.id, b.deviceId, b.deviceOrderId, b.alias]),
      })
    : "";

  useEffect(() => {
    if (!showBindings || !equipment) return;
    let cancelled = false;
    const { type, dataBindings, orderBindings } = equipment;
    const deviceIds = [
      ...new Set([...dataBindings.map((b) => b.deviceId), ...orderBindings.map((b) => b.deviceId)]),
    ];
    if (deviceIds.length === 0) {
      setMissingCount(0);
      return;
    }
    void fetchDevices(deviceIds).then((devices) => {
      if (cancelled) return;
      setMissingCount(computeMissingBindings(devices, type, dataBindings, orderBindings).length);
    });
    return () => {
      cancelled = true;
    };
    // `bindingSignature` is exactly the part of `equipment` this effect depends on.
  }, [showBindings, bindingSignature]);

  if (!equipment) {
    // Nothing before the first read: no flash of "not found".
    if (!loaded && !loadError) return null;
    // Our own delete went through: the page is leaving, no flash of "not found".
    if (deleting) return null;
    return (
      <div>
        <BackLink room={null} />
        <div className="flex flex-col items-center justify-center py-20 text-center">
          <div className="w-16 h-16 rounded-full bg-error/10 flex items-center justify-center mb-4">
            <Box size={28} strokeWidth={1.5} className="text-error" />
          </div>
          {loaded ? (
            <>
              <h3 className="text-lead font-medium text-text mb-1">
                {t("equipments.notFound.title")}
              </h3>
              <p className="text-small text-text-secondary">{t("equipments.notFound.message")}</p>
            </>
          ) : (
            <>
              <h3 className="text-lead font-medium text-text mb-1">
                {t("equipments.error.title")}
              </h3>
              <p className="text-small text-text-secondary max-w-[320px] mb-4">{loadError}</p>
              <button
                type="button"
                onClick={() => void useEquipments.getState().fetchEquipments()}
                className="px-4 py-2 bg-primary-fill text-white text-small font-medium rounded-sm hover:bg-primary-fill-hover transition-colors duration-150 ease-out"
              >
                {t("common.retry")}
              </button>
            </>
          )}
        </div>
      </div>
    );
  }

  // A refused order, for 5 seconds: the server's own text for a 502 or a 503, the translated text
  // when no answer came.
  const execute = async (alias: string, value: unknown): Promise<void> => {
    try {
      await useEquipments.getState().executeOrder(equipment.id, alias, value);
    } catch (err) {
      showControlError(
        err instanceof ApiError && err.status !== 0 && err.message
          ? err.message
          : apiErrorText(err),
      );
    }
  };

  const removeBinding = async (kind: "data" | "order", bindingId: string) => {
    if (removingRef.current.has(bindingId)) return;
    removingRef.current.add(bindingId);
    setRemoving(new Set(removingRef.current));
    try {
      const store = useEquipments.getState();
      if (kind === "data") await store.removeDataBinding(equipment.id, bindingId);
      else await store.removeOrderBinding(equipment.id, bindingId);
    } catch (err) {
      // Gone meanwhile (another browser): the list is read again.
      if (apiErrorStatus(err) === 404) void useEquipments.getState().fetchEquipments();
      showBindingError(formErrorText(err, t));
    } finally {
      removingRef.current.delete(bindingId);
      setRemoving(new Set(removingRef.current));
    }
  };

  const handleDelete = async () => {
    if (!window.confirm(t("equipments.deleteConfirm", { name: equipment.name }))) return;
    setDeleting(true);
    try {
      await useEquipments.getState().deleteEquipment(equipment.id);
      window.location.hash = "#/equipments";
    } catch (err) {
      if (apiErrorStatus(err) === 404) void useEquipments.getState().fetchEquipments();
      showDeleteError(formErrorText(err, t));
      setDeleting(false);
    }
  };

  const label = TYPE_LABELS[equipment.type];
  const sensorBindings = getSensorBindings(equipment.dataBindings);
  const showLight =
    SWITCHABLE.includes(equipment.type) &&
    equipment.enabled &&
    !!findMainOnOffOrder(equipment.orderBindings);

  return (
    <div>
      <BackLink room={room} />

      {/* Header: min-w-0 down the chain and wrap-anywhere, so a long name wraps on a phone. */}
      <div className="flex items-start justify-between gap-3 mb-4 sm:mb-8">
        <div className="flex items-center gap-3 min-w-0">
          <div
            aria-hidden="true"
            className={`w-10 h-10 shrink-0 rounded-md flex items-center justify-center ${tileClasses(familyOf(equipment), relayStateOn(equipment.dataBindings))}`}
          >
            {TYPE_ICONS[equipment.type] ?? <Box size={18} strokeWidth={1.5} />}
          </div>
          <div className="min-w-0">
            <div className="flex items-center gap-3 flex-wrap">
              <h1 className="text-display font-semibold text-text min-w-0 wrap-anywhere">
                {equipment.name}
              </h1>
              <EquipmentStatusBadge
                status={equipment.status}
                reason={equipment.statusReason}
                size="md"
              />
            </div>
            <p className="text-small text-text-secondary wrap-anywhere">
              {label ? t(label) : equipment.type}
              {room && ` · ${room.name}`}
              {!equipment.enabled && (
                <span className="text-warning-ink ml-2">{t("common.disabled")}</span>
              )}
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2 shrink-0">
          <button
            type="button"
            onClick={() => setShowEditForm(true)}
            className="flex items-center gap-1.5 p-2 sm:px-3 sm:py-2 text-small font-medium text-text-secondary border border-border rounded-sm hover:bg-border-light transition-colors duration-150 cursor-pointer"
            title={t("common.edit")}
          >
            <Pencil size={14} strokeWidth={1.5} />
            <span className="hidden sm:inline">{t("common.edit")}</span>
          </button>
        </div>
      </div>

      {/* Controls: only while enabled. */}
      {showLight && (
        <div className="card p-4 mb-6">
          <h3 className="text-body font-semibold text-text mb-3">{t("equipments.controls")}</h3>
          <div className="flex flex-wrap items-center gap-3">
            <LightControl equipment={equipment} onExecuteOrder={execute} />
          </div>
          {controlError && (
            <p role="alert" className="text-caption text-error mt-2">
              {controlError}
            </p>
          )}
        </div>
      )}

      {/* Values: read, not sent, so also while disabled. */}
      {sensorBindings.length > 0 && (
        <div className="card p-4 mb-6">
          <h3 className="text-body font-semibold text-text mb-3">{t("equipments.values")}</h3>
          <SensorValues
            sensorBindings={sensorBindings}
            batteryBindings={getAllBatteryBindings(equipment.dataBindings)}
            layout="wrap"
          />
        </div>
      )}

      {/* Configuration: every setting is a collapsed row with a one-line summary. */}
      <div className="card mb-6 divide-y divide-border-light">
        <div className="flex items-center gap-2 p-4">
          <Settings size={16} strokeWidth={1.5} className="text-text-tertiary" />
          <h3 className="text-body font-semibold text-text">{t("equipments.configuration")}</h3>
        </div>

        <EnabledRow equipmentId={equipment.id} enabled={equipment.enabled} />

        <DevicesSection equipment={equipment} onChangeDevice={() => setShowChangeDevice(true)} />

        {/* Bindings */}
        <div>
          <button
            type="button"
            onClick={() => setShowBindings(!showBindings)}
            aria-expanded={showBindings}
            className="flex items-center gap-2 w-full p-4 text-left cursor-pointer"
          >
            {showBindings ? (
              <ChevronDown size={14} strokeWidth={1.5} className="text-text-tertiary" />
            ) : (
              <ChevronRight size={14} strokeWidth={1.5} className="text-text-tertiary" />
            )}
            <Link2 size={14} strokeWidth={1.5} className="text-text-tertiary" />
            <span className="text-small font-medium text-text-secondary">
              {t("equipments.bindings")}
            </span>
            <span className="text-micro text-text-tertiary">
              {t("equipments.bindingCount", {
                count: equipment.dataBindings.length + equipment.orderBindings.length,
              })}
            </span>
          </button>

          {showBindings && (
            <div className="px-4 pb-4 space-y-4">
              {/* A device can start giving data long after the equipment was bound: the missing
                  ones are offered additively, nothing is rebuilt. */}
              {missingCount !== null && missingCount > 0 && (
                <div className="flex items-center gap-2 px-3 py-2.5 rounded-sm bg-primary-light border border-primary/20">
                  <Sparkles size={14} strokeWidth={1.5} className="text-primary flex-shrink-0" />
                  <span className="flex-1 text-caption text-text-secondary">
                    {t("binding.missingAvailable", { count: missingCount })}
                  </span>
                  <button
                    type="button"
                    onClick={() => setShowMissingBindings(true)}
                    className="px-2 py-1 text-micro font-medium text-white bg-primary-fill hover:bg-primary-fill-hover rounded-sm transition-colors duration-150 flex-shrink-0"
                  >
                    {t("binding.missingReview")}
                  </button>
                </div>
              )}

              {/* Data bindings */}
              <div>
                <div className="flex items-center justify-between mb-2">
                  <h4 className="text-caption font-medium text-text-tertiary flex items-center gap-1.5">
                    <Link2 size={13} strokeWidth={1.5} />
                    {t("equipments.dataBindings")}
                  </h4>
                  <AddButton onClick={() => setShowAddDataBinding(true)} />
                </div>
                {equipment.dataBindings.length === 0 ? (
                  <p className="text-caption text-text-tertiary">{t("common.none")}</p>
                ) : (
                  <div className="space-y-1">
                    {equipment.dataBindings.map((binding) => (
                      <div
                        key={binding.id}
                        data-testid="data-binding"
                        className="flex items-center gap-2 px-2.5 py-1.5 rounded-sm bg-border-light/50 text-caption group"
                      >
                        <span className="font-mono font-medium text-primary">{binding.alias}</span>
                        <span className="text-text-tertiary">({binding.category})</span>
                        <span className="text-text-tertiary">
                          · {binding.deviceName} · {binding.key}
                        </span>
                        <span className="ml-auto tabular-nums text-text">
                          {binding.value !== null && binding.value !== undefined
                            ? String(binding.value)
                            : "—"}
                          {binding.unit && (
                            <span className="text-text-tertiary ml-0.5">{binding.unit}</span>
                          )}
                        </span>
                        <RemoveButton
                          onClick={() => void removeBinding("data", binding.id)}
                          disabled={removing.has(binding.id)}
                        />
                      </div>
                    ))}
                  </div>
                )}
              </div>

              {/* Order bindings */}
              <div>
                <div className="flex items-center justify-between mb-2">
                  <h4 className="text-caption font-medium text-text-tertiary flex items-center gap-1.5">
                    <Zap size={13} strokeWidth={1.5} />
                    {t("equipments.orderBindings")}
                  </h4>
                  <AddButton onClick={() => setShowAddOrderBinding(true)} />
                </div>
                {equipment.orderBindings.length === 0 ? (
                  <p className="text-caption text-text-tertiary">{t("common.none")}</p>
                ) : (
                  <div className="space-y-1">
                    {equipment.orderBindings.map((binding) => (
                      <div
                        key={binding.id}
                        data-testid="order-binding"
                        className="flex items-center gap-2 px-2.5 py-1.5 rounded-sm bg-border-light/50 text-caption group"
                      >
                        <span className="font-mono font-medium text-text-secondary">
                          {binding.alias}
                        </span>
                        <span className="text-text-tertiary">({binding.type})</span>
                        <span className="text-text-tertiary">
                          · {binding.deviceName} · {binding.key}
                        </span>
                        <RemoveButton
                          className="ml-auto"
                          onClick={() => void removeBinding("order", binding.id)}
                          disabled={removing.has(binding.id)}
                        />
                      </div>
                    ))}
                  </div>
                )}
              </div>

              {bindingError && (
                <p role="alert" className="text-caption text-error">
                  {bindingError}
                </p>
              )}
            </div>
          )}
        </div>

        {/* Delete */}
        <div className="p-4">
          <button
            type="button"
            onClick={() => void handleDelete()}
            disabled={deleting}
            className="inline-flex items-center gap-1.5 text-small font-medium text-error hover:underline disabled:opacity-50 cursor-pointer"
          >
            <Trash2 size={14} strokeWidth={1.5} />
            {deleting ? t("common.deleting") : t("equipments.deleteEquipment")}
          </button>
          {deleteError && (
            <p role="alert" className="text-caption text-error mt-1">
              {deleteError}
            </p>
          )}
        </div>
      </div>

      {showEditForm && (
        <EquipmentForm
          title={t("equipments.editEquipment")}
          initial={{ name: equipment.name, type: equipment.type, zoneId: equipment.zoneId }}
          zones={tree}
          onSubmit={async (data) => {
            try {
              await useEquipments
                .getState()
                .updateEquipment(equipment.id, { name: data.name, zoneId: data.zoneId });
            } catch (err) {
              // Deleted meanwhile by another browser: the store is read again.
              if (apiErrorStatus(err) === 404) void useEquipments.getState().fetchEquipments();
              throw err;
            }
          }}
          onClose={() => setShowEditForm(false)}
        />
      )}

      {showMissingBindings && (
        <MissingBindingsModal
          equipmentId={equipment.id}
          equipmentType={equipment.type}
          dataBindings={equipment.dataBindings}
          orderBindings={equipment.orderBindings}
          onClose={() => setShowMissingBindings(false)}
          onAdded={() => {
            setShowMissingBindings(false);
            setMissingCount(null);
            void useEquipments.getState().fetchEquipments();
          }}
        />
      )}

      {showAddDataBinding && (
        <AddBindingModal
          mode="data"
          existingAliases={equipment.dataBindings.map((b) => b.alias)}
          equipmentType={equipment.type}
          onAdd={({ id, alias }) =>
            useEquipments.getState().addDataBinding(equipment.id, id, alias)
          }
          onClose={() => setShowAddDataBinding(false)}
        />
      )}

      {showAddOrderBinding && (
        <AddBindingModal
          mode="order"
          existingAliases={equipment.orderBindings.map((b) => b.alias)}
          equipmentType={equipment.type}
          onAdd={({ id, alias }) =>
            useEquipments.getState().addOrderBinding(equipment.id, id, alias)
          }
          onClose={() => setShowAddOrderBinding(false)}
        />
      )}

      {showChangeDevice && (
        <ChangeDeviceModal
          equipment={equipment}
          onDone={() => setShowChangeDevice(false)}
          onClose={() => setShowChangeDevice(false)}
        />
      )}
    </div>
  );
}

/** Back to the equipment's room (Sowel's fallback), "Equipments" when the room is unknown. */
function BackLink({ room }: { room: { id: string; name: string } | null }) {
  const { t } = useTranslation();
  return (
    <HashLink href={room ? hashForZone(room.id) : "#/equipments"} className={BACK_LINK}>
      <ArrowLeft size={14} strokeWidth={1.5} />
      {room ? room.name : t("nav.equipments")}
    </HashLink>
  );
}

function AddButton({ onClick }: { onClick: () => void }) {
  const { t } = useTranslation();
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex items-center gap-1 px-2 py-1 text-micro font-medium text-primary border border-primary/30 rounded-sm hover:bg-primary-light transition-colors duration-150"
    >
      <Plus size={11} strokeWidth={1.5} />
      {t("common.add")}
    </button>
  );
}

/** The remove button of a binding row: on hover, and always on a touch screen. */
function RemoveButton({
  onClick,
  disabled = false,
  className = "",
}: {
  onClick: () => void;
  disabled?: boolean;
  className?: string;
}) {
  const { t } = useTranslation();
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      title={t("common.remove")}
      aria-label={t("common.remove")}
      className={`p-1 text-text-tertiary hover:text-error rounded-sm hover:bg-error/10 opacity-0 group-hover:opacity-100 focus-visible:opacity-100 [@media(hover:none)]:opacity-100 transition-opacity ${className}`}
    >
      <Unlink size={11} strokeWidth={1.5} />
    </button>
  );
}

/** Collapsible devices sub-section inside the Configuration card. */
export function DevicesSection({
  equipment,
  onChangeDevice,
}: {
  equipment: EquipmentWithDetails;
  onChangeDevice: () => void;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const devicesStore = useDevices((s) => s.devices);

  const deviceMap = new Map<
    string,
    { deviceId: string; deviceName: string; data: number; orders: number }
  >();
  const entryOf = (deviceId: string, deviceName: string) => {
    let entry = deviceMap.get(deviceId);
    if (!entry) {
      entry = { deviceId, deviceName, data: 0, orders: 0 };
      deviceMap.set(deviceId, entry);
    }
    return entry;
  };
  for (const db of equipment.dataBindings) entryOf(db.deviceId, db.deviceName).data++;
  for (const ob of equipment.orderBindings) entryOf(ob.deviceId, ob.deviceName).orders++;
  const devices = [...deviceMap.values()];

  return (
    <div>
      <div className="flex items-center">
        <button
          type="button"
          onClick={() => setOpen(!open)}
          aria-expanded={open}
          className="flex flex-1 min-w-0 items-center gap-2 p-4 text-left cursor-pointer"
        >
          {open ? (
            <ChevronDown size={14} strokeWidth={1.5} className="text-text-tertiary" />
          ) : (
            <ChevronRight size={14} strokeWidth={1.5} className="text-text-tertiary" />
          )}
          <Cpu size={14} strokeWidth={1.5} className="text-text-tertiary" />
          <span className="text-small font-medium text-text-secondary">
            {t("equipments.devices")}
          </span>
          <span className="text-micro text-text-tertiary">{devices.length}</span>
        </button>
        {open && (
          <button
            type="button"
            onClick={onChangeDevice}
            className="flex items-center gap-1 mr-4 px-2 py-1 text-micro font-medium text-primary border border-primary/30 rounded-sm hover:bg-primary-light transition-colors duration-150"
          >
            {devices.length > 0 ? (
              <>
                <RefreshCw size={11} strokeWidth={1.5} /> {t("equipments.changeDevice")}
              </>
            ) : (
              <>
                <Plus size={11} strokeWidth={1.5} /> {t("common.add")}
              </>
            )}
          </button>
        )}
      </div>

      {open && (
        <div className="px-4 pb-4">
          {devices.length === 0 ? (
            <p className="text-small text-text-tertiary">{t("equipments.noDevice")}</p>
          ) : (
            <div className="space-y-2">
              {devices.map((dev) => {
                const lastSeen = devicesStore[dev.deviceId]?.lastSeen ?? null;
                return (
                  <div
                    key={dev.deviceId}
                    className="flex items-center gap-3 px-3 py-2.5 rounded-sm bg-border-light/50"
                  >
                    <Cpu size={14} strokeWidth={1.5} className="text-text-tertiary flex-shrink-0" />
                    <div className="flex-1 min-w-0">
                      <HashLink
                        href={hashForDetail("devices", dev.deviceId)}
                        className="text-small font-medium text-text hover:text-primary"
                      >
                        {dev.deviceName}
                      </HashLink>
                      <div className="flex items-center gap-1 text-micro text-text-tertiary mt-0.5">
                        <Clock size={10} strokeWidth={1.5} />
                        <span>
                          {lastSeen ? <RelativeTime iso={lastSeen} /> : t("common.never")}
                        </span>
                      </div>
                    </div>
                    <span className="text-micro text-text-tertiary flex-shrink-0">
                      {dev.data > 0 && t("equipments.dataCount", { count: dev.data })}
                      {dev.data > 0 && dev.orders > 0 && " · "}
                      {dev.orders > 0 && t("equipments.orderCount", { count: dev.orders })}
                    </span>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/**
 * Chooses devices and replaces every binding of the equipment with what the type uses from them:
 * the bindings are removed one after the other, then the creation form's plan is written. The first
 * refusal stops it and shows the host's text; what was done so far stays (not atomic, as Sowel's).
 */
export function ChangeDeviceModal({
  equipment,
  onDone,
  onClose,
}: {
  equipment: EquipmentWithDetails;
  onDone: () => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [deviceIds, setDeviceIds] = useState<string[]>([]);
  const [devicesRead, setDevicesRead] = useState<DeviceWithData[] | null>(null);
  const [saving, setSaving] = useState(false);
  const busy = useRef(false);
  const [error, setError] = useState<string | null>(null);

  // One of Sowel's 23 other types has no binding rules: its plan is always empty.
  const ruled = SOLITE_EQUIPMENT_TYPES.includes(equipment.type);
  const plan = useMemo(
    () => computeBindingPlan(pickDevices(devicesRead ?? [], deviceIds), equipment.type),
    [devicesRead, deviceIds, equipment.type],
  );
  const canSave = deviceIds.length > 0 && plan.length > 0 && !saving;

  const handleConfirm = async () => {
    if (!canSave || busy.current) return;
    busy.current = true;
    setSaving(true);
    setError(null);
    try {
      await removeAllBindings(equipment.id, equipment.dataBindings, equipment.orderBindings);
      await applyBindingPlan(equipment.id, plan);
      await useEquipments.getState().fetchEquipments();
      onDone();
    } catch (err) {
      setError(formErrorText(err, t));
      // What was removed or written before the refusal shows in the bindings list.
      void useEquipments.getState().fetchEquipments();
    } finally {
      busy.current = false;
      setSaving(false);
    }
  };

  return (
    <Modal title={t("equipments.changeDevice")} onClose={onClose} maxWidth="max-w-[520px]">
      <div className="flex flex-col flex-1 overflow-hidden">
        <div className="p-6 overflow-y-auto flex-1">
          <p className="text-small text-text-secondary mb-4">
            {ruled ? t("equipments.changeDeviceDescription") : t("equipments.changeDeviceNoRules")}
          </p>
          <DeviceSelector
            equipmentType={equipment.type}
            selectedDeviceIds={deviceIds}
            onSelectionChange={setDeviceIds}
            onDevicesRead={setDevicesRead}
            reread={ruled && plan.length === 0}
          />
          {ruled && deviceIds.length > 0 && plan.length === 0 && devicesRead !== null && (
            <p className="text-small text-text-secondary mt-3">
              {t("equipments.form.nothingToBind")}
            </p>
          )}
          {error && (
            <p role="alert" className="text-small text-error mt-3">
              {error}
            </p>
          )}
        </div>

        <div className="flex justify-end gap-3 px-6 py-4 border-t border-border-light flex-shrink-0">
          <button type="button" onClick={onClose} className={BUTTON_SECONDARY}>
            {t("common.cancel")}
          </button>
          <button
            type="button"
            onClick={() => void handleConfirm()}
            disabled={!canSave}
            className={BUTTON_PRIMARY}
          >
            {saving ? t("common.saving") : t("common.save")}
          </button>
        </div>
      </div>
    </Modal>
  );
}
