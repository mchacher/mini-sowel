// Solite's own file: Discovery (spec 019, R2), one screen per integration at
// `#/discovery/<integrationId>`: the title, Sowel's integration tabs (shown even with one
// integration, so the screen says which one it is about), the screen of the integration shown.
import { useTranslation } from "react-i18next";
import { useDevices } from "../../store/useDevices";
import { TabButton } from "../ui/TabButton";
import { INTEGRATIONS, integrationFromHash } from "./integrations";

export function DiscoveryPage({ hash }: { hash: string }) {
  const { t } = useTranslation();
  const devices = useDevices((s) => s.devices);
  const current = integrationFromHash(hash);
  const Screen = current.Screen;
  const all = Object.values(devices);

  return (
    <div>
      <h1 className="text-display font-semibold text-text mb-4">{t("discovery.title")}</h1>
      <div className="flex gap-1 mb-4 border-b border-border overflow-x-auto">
        {INTEGRATIONS.map((integration) => (
          <TabButton
            key={integration.id}
            label={t(integration.labelKey)}
            count={all.filter((d) => d.integrationId === integration.id).length}
            active={integration.id === current.id}
            onClick={() => {
              window.location.hash = `#/discovery/${encodeURIComponent(integration.id)}`;
            }}
          />
        ))}
      </div>
      <Screen key={current.id} />
    </div>
  );
}
