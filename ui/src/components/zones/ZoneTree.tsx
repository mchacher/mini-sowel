// Ported from Sowel ui/src/components/zones/ZoneTree.tsx (Sowel commit 5da84dc1, UI 1.87.0). Changes
// (spec 018): the reorder arrows cut (Solite has no reorder route); every node open (no
// expand/collapse, as the sidebar); the node's name opens the room page in the hash instead of
// Sowel's zone detail page, whose edit and delete move into the row ("Edit", "Delete"); the
// buttons are always visible (a phone has no hover); a refusal of the delete shows under the row
// for 5 s, as the devices page's rows.
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Building2, DoorOpen, Home, Layers, Pencil, Plus, Trash2 } from "lucide-react";
import type { ZoneWithChildren } from "../../types";
import { apiErrorText } from "../../lib/api-error";
import { hashForZone } from "../../zones";

/** How long a refusal stays under the row. */
const ERROR_MS = 5000;

interface ZoneTreeProps {
  zones: ZoneWithChildren[];
  onAddChild: (parentId: string) => void;
  onEdit: (zone: ZoneWithChildren, isRoot: boolean) => void;
  /** Rejects with the API's error when the zone cannot be deleted. */
  onDelete: (zone: ZoneWithChildren) => Promise<void>;
}

export function ZoneTree({ zones, onAddChild, onEdit, onDelete }: ZoneTreeProps) {
  if (zones.length === 0) {
    return <EmptyState />;
  }

  return (
    <div className="card overflow-hidden">
      <div className="divide-y divide-border-light">
        {zones.map((zone) => (
          <ZoneTreeNode
            key={zone.id}
            zone={zone}
            depth={0}
            onAddChild={onAddChild}
            onEdit={onEdit}
            onDelete={onDelete}
          />
        ))}
      </div>
    </div>
  );
}

interface ZoneTreeNodeProps extends Omit<ZoneTreeProps, "zones"> {
  zone: ZoneWithChildren;
  depth: number;
}

const ACTION =
  "p-1 rounded-sm text-text-tertiary hover:text-primary hover:bg-primary-light transition-colors cursor-pointer";

function ZoneTreeNode({ zone, depth, onAddChild, onEdit, onDelete }: ZoneTreeNodeProps) {
  const { t } = useTranslation();
  const [error, setError] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const hasChildren = zone.children.length > 0;
  const childCount = zone.children.length;
  const isRoot = depth === 0;

  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );

  const remove = async () => {
    if (!window.confirm(t("zones.deleteConfirm", { name: zone.name }))) return;
    try {
      await onDelete(zone);
    } catch (err) {
      setError(apiErrorText(err, t));
      if (timer.current) clearTimeout(timer.current);
      timer.current = setTimeout(() => setError(null), ERROR_MS);
    }
  };

  return (
    <div>
      <div
        className="flex items-center gap-2 px-4 py-3 hover:bg-primary-light/40 transition-colors duration-150"
        style={{ paddingLeft: `${16 + depth * 24}px` }}
      >
        {/* Zone info — clickable to open the room */}
        <a
          href={hashForZone(zone.id)}
          onClick={(e) => {
            e.preventDefault();
            window.location.hash = hashForZone(zone.id);
          }}
          className="flex-1 flex items-center gap-3 min-w-0"
        >
          <span className="flex-shrink-0 text-text-secondary">
            {isRoot ? (
              <Building2 size={18} strokeWidth={1.5} />
            ) : hasChildren ? (
              <Layers size={18} strokeWidth={1.5} />
            ) : (
              <DoorOpen size={18} strokeWidth={1.5} />
            )}
          </span>

          <div className="flex-1 min-w-0">
            <span className="text-body font-medium text-text truncate block">{zone.name}</span>
            {zone.description && (
              <span className="text-caption text-text-tertiary truncate block">
                {zone.description}
              </span>
            )}
          </div>

          {/* Badges */}
          <div className="flex items-center gap-2 flex-shrink-0">
            {childCount > 0 && (
              <span className="text-micro text-text-tertiary bg-border-light px-2 py-0.5 rounded-full">
                {t("zones.childCount", { count: childCount })}
              </span>
            )}
          </div>
        </a>

        {/* Action buttons */}
        <div className="flex items-center gap-0.5">
          <button
            onClick={() => onAddChild(zone.id)}
            className={ACTION}
            title={t("zones.addSubZone")}
            aria-label={t("zones.addSubZone")}
          >
            <Plus size={14} strokeWidth={1.5} />
          </button>
          <button
            onClick={() => onEdit(zone, isRoot)}
            className={ACTION}
            title={t("common.edit")}
            aria-label={t("common.edit")}
          >
            <Pencil size={14} strokeWidth={1.5} />
          </button>
          {!isRoot && (
            <button
              onClick={() => void remove()}
              className="p-1 rounded-sm text-text-tertiary hover:text-error hover:bg-error/10 transition-colors cursor-pointer"
              title={t("common.delete")}
              aria-label={t("common.delete")}
            >
              <Trash2 size={14} strokeWidth={1.5} />
            </button>
          )}
        </div>
      </div>
      {error && (
        <p className="text-caption text-error pb-2" style={{ paddingLeft: `${16 + depth * 24}px` }}>
          {error}
        </p>
      )}

      {/* Children */}
      {hasChildren && (
        <div>
          {zone.children.map((child) => (
            <ZoneTreeNode
              key={child.id}
              zone={child}
              depth={depth + 1}
              onAddChild={onAddChild}
              onEdit={onEdit}
              onDelete={onDelete}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function EmptyState() {
  const { t } = useTranslation();

  return (
    <div className="flex flex-col items-center justify-center py-20 text-center">
      <div className="w-16 h-16 rounded-full bg-border-light flex items-center justify-center mb-4">
        <Home size={28} strokeWidth={1.5} className="text-text-tertiary" />
      </div>
      <h3 className="text-lead font-medium text-text mb-1">{t("zones.empty.title")}</h3>
      <p className="text-small text-text-secondary max-w-[320px]">{t("zones.empty.message")}</p>
    </div>
  );
}
