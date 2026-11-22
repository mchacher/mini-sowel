// Ported from Sowel ui/src/pages/EquipmentsPage.tsx (Sowel commit 5da84dc1, UI 1.87.0). Changes (spec
// 018): no auth, no `useWsSubscription`, no search, no singleton types; the header is written out;
// Sowel's `EquipmentCard` (its detail page, 1288 lines, is out of scope) is a row: name, type,
// status badge, number of devices, "Edit" and "Delete"; the form sends the plan's requests
// (decision D30).
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Box, Loader2, Pencil, Plus, Trash2 } from "lucide-react";
import type { EquipmentWithDetails } from "../../types";
import { useEquipments } from "../../store/useEquipments";
import { useZones } from "../../store/useZones";
import { apiErrorStatus, apiErrorText } from "../../lib/api-error";
import {
  flattenZonesWithPath,
  groupEquipmentsByZone,
  ZONE_PATH_SEPARATOR,
} from "../../lib/zone-path";
import { hashForZone } from "../../zones";
import { EquipmentStatusBadge } from "./EquipmentStatusBadge";
import { EquipmentForm } from "./EquipmentForm";
import { NewEquipmentForm } from "./NewEquipmentForm";
import { TYPE_ICONS, TYPE_LABELS } from "./equipment-type-meta";

/** How long a refusal stays under the row. */
const ERROR_MS = 5000;

export function EquipmentsPage() {
  const { t } = useTranslation();
  const equipments = useEquipments((s) => s.equipments);
  const loading = useEquipments((s) => s.loading);
  const error = useEquipments((s) => s.error);
  const tree = useZones((s) => s.tree);
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<EquipmentWithDetails | null>(null);

  // Group by zone id — not by name, which merges homonym zones (spec 142).
  const zoneOptions = useMemo(() => flattenZonesWithPath(tree), [tree]);
  const byZone = groupEquipmentsByZone(equipments, zoneOptions);

  return (
    <div>
      <div className="flex flex-wrap items-start justify-between gap-3 mb-4">
        <div>
          <h1 className="text-display font-semibold text-text">{t("equipments.title")}</h1>
          {equipments.length > 0 && (
            <p className="text-small text-text-secondary">
              {t("equipments.count", { count: equipments.length })}
            </p>
          )}
        </div>
        <button
          onClick={() => setCreating(true)}
          className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-sm bg-primary text-white text-small font-medium hover:bg-primary-hover"
        >
          <Plus size={16} strokeWidth={1.5} />
          {t("equipments.addEquipment")}
        </button>
      </div>

      {/* Content */}
      {loading && equipments.length === 0 ? (
        <div className="flex items-center justify-center py-20">
          <Loader2 size={24} className="animate-spin text-text-tertiary" />
        </div>
      ) : error && equipments.length === 0 ? (
        <ErrorState error={error} />
      ) : equipments.length === 0 ? (
        <EmptyState onAdd={() => setCreating(true)} />
      ) : (
        <div className="space-y-6">
          {byZone.map(({ zone, equipments: zoneEquipments }) => (
            <div key={zone?.id ?? "orphans"}>
              <ZoneHeading chain={zone?.chain} />
              <div className="space-y-1.5">
                {zoneEquipments.map((eq) => (
                  <EquipmentListRow key={eq.id} equipment={eq} onEdit={() => setEditing(eq)} />
                ))}
              </div>
            </div>
          ))}
        </div>
      )}

      {creating && <NewEquipmentForm onClose={() => setCreating(false)} />}
      {editing && (
        <EquipmentForm
          title={t("equipments.editEquipment")}
          initial={{ name: editing.name, type: editing.type, zoneId: editing.zoneId }}
          zones={tree}
          onSubmit={async (data) => {
            try {
              await useEquipments
                .getState()
                .updateEquipment(editing.id, { name: data.name, zoneId: data.zoneId });
            } catch (err) {
              // Deleted meanwhile by another browser: the list is read again.
              if (apiErrorStatus(err) === 404) void useEquipments.getState().fetchEquipments();
              throw err;
            }
          }}
          onClose={() => setEditing(null)}
        />
      )}
    </div>
  );
}

