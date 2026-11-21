import { useEffect, useState } from "react";
import { Banner } from "./components/layout/Banner";
import { MobileDrawer, MobileTopBar, Sidebar } from "./components/layout/Sidebar";
import { ZonePage } from "./components/home/ZonePage";
import { useTranslation } from "react-i18next";
import { connect, useLive } from "./store/useLive";
import { useEquipments } from "./store/useEquipments";
import { useZoneAggregation } from "./store/useZoneAggregation";
import { useZones } from "./store/useZones";
import { chooseZone, flattenZones, zoneIdFromHash } from "./zones";

/** The id in `location.hash`, followed through `hashchange`. */
function useHashZone(): string | null {
  const [hash, setHash] = useState(window.location.hash);
  useEffect(() => {
    const update = () => setHash(window.location.hash);
    window.addEventListener("hashchange", update);
    return () => window.removeEventListener("hashchange", update);
  }, []);
  return zoneIdFromHash(hash);
}

/** The home screen: the sidebar of rooms and the page of the selected one. */
export function App() {
  const { t } = useTranslation();
  const tree = useZones((s) => s.tree);
  const zonesLoading = useZones((s) => s.loading);
  const zonesError = useZones((s) => s.error);
  const equipmentsError = useEquipments((s) => s.error);
  const error = zonesError ?? equipmentsError;
  const equipments = useEquipments((s) => s.equipments);
  const aggregation = useZoneAggregation((s) => s.data);
  const live = useLive((s) => s.status);
  const hashId = useHashZone();
  const [menuOpen, setMenuOpen] = useState(false);

  // The stores fetch when the socket opens, the first time and after each reconnection.
  useEffect(connect, []);

  const flat = flattenZones(tree);
  const zone = chooseZone(flat, equipments, hashId);
  const onlyRoot = flat.length <= 1;
  const loaded = tree.length > 0 || (live === "open" && !zonesLoading);

  return (
    <div className="flex h-screen bg-background text-text">
      <Sidebar zones={flat} activeId={zone?.id ?? null} />
      {menuOpen && (
        <MobileDrawer zones={flat} activeId={zone?.id ?? null} onClose={() => setMenuOpen(false)} />
      )}
      <div className="flex flex-col flex-1 min-w-0">
        <MobileTopBar onMenu={() => setMenuOpen(true)} />
        <main className="flex-1 overflow-y-auto p-4 md:p-6 min-w-0">
          <Banner />
          {error && <p className="text-body text-error">{error}</p>}
          {zone ? (
            <ZonePage
              zone={zone}
              aggregate={aggregation[zone.id]}
              equipments={equipments.filter((e) => e.zoneId === zone.id)}
              onExecuteOrder={(id, alias, value) =>
                useEquipments.getState().executeOrder(id, alias, value)
              }
              empty={onlyRoot}
            />
          ) : (
            loaded && !error && <p className="text-body text-text-secondary">{t("app.noRoom")}</p>
          )}
        </main>
      </div>
    </div>
  );
}
