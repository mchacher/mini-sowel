import { Menu, Radio, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { FlatZone } from "../../zones";
import { SoliteLogo } from "./SoliteLogo";
import { ZoneTree } from "./ZoneTree";

type Zones = { zones: FlatZone[]; activeId: string | null; devicesActive?: boolean };

function Brand() {
  return <SoliteLogo size={36} showWordmark className="min-w-0" />;
}

function Nav({ zones, activeId, devicesActive, onPick }: Zones & { onPick?: () => void }) {
  const { t } = useTranslation();
  return (
    <nav aria-label={t("app.mainNav")} className="flex-1 overflow-y-auto py-3 px-2">
      <ZoneTree zones={zones} activeId={devicesActive ? null : activeId} onPick={onPick} />
      <a
        href="#/devices"
        onClick={(e) => {
          e.preventDefault();
          window.location.hash = "#/devices";
          onPick?.();
        }}
        aria-current={devicesActive ? "page" : undefined}
        className={`flex items-center gap-2 px-3 py-1.5 mt-3 mx-1 min-w-0 rounded-sm group transition-colors duration-150 ${devicesActive ? "bg-primary-light" : ""}`}
      >
        <Radio
          size={14}
          strokeWidth={1.5}
          className={`flex-shrink-0 transition-colors ${devicesActive ? "text-primary" : "text-text group-hover:text-primary"}`}
        />
        <span
          className={`text-body font-medium transition-colors truncate ${devicesActive ? "text-primary" : "text-text group-hover:text-primary"}`}
        >
          {t("app.devices")}
        </span>
      </a>
    </nav>
  );
}

/**
 * The navigation column, in the look of Sowel's sidebar: the mark with the wordmark, the zones.
 * Hidden below `md`, where the top bar's menu button opens the same column as a drawer (Sowel
 * hides its sidebar below `lg` and opens a drawer the same way).
 */
export function Sidebar({ zones, activeId, devicesActive }: Zones) {
  return (
    <aside className="hidden md:flex flex-col h-full w-[260px] bg-surface border-r border-border shrink-0">
      <div className="flex items-center h-[49px] px-4 border-b border-border-light">
        <Brand />
      </div>
      <Nav zones={zones} activeId={activeId} devicesActive={devicesActive} />
    </aside>
  );
}

/** The phone's top bar: the menu button and the name. */
export function MobileTopBar({ onMenu }: { onMenu: () => void }) {
  const { t } = useTranslation();
  return (
    <header className="flex md:hidden items-center h-topbar px-3 border-b border-border-light bg-surface gap-2 shrink-0">
      <button
        onClick={onMenu}
        aria-label={t("app.menu")}
        className="w-10 h-10 -ml-2 flex items-center justify-center rounded-sm text-text-secondary hover:bg-border-light/60 transition-colors duration-150"
      >
        <Menu size={20} strokeWidth={1.7} />
      </button>
      <Brand />
    </header>
  );
}

/** The phone's drawer: the zone tree over a backdrop; picking a zone or the backdrop closes it. */
export function MobileDrawer({
  zones,
  activeId,
  devicesActive,
  onClose,
}: Zones & { onClose: () => void }) {
  const { t } = useTranslation();
  return (
    <div className="fixed inset-0 z-50 md:hidden">
      <div className="absolute inset-0 bg-black/40" onClick={onClose} />
      <div
        role="dialog"
        aria-label={t("app.menu")}
        className="absolute inset-y-0 left-0 flex flex-col w-[320px] max-w-[85vw] bg-surface border-r border-border"
      >
        <div className="flex items-center justify-between h-topbar px-4 border-b border-border-light">
          <div className="flex items-center min-w-0">
            <Brand />
          </div>
          <button
            onClick={onClose}
            aria-label={t("app.closeMenu")}
            className="p-1.5 rounded-sm text-text-tertiary hover:bg-border-light"
          >
            <X size={18} strokeWidth={1.5} />
          </button>
        </div>
        <Nav zones={zones} activeId={activeId} devicesActive={devicesActive} onPick={onClose} />
      </div>
    </div>
  );
}
