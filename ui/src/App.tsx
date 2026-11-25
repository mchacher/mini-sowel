import { useEffect, useState } from "react";
import { MobileDrawer, Sidebar } from "./components/layout/Sidebar";
import { TopBar } from "./components/layout/TopBar";
import { TopBarFrame } from "./components/layout/TopBarFrame";
import { ZonePage } from "./components/home/ZonePage";
import { DevicesPage } from "./components/devices/DevicesPage";
import { DiscoveryPage } from "./components/discovery/DiscoveryPage";
import { adminPageFromHash, detailIdFromHash } from "./components/layout/admin-nav-items";
import { DeviceDetailPage } from "./components/devices/DeviceDetailPage";
import { EquipmentDetailPage } from "./components/equipments/EquipmentDetailPage";
import { EquipmentsPage } from "./components/equipments/EquipmentsPage";
import { SettingsPage } from "./components/settings/SettingsPage";
import { ZonesPage } from "./components/zones/ZonesPage";
import { useTranslation } from "react-i18next";
import { LoginPage } from "./components/auth/LoginPage";
import { SetupPage } from "./components/auth/SetupPage";
import { AuthScreen } from "./components/auth/AuthScreen";
import { BUTTON_SECONDARY } from "./components/ui/Modal";
import { useAuth } from "./store/useAuth";
import { connect, disconnect, useLive } from "./store/useLive";
import { useEquipments } from "./store/useEquipments";
import { useZoneAggregation } from "./store/useZoneAggregation";
import { useZones } from "./store/useZones";
import { useHomeSettings } from "./store/useHomeSettings";
import { useTimezone } from "./store/useTimezone";
import { chooseZone, flattenZones, zoneIdFromHash } from "./zones";

/** `location.hash`, followed through `hashchange`. */
function useHash(): string {
  const [hash, setHash] = useState(window.location.hash);
  useEffect(() => {
    const update = () => setHash(window.location.hash);
    window.addEventListener("hashchange", update);
    return () => window.removeEventListener("hashchange", update);
  }, []);
  return hash;
}

/**
 * What the page shows, from `GET /auth/status` and the session (spec 029, R9.1): the first-start
 * screen when the host has no account, the login screen without a session, else the home.
 */
export function App() {
  const { t } = useTranslation();
  const loading = useAuth((s) => s.loading);
  const setupRequired = useAuth((s) => s.setupRequired);
  const isAuthenticated = useAuth((s) => s.isAuthenticated);
  const unreachable = useAuth((s) => s.unreachable);

  useEffect(() => {
    void useAuth.getState().checkStatus();
  }, []);

  if (unreachable) {
    return (
      <AuthScreen>
        <div className="card p-6 text-center">
          <p className="text-body text-error mb-4">{t("auth.unreachable")}</p>
          <button
            type="button"
            onClick={() => void useAuth.getState().checkStatus()}
            className={BUTTON_SECONDARY}
          >
            {t("auth.retry")}
          </button>
        </div>
      </AuthScreen>
    );
  }
  if (setupRequired) return <SetupPage />;
  if (loading || setupRequired === null) return null;
  if (!isAuthenticated) return <LoginPage />;
  return <Home />;
}

/** The home screen: the sidebar of rooms and devices, and the page of the selected one. */
function Home() {
  const { t } = useTranslation();
  const tree = useZones((s) => s.tree);
  const zonesLoading = useZones((s) => s.loading);
  const zonesError = useZones((s) => s.error);
  const equipmentsError = useEquipments((s) => s.error);
  const error = zonesError ?? equipmentsError;
  const equipments = useEquipments((s) => s.equipments);
  const aggregation = useZoneAggregation((s) => s.data);
  const live = useLive((s) => s.status);
  const hash = useHash();
  const hashId = zoneIdFromHash(hash);
  const page = adminPageFromHash(hash);
  // An equipment's or a device's page when the address carries an id; the list otherwise.
  const equipmentId = detailIdFromHash(hash, "equipments");
  const deviceId = detailIdFromHash(hash, "devices");
  const [menuOpen, setMenuOpen] = useState(false);

  // The stores fetch when the socket opens, the first time and after each reconnection. The
  // socket closes with the home: a session that ended must not keep it. The top bar's settings
  // are also read once now, as Sowel's AppLayout does, so the name shows even if /ws never opens.
  useEffect(() => {
    void useHomeSettings.getState().fetch();
    void useTimezone.getState().fetch();
    connect();
    return disconnect;
  }, []);

  const flat = flattenZones(tree);
  const zone = chooseZone(flat, equipments, hashId);
  const onlyRoot = flat.length <= 1;
  const loaded = tree.length > 0 || (live === "open" && !zonesLoading);

  return (
    <div className="flex h-screen bg-background text-text">
      <Sidebar zones={flat} activeId={zone?.id ?? null} page={page} />
      {menuOpen && (
        <MobileDrawer
          zones={flat}
          activeId={zone?.id ?? null}
          page={page}
          onClose={() => setMenuOpen(false)}
        />
      )}
      <div className="flex flex-col flex-1 min-w-0">
        <TopBarFrame>
          <TopBar onMenu={() => setMenuOpen(true)} zoneId={zone?.id ?? null} page={page} />
        </TopBarFrame>
        <main className="flex-1 overflow-y-auto p-4 md:p-6 min-w-0">
          {error && <p className="text-body text-error">{error}</p>}
          {page === "devices" ? (
            deviceId ? (
              <DeviceDetailPage key={deviceId} deviceId={deviceId} />
            ) : (
              <DevicesPage />
            )
          ) : page === "equipments" ? (
            equipmentId ? (
              <EquipmentDetailPage key={equipmentId} equipmentId={equipmentId} />
            ) : (
              <EquipmentsPage />
            )
          ) : page === "zones" ? (
            <ZonesPage />
          ) : page === "discovery" ? (
            <DiscoveryPage hash={hash} />
          ) : page === "settings" ? (
            <SettingsPage />
          ) : zone ? (
            <ZonePage
              zone={zone}
              aggregate={aggregation[zone.id]}
              equipments={equipments.filter((e) => e.zoneId === zone.id)}
              onExecuteOrder={(id, alias, value) =>
                useEquipments.getState().executeOrder(id, alias, value)
              }
              empty={onlyRoot && !equipments.some((e) => e.zoneId === zone.id)}
            />
          ) : (
            loaded && !error && <p className="text-body text-text-secondary">{t("app.noRoom")}</p>
          )}
        </main>
      </div>
    </div>
  );
}
