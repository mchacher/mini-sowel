// Solite's own file: Discovery (spec 019, R2), one screen per integration at
// `#/discovery/<integrationId>`: the title, Sowel's integration tabs only past one integration
// (with one, its name is a heading), the screen of the integration shown.
import { useTranslation } from "react-i18next";
import { useDevices } from "../../store/useDevices";
import { TabButton } from "../ui/TabButton";
import { INTEGRATIONS, integrationFromHash, type Integration } from "./integrations";

export function DiscoveryPage({
  hash,
  integrations = INTEGRATIONS,
}: {
  hash: string;
  integrations?: Integration[];
}) {
  const { t } = useTranslation();
  const devices = useDevices((s) => s.devices);
  const current = integrationFromHash(hash, integrations);
  const Screen = current.Screen;
  const all = Object.values(devices);

  return (
    <div>
      <h1 className="text-display font-semibold text-text mb-4">{t("discovery.title")}</h1>
      {integrations.length > 1 ? (
        // overflow-y-hidden: the active tab's -mb-px would otherwise draw a vertical scrollbar.
        <div className="flex gap-1 mb-4 border-b border-border overflow-x-auto overflow-y-hidden">
          {integrations.map((integration) => (
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
      ) : (
        <h2 className="text-body font-semibold text-text-secondary mb-4">{t(current.labelKey)}</h2>
      )}
      <Screen key={current.id} />
    </div>
  );
}
