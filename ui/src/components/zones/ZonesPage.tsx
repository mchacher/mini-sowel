// Ported from Sowel ui/src/pages/ZonesPage.tsx (Sowel commit 5da84dc1, UI 1.87.0). Changes (spec
// 018): no auth (`isAdmin`) and no `useWsSubscription` (Solite's `useLive` feeds the stores for
// every page); the header is written out (no `PageHeader`); the same form edits a zone, opened from
// the tree's rows; the delete is here (Sowel's is on its zone detail page, not ported).
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Home, Loader2, Plus } from "lucide-react";
import type { ZoneWithChildren } from "../../types";
import { useZones } from "../../store/useZones";
import { apiErrorStatus } from "../../lib/api-error";
import { ZoneTree } from "./ZoneTree";
import { ZoneForm, flattenZoneTree } from "./ZoneForm";

function countZones(zones: ZoneWithChildren[]): number {
  return zones.reduce((count, zone) => count + 1 + countZones(zone.children), 0);
}

export function ZonesPage() {
  const { t } = useTranslation();
  const tree = useZones((s) => s.tree);
  const loading = useZones((s) => s.loading);
  const error = useZones((s) => s.error);
  const [form, setForm] = useState<
    | { kind: "create"; parentId: string | null }
    | { kind: "edit"; zone: ZoneWithChildren; isRoot: boolean }
    | null
  >(null);

  const zoneCount = countZones(tree);
  const close = () => setForm(null);

  const remove = async (zone: ZoneWithChildren) => {
    try {
      await useZones.getState().deleteZone(zone.id);
    } catch (err) {
      // Deleted meanwhile by another browser: the tree is read again.
      if (apiErrorStatus(err) === 404) void useZones.getState().fetchZones();
      throw err;
    }
  };

  return (
    <div>
      <div className="flex flex-wrap items-start justify-between gap-3 mb-4">
        <div>
          <h1 className="text-display font-semibold text-text">{t("zones.title")}</h1>
          {zoneCount > 0 && (
            <p className="text-small text-text-secondary">
              {t("zones.count", { count: zoneCount })}
            </p>
          )}
        </div>
        <button
          onClick={() => setForm({ kind: "create", parentId: null })}
          className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-sm bg-primary text-white text-small font-medium hover:bg-primary-hover"
        >
          <Plus size={16} strokeWidth={1.5} />
          {t("zones.addZone")}
        </button>
      </div>

      {/* Content */}
      {loading && tree.length === 0 ? (
        <div className="flex items-center justify-center py-20">
          <Loader2 size={24} className="animate-spin text-text-tertiary" />
        </div>
      ) : error && tree.length === 0 ? (
        <ErrorState error={error} />
      ) : (
        <ZoneTree
          zones={tree}
          onAddChild={(parentId) => setForm({ kind: "create", parentId })}
          onEdit={(zone, isRoot) => setForm({ kind: "edit", zone, isRoot })}
          onDelete={remove}
        />
      )}

      {form?.kind === "create" && (
        <ZoneForm
          title={t("zones.createZone")}
          parentZones={flattenZoneTree(tree)}
          defaultParentId={form.parentId}
          onSubmit={async (data) => {
            await useZones
              .getState()
              .createZone({ ...data, description: data.description || undefined });
          }}
          onClose={close}
        />
      )}
      {form?.kind === "edit" && (
        <ZoneForm
          title={t("zones.editZone")}
          initial={{ name: form.zone.name, description: form.zone.description }}
          isRoot={form.isRoot}
          parentZones={flattenZoneTree(tree, form.zone.id)}
          defaultParentId={form.zone.parentId}
          onSubmit={async (data) => {
            try {
              await useZones.getState().updateZone(form.zone.id, data);
            } catch (err) {
              // Deleted meanwhile by another browser: the tree is read again.
              if (apiErrorStatus(err) === 404) void useZones.getState().fetchZones();
              throw err;
            }
          }}
          onClose={close}
        />
      )}
    </div>
  );
}

function ErrorState({ error }: { error: string }) {
  const { t } = useTranslation();

  return (
    <div className="flex flex-col items-center justify-center py-20 text-center">
      <div className="w-16 h-16 rounded-full bg-error/10 flex items-center justify-center mb-4">
        <Home size={28} strokeWidth={1.5} className="text-error" />
      </div>
      <h3 className="text-lead font-medium text-text mb-1">{t("zones.error.title")}</h3>
      <p className="text-small text-text-secondary max-w-[320px] mb-4">{error}</p>
      <button
        onClick={() => void useZones.getState().fetchZones()}
        className="px-4 py-2 bg-primary-fill text-white text-small font-medium rounded-sm hover:bg-primary-fill-hover transition-colors duration-150 ease-out"
      >
        {t("common.retry")}
      </button>
    </div>
  );
}
