// mini-Sowel's replacement for Sowel's ui/src/components/layout/SidebarZoneTree.tsx, which needs
// react-router and an expand/collapse state per node: the same classes and icons, the zones as links
// to `#/zone/<id>`, every node open.
import { DoorOpen, Home, Layers } from "lucide-react";
import type { FlatZone } from "../../zones";
import { hashForZone } from "../../zones";

export function ZoneTree({
  zones,
  activeId,
  onPick,
}: {
  zones: FlatZone[];
  activeId: string | null;
  /** Called after a zone is picked (the phone's drawer closes). */
  onPick?: () => void;
}) {
  return (
    <div className="space-y-0.5">
      {zones.map(({ zone, depth }) => {
        const active = zone.id === activeId;
        const link = (
          e: { preventDefault: () => void }, // the hash is set by hand: the same in every browser
        ) => {
          e.preventDefault();
          window.location.hash = hashForZone(zone.id);
          onPick?.();
        };
        if (depth === 0) {
          return (
            <a
              key={zone.id}
              href={hashForZone(zone.id)}
              onClick={link}
              aria-current={active ? "page" : undefined}
              className={`flex items-center gap-2 px-3 py-1.5 min-w-0 rounded-sm mx-1 group transition-colors duration-150 ${active ? "bg-primary-light" : ""}`}
            >
              <Home
                size={14}
                strokeWidth={1.5}
                className={`flex-shrink-0 transition-colors ${active ? "text-primary" : "text-text group-hover:text-primary"}`}
              />
              <span
                className={`text-body font-medium transition-colors truncate ${active ? "text-primary" : "text-text group-hover:text-primary"}`}
              >
                {zone.name}
              </span>
            </a>
          );
        }
        return (
          <div key={zone.id} style={{ paddingLeft: `${8 + depth * 12}px` }}>
            <a
              href={hashForZone(zone.id)}
              onClick={link}
              aria-current={active ? "page" : undefined}
              className={`
                flex items-center gap-2 px-2 py-1.5 rounded-sm min-w-0
                text-small transition-colors duration-150 ease-out
                ${
                  active
                    ? "bg-primary-light text-primary font-medium"
                    : "text-text-secondary hover:bg-border-light hover:text-text"
                }
              `}
            >
              <span className="flex-shrink-0">
                {zone.children.length > 0 ? (
                  <Layers size={15} strokeWidth={1.5} />
                ) : (
                  <DoorOpen size={15} strokeWidth={1.5} />
                )}
              </span>
              <span className="truncate">{zone.name}</span>
            </a>
          </div>
        );
      })}
    </div>
  );
}
