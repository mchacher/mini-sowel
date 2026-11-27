// Ported from Sowel ui/src/components/layout/SidebarItem.tsx (Sowel commit 5da84dc1, UI 1.87.0),
// classes unchanged. Changes (spec 019): a hash link instead of `NavLink` (Solite has no router),
// `active` always given by the caller, no collapsed mode; the `badge` is kept (spec 042: the Plugins
// entry's update count).
import type { ReactNode } from "react";

interface SidebarItemProps {
  to: string;
  label: string;
  icon: ReactNode;
  active: boolean;
  /** Trailing element shown on the right (a count). */
  badge?: ReactNode;
  onClick?: () => void;
}

function itemClasses(isActive: boolean): string {
  const base =
    "flex items-center gap-3 px-3 py-1.5 rounded-sm min-w-0 transition-colors duration-150 ease-out";
  const state = isActive
    ? "bg-primary-light text-primary font-medium"
    : "text-text-secondary hover:bg-background hover:text-text";
  return `${base} ${state}`;
}

export function SidebarItem({ to, label, icon, active, badge, onClick }: SidebarItemProps) {
  return (
    <a
      href={to}
      onClick={(e) => {
        e.preventDefault();
        window.location.hash = to;
        onClick?.();
      }}
      aria-current={active ? "page" : undefined}
      className={itemClasses(active)}
    >
      <span className="flex-shrink-0 relative">{icon}</span>
      <span className="text-small font-medium truncate">{label}</span>
      {badge !== undefined && <span className="ml-auto flex-shrink-0">{badge}</span>}
    </a>
  );
}
