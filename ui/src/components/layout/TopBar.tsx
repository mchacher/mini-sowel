// Ported from Sowel ui/src/components/layout/AppLayout.tsx (Sowel commit 2d360726): the header
// (lines 178-262), `TopbarBreadcrumb`, `MobileTopbarTitle` and `HomeNameSkeleton`. Changed: the
// switch point `lg` is `md`; no router, so the page and the zone come from the props (the page of
// the hash, the zone `chooseZone` picked) and the parts from `top-bar-context.ts`; the menu button
// opens Solite's drawer; no alarms, updates or restart pills, no user block; `hit-group` dropped
// (Solite's CSS has no such utility); the home's name comes from `useHomeSettings`; and the
// "Set the location" link (Solite's, D62) where Sowel shows no sun times.
import { Menu } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useHomeTimeShown, useLinkDegraded } from "../../hooks/useTopBarState";
import { useHomeSettings } from "../../store/useHomeSettings";
import { useZoneAggregation } from "../../store/useZoneAggregation";
import { useZones } from "../../store/useZones";
import { ROOT_ZONE_ID } from "../../zones";
import { ZONE_PATH_SEPARATOR } from "../../lib/zone-path";
import { ConnectionStatus } from "./ConnectionStatus";
import { HomeTimeNote } from "./HomeTimeNote";
import type { Page } from "./Sidebar";
import { MobileSunLocationHint, MobileSunTimes, SunLocationHint, SunTimes } from "./SunTimes";
import { topBarContext } from "./top-bar-context";

// Placeholder for the home name while the settings load: a generic « Maison »
// would flash and then change (spec 196, FR-6).
function HomeNameSkeleton() {
  return (
    <span
      aria-hidden="true"
      data-testid="home-name-skeleton"
      className="inline-block align-middle w-24 h-3.5 rounded-sm bg-border-light"
    />
  );
}

// Mock parity: text-small / n-400 / weight 500 for crumb, n-700 weight 600 for current.
function TopbarBreadcrumb({ homeLabel, parts }: { homeLabel: React.ReactNode; parts: string[] }) {
  if (parts.length === 0) {
    return (
      <span className="block truncate text-small font-semibold text-[var(--n-700)]">
        {homeLabel}
      </span>
    );
  }
  // Truncated, unlike Sowel's: a long name and path never wrap the bar (spec 037, edge cases).
  return (
    <div
      data-testid="topbar-breadcrumb"
      className="truncate text-small font-medium text-[var(--n-400)]"
    >
      <span>{homeLabel}</span>
      {parts.map((part, i) => (
        <span key={i}>
          <span className="mx-[.35rem] opacity-35">›</span>
          {i === parts.length - 1 ? (
            <b className="font-semibold text-[var(--n-700)]">{part}</b>
          ) : (
            <span>{part}</span>
          )}
        </span>
      ))}
    </div>
  );
}

// Mobile topbar title — home name big, current context small.
function MobileTopbarTitle({ homeLabel, parts }: { homeLabel: React.ReactNode; parts: string[] }) {
  const { t } = useTranslation();
  const sub = parts.length > 0 ? parts.join(ZONE_PATH_SEPARATOR) : t("nav.maison");
  return (
    <div className="min-w-0">
      <div className="text-lead font-semibold text-text leading-[1.15] tracking-[-0.015em] truncate">
        {homeLabel}
      </div>
      <div className="text-micro text-text-tertiary font-medium leading-[1.3] truncate">{sub}</div>
    </div>
  );
}

/**
 * The top bar of every page of the home (spec 037): the home's name and where you are on the left,
 * the home's time, the sun times and the link indicator on the right. `zoneId` is the zone the page
 * shows (`chooseZone`), `page` the administration page, `null` on a room's page.
 */
export function TopBar({
  onMenu,
  zoneId,
  page,
}: {
  onMenu: () => void;
  zoneId: string | null;
  page: Page | null;
}) {
  const { t } = useTranslation();
  const tree = useZones((s) => s.tree);
  const homeName = useHomeSettings((s) => s.name);
  const hasLocation = useHomeSettings((s) => s.hasLocation);
  const rootAgg = useZoneAggregation((s) => s.data[ROOT_ZONE_ID]);
  // What shares the bar with the title below md: the sun gives way to it on phones (#1133).
  const homeTime = useHomeTimeShown().shown;
  const linkDegraded = useLinkDegraded().shown;
  const headerPills = [homeTime, linkDegraded].filter(Boolean).length;

  const homeLabel = homeName === null ? <HomeNameSkeleton /> : homeName || t("nav.maison");
  const parts = topBarContext({ page, zoneId, tree, t });
  // Solite ships with no location: the link to Settings where Sowel shows nothing.
  const noLocation = hasLocation === false && rootAgg !== undefined && rootAgg.sunrise === null;

  return (
    <header className="flex items-center h-topbar md:h-auto md:min-h-[49px] px-3 md:px-6 border-b border-border-light bg-surface gap-2">
      {/* Left: menu button and title+sub on phones, breadcrumb from md up */}
      <div className="flex items-center gap-2 flex-1 min-w-0">
        <button
          type="button"
          onClick={onMenu}
          aria-label={t("app.menu")}
          className="md:hidden w-10 h-10 -ml-2 flex-shrink-0 flex items-center justify-center rounded-sm text-text-secondary hover:bg-border-light/60 transition-colors duration-150"
        >
          <Menu size={20} strokeWidth={1.7} />
        </button>
        <div className="md:hidden flex-1 min-w-0">
          <MobileTopbarTitle homeLabel={homeLabel} parts={parts} />
        </div>
        <div className="hidden md:flex items-center min-w-0">
          <TopbarBreadcrumb homeLabel={homeLabel} parts={parts} />
        </div>
      </div>

      {/* Right: the home time only when the browser is in another time zone, the sun times (or the
        link to Settings), the link only when degraded. */}
      <div className="flex items-center gap-2 flex-shrink-0">
        <div className="hidden md:flex items-center gap-3 mr-1">
          <HomeTimeNote />
          {noLocation ? <SunLocationHint /> : <SunTimes data={rootAgg} />}
        </div>
        <div className="flex md:hidden items-center gap-2">
          <HomeTimeNote compact />
          {noLocation ? (
            <MobileSunLocationHint headerPills={headerPills} />
          ) : (
            <MobileSunTimes data={rootAgg} headerPills={headerPills} />
          )}
        </div>
        <ConnectionStatus />
      </div>
    </header>
  );
}
