// Ported from Sowel ui/src/pages/DevicesPage.tsx (Sowel commit 5da84dc1, UI 1.87.0), cut (spec 019,
// R3): the title and subtitle, the integration tabs (only when the devices come from more than one
// integration, as Sowel), `DeviceList` of every device sorted by name, Sowel's empty and error
// states; no search, no sort, no source filter. The "Add devices" button opens Discovery.
import { useState } from "react";
import { Radio } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useDevices } from "../../store/useDevices";
import { TabButton } from "../ui/TabButton";
import { INTEGRATIONS } from "../discovery/integrations";
import { DeviceList } from "./DeviceList";

const ALL_TAB = "all";

export function DevicesPage() {
  const { t } = useTranslation();
  const devices = useDevices((s) => s.devices);
  const loadError = useDevices((s) => s.error);
  const [tab, setTab] = useState(ALL_TAB);

  const all = Object.values(devices);
  const tabs = Array.from(new Set(all.map((d) => d.integrationId).filter(Boolean))).sort((a, b) =>
    a.localeCompare(b),
  );
  // A tab that no longer exists falls back to all of them.
  const shown = tab === ALL_TAB || tabs.includes(tab) ? tab : ALL_TAB;
  const list = (shown === ALL_TAB ? all : all.filter((d) => d.integrationId === shown)).sort(
    (a, b) => a.name.localeCompare(b.name),
  );
  const online = all.filter((d) => d.status === "online").length;
  const labelOf = (id: string) => {
    const known = INTEGRATIONS.find((i) => i.id === id);
    return known ? t(known.labelKey) : id;
  };

  return (
    <div>
      <div className="flex flex-wrap items-start justify-between gap-3 mb-4">
        <div>
          <h1 className="text-display font-semibold text-text">{t("devices.title")}</h1>
          {all.length > 0 && (
            <p className="text-small text-text-secondary">
              {t("devices.subtitle", { count: all.length, online })}
            </p>
          )}
        </div>
        <button
          type="button"
          onClick={() => {
            window.location.hash = "#/discovery/solite";
          }}
          className="px-3 py-1.5 rounded-sm bg-primary text-white text-small font-medium hover:bg-primary-hover"
        >
          {t("devices.addDevices")}
        </button>
      </div>

      {tabs.length > 1 && (
        <div className="flex gap-1 mb-4 border-b border-border overflow-x-auto">
          <TabButton
            label={t("common.all")}
            count={all.length}
            active={shown === ALL_TAB}
            onClick={() => setTab(ALL_TAB)}
          />
          {tabs.map((id) => (
            <TabButton
              key={id}
              label={labelOf(id)}
              count={all.filter((d) => d.integrationId === id).length}
              active={shown === id}
              onClick={() => setTab(id)}
            />
          ))}
        </div>
      )}

      {loadError && all.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-20 text-center">
          <h3 className="text-lead font-medium text-text mb-1">{t("devices.error.title")}</h3>
          <p className="text-small text-text-secondary max-w-[320px] mb-4">{loadError}</p>
          <button
            type="button"
            onClick={() => void useDevices.getState().fetchDevices()}
            className="px-3 py-1.5 rounded-sm border border-border text-small font-medium text-text hover:bg-border-light"
          >
            {t("common.retry")}
          </button>
        </div>
      ) : (
        <>
          {loadError && <p className="text-small text-error mb-3">{loadError}</p>}
          {all.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-20 text-center">
              <div className="w-14 h-14 rounded-full bg-border-light flex items-center justify-center mb-4">
                <Radio size={28} strokeWidth={1.5} className="text-text-tertiary" />
              </div>
              <h3 className="text-lead font-medium text-text mb-1">{t("devices.empty.title")}</h3>
              <p className="text-small text-text-secondary max-w-[320px]">
                <a href="#/discovery/solite" className="text-primary hover:underline">
                  {t("devices.empty.message")}
                </a>
              </p>
            </div>
          ) : (
            <DeviceList devices={list} />
          )}
        </>
      )}
    </div>
  );
}