function EquipmentListRow({
  equipment,
  onEdit,
}: {
  equipment: EquipmentWithDetails;
  onEdit: () => void;
}) {
  const { t } = useTranslation();
  const [error, setError] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const deviceCount = new Set(
    [...equipment.dataBindings, ...equipment.orderBindings].map((b) => b.deviceId),
  ).size;
  const label = TYPE_LABELS[equipment.type];

  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );

  const remove = async () => {
    if (!window.confirm(t("equipments.deleteConfirm", { name: equipment.name }))) return;
    try {
      await useEquipments.getState().deleteEquipment(equipment.id);
    } catch (err) {
      // Deleted meanwhile by another browser: the list is read again.
      if (apiErrorStatus(err) === 404) void useEquipments.getState().fetchEquipments();
      setError(apiErrorText(err, t));
      if (timer.current) clearTimeout(timer.current);
      timer.current = setTimeout(() => setError(null), ERROR_MS);
    }
  };

  return (
    <div
      data-testid="equipment-row"
      className={`bg-surface border border-border rounded-md px-4 py-3 ${equipment.enabled ? "" : "opacity-50"}`}
    >
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <span className="flex-shrink-0 text-text-secondary">{TYPE_ICONS[equipment.type]}</span>
        <a
          href={hashForZone(equipment.zoneId)}
          onClick={(e) => {
            e.preventDefault();
            window.location.hash = hashForZone(equipment.zoneId);
          }}
          className="flex-1 basis-40 min-w-0 text-body font-medium text-text truncate hover:text-primary"
        >
          {equipment.name}
        </a>
        <span className="text-caption text-text-secondary">
          {label ? t(label) : equipment.type}
        </span>
        <EquipmentStatusBadge status={equipment.status} reason={equipment.statusReason} size="sm" />
        <span className="text-caption text-text-secondary">
          {t("equipments.deviceCount", { count: deviceCount })}
        </span>
        <div className="flex items-center gap-0.5">
          <button
            onClick={onEdit}
            title={t("common.edit")}
            aria-label={t("common.edit")}
            className="p-1.5 rounded-sm text-text-tertiary hover:text-primary hover:bg-primary-light transition-colors duration-150"
          >
            <Pencil size={16} strokeWidth={1.5} />
          </button>
          <button
            onClick={() => void remove()}
            title={t("common.delete")}
            aria-label={t("common.delete")}
            className="p-1.5 rounded-sm text-text-tertiary hover:text-error hover:bg-error/10 transition-colors duration-150 ease-out"
          >
            <Trash2 size={16} strokeWidth={1.5} />
          </button>
        </div>
      </div>
      {error && <p className="text-caption text-error mt-1">{error}</p>}
    </div>
  );
}

/**
 * Group heading spelling out where the zone actually is: ancestors muted, the
 * zone's own name carrying the emphasis (Sowel's, unchanged).
 */
function ZoneHeading({ chain }: { chain?: string[] }) {
  const { t } = useTranslation();
  const ancestors = chain?.slice(0, -1) ?? [];
  const name = chain?.[chain.length - 1] ?? t("dashboard.unknownZone");

  return (
    <h3 className="text-lead font-semibold text-text mb-2">
      {ancestors.length > 0 && (
        <span className="font-normal text-text-tertiary">
          {ancestors.join(ZONE_PATH_SEPARATOR)}
          {ZONE_PATH_SEPARATOR}
        </span>
      )}
      <span className="whitespace-nowrap">{name}</span>
    </h3>
  );
}

function EmptyState({ onAdd }: { onAdd: () => void }) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-col items-center justify-center py-20 text-center">
      <div className="w-16 h-16 rounded-full bg-primary-light flex items-center justify-center mb-4">
        <Box size={28} strokeWidth={1.5} className="text-primary" />
      </div>
      <h3 className="text-lead font-medium text-text mb-1">{t("equipments.empty.title")}</h3>
      <p className="text-small text-text-secondary max-w-[320px] mb-4">
        {t("equipments.empty.message")}
      </p>
      <button
        onClick={onAdd}
        className="px-4 py-2 bg-primary-fill text-white text-small font-medium rounded-sm hover:bg-primary-fill-hover transition-colors duration-150 ease-out"
      >
        {t("equipments.createEquipment")}
      </button>
    </div>
  );
}

function ErrorState({ error }: { error: string }) {
  const { t } = useTranslation();

  return (
    <div className="flex flex-col items-center justify-center py-20 text-center">
      <div className="w-16 h-16 rounded-full bg-error/10 flex items-center justify-center mb-4">
        <Box size={28} strokeWidth={1.5} className="text-error" />
      </div>
      <h3 className="text-lead font-medium text-text mb-1">{t("equipments.error.title")}</h3>
      <p className="text-small text-text-secondary max-w-[320px] mb-4">{error}</p>
      <button
        onClick={() => void useEquipments.getState().fetchEquipments()}
        className="px-4 py-2 bg-primary-fill text-white text-small font-medium rounded-sm hover:bg-primary-fill-hover transition-colors duration-150 ease-out"
      >
        {t("common.retry")}
      </button>
    </div>
  );
}
