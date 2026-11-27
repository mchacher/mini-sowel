import { useEffect, useState } from "react";
import { ChevronDown, ChevronRight, LogOut, Shield, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { FlatZone } from "../../zones";
import { ADMIN_NAV_ITEMS, type AdminPage } from "./admin-nav-items";
import { SidebarItem } from "./SidebarItem";
import { SoliteLogo } from "./SoliteLogo";
import { ZoneTree } from "./ZoneTree";
import { useAuth } from "../../store/useAuth";
import { useRecipeUpdates } from "./useRecipeUpdates";

/** The page shown: an administration page, or `null` for a room's page. */
export type Page = AdminPage;

type Zones = { zones: FlatZone[]; activeId: string | null; page?: Page | null };

function Brand() {
  return <SoliteLogo size={36} showWordmark className="min-w-0" />;
}

/**
 * The Administration block of Sowel's sidebar: its title (Shield, chevron) toggles the list of
 * entries; the list is open whenever an administration page is shown, and closed on a room's page
 * until the title is pressed.
 */
function AdminSection({ page, onPick }: { page: Page | null; onPick?: () => void }) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(page !== null);
  const recipeUpdateCount = useRecipeUpdates();
  useEffect(() => {
    if (page !== null) setExpanded(true);
  }, [page]);
  return (
    <div>
      <button
        type="button"
        onClick={() => setExpanded(!expanded)}
        aria-expanded={expanded}
        className={`flex items-center gap-3 w-full px-3 py-1.5 rounded-sm text-left transition-colors duration-150 ease-out ${
          page !== null && !expanded
            ? "text-primary"
            : "text-text-secondary hover:bg-background hover:text-text"
        }`}
      >
        <Shield size={16} strokeWidth={1.5} className="flex-shrink-0" />
        <span className="flex-1 text-small font-medium truncate">{t("nav.administration")}</span>
        {expanded ? (
          <ChevronDown size={14} strokeWidth={1.5} className="flex-shrink-0" />
        ) : (
          <ChevronRight size={14} strokeWidth={1.5} className="flex-shrink-0" />
        )}
      </button>
      {expanded && (
        <div className="mt-0.5 ml-3 space-y-0.5">
          {ADMIN_NAV_ITEMS.map(({ to, page: entry, labelKey, icon: Icon }) => {
            const showBadge = entry === "plugins" && recipeUpdateCount > 0;
            return (
              <SidebarItem
                key={entry}
                to={to}
                label={t(labelKey)}
                icon={
                  <span className="relative">
                    <Icon size={16} strokeWidth={1.5} />
                    {showBadge && (
                      <span className="absolute -top-1 -right-1 w-2 h-2 bg-error rounded-full" />
                    )}
                  </span>
                }
                badge={
                  showBadge ? (
                    <span
                      data-testid="plugins-badge"
                      className="text-micro font-medium text-error bg-error/10 px-1.5 py-0.5 rounded-full"
                    >
                      {recipeUpdateCount}
                    </span>
                  ) : undefined
                }
                active={page === entry}
                onClick={onPick}
              />
            );
          })}
        </div>
      )}
    </div>
  );
}

/** Ends the session: the host forgets it, the tokens are cleared, the login screen shows. */
function LogoutButton() {
  const { t } = useTranslation();
  return (
    <button
      type="button"
      onClick={() => void useAuth.getState().logout()}
      className="flex items-center gap-3 w-full px-3 py-1.5 mt-1 rounded-sm text-left text-text-secondary hover:bg-background hover:text-text transition-colors duration-150 ease-out"
    >
      <LogOut size={16} strokeWidth={1.5} className="flex-shrink-0" />
      <span className="text-small font-medium truncate">{t("auth.logout")}</span>
    </button>
  );
}

function Nav({ zones, activeId, page, onPick }: Zones & { onPick?: () => void }) {
  const { t } = useTranslation();
  return (
    <nav aria-label={t("app.mainNav")} className="flex-1 min-h-0 flex flex-col">
      {/* The rooms scroll alone; Administration and Log out stay pinned at the bottom, as Sowel's
          sidebar (spec 047). */}
      <div className="flex-1 min-h-0 overflow-y-auto py-3 px-2">
        <ZoneTree zones={zones} activeId={page ? null : activeId} onPick={onPick} />
      </div>
      <div className="border-t border-border-light py-2 px-2 min-h-0 overflow-y-auto">
        <AdminSection page={page ?? null} onPick={onPick} />
        <LogoutButton />
      </div>
    </nav>
  );
}

/**
 * The navigation column, in the look of Sowel's sidebar: the mark with the wordmark, the zones.
 * Hidden below `md`, where the top bar's menu button opens the same column as a drawer (Sowel
 * hides its sidebar below `lg` and opens a drawer the same way).
 */
export function Sidebar({ zones, activeId, page }: Zones) {
  return (
    <aside className="hidden md:flex flex-col h-full w-[260px] bg-surface border-r border-border shrink-0">
      <div className="flex items-center h-[49px] px-4 border-b border-border-light">
        <Brand />
      </div>
      <Nav zones={zones} activeId={activeId} page={page} />
    </aside>
  );
}

/** The phone's drawer: the zone tree over a backdrop; picking a zone or the backdrop closes it. */
export function MobileDrawer({ zones, activeId, page, onClose }: Zones & { onClose: () => void }) {
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
        <Nav zones={zones} activeId={activeId} page={page} onPick={onClose} />
      </div>
    </div>
  );
}
