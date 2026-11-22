// Solite's own file: the devices page (spec 017, R5). Sowel's DevicesPage and DeviceList (267 and
// 416 lines, grouping, filters, data table) are out of scope; this keeps their title and subtitle,
// their empty state and their status colours.
import { useEffect } from "react";
import { Radio } from "lucide-react";
import { useTranslation } from "react-i18next";
import { usePairing } from "../../store/usePairing";
import { useDevices } from "../../store/useDevices";
import { useZones } from "../../store/useZones";
import { flattenZones } from "../../zones";
import { DeviceRow } from "./DeviceRow";

export function DevicesPage() {
  const { t } = useTranslation();
  const devices = useDevices((s) => s.devices);
  const loadError = useDevices((s) => s.error);
  const tree = useZones((s) => s.tree);
  const open = usePairing((s) => s.open);
  const remaining = usePairing((s) => s.remaining);
  const pairingError = usePairing((s) => s.error);
  const newIds = usePairing((s) => s.newIds);

  // The "New" marks cover the time the page is open, and last until it is left.
  useEffect(() => {
    usePairing.getState().setPageOpen(true);
    return () => usePairing.getState().setPageOpen(false);
  }, []);

  // Newest first: the pairing order reversed.
  const list = Object.values(devices).reverse();
  const online = list.filter((d) => d.status === "online").length;
  // Every zone but the home's root.
  const zones = flattenZones(tree).filter((f) => f.depth > 0);

  return (
    <div>
      <div className="flex flex-wrap items-start justify-between gap-3 mb-4">
        <div>
          <h1 className="text-display font-semibold text-text">{t("devices.title")}</h1>
          <p className="text-small text-text-secondary">
            {t("devices.subtitle", { count: list.length, online })}
          </p>
        </div>
        {open ? (
          <div className="flex items-center gap-3">
            <span className="text-small text-text-secondary">
              {t("devices.pairing.open")}
              {" · "}
              <span className="tabular-nums">
                {t("devices.pairing.seconds", { count: remaining })}
              </span>
            </span>
            <button
              onClick={() => void usePairing.getState().stop()}
              className="px-3 py-1.5 rounded-sm border border-border text-small font-medium text-text hover:bg-border-light"
            >
              {t("devices.pairing.stop")}
            </button>
          </div>
        ) : (
          <button
            onClick={() => void usePairing.getState().start()}
            className="px-3 py-1.5 rounded-sm bg-primary text-white text-small font-medium hover:bg-primary-hover"
          >
            {t("devices.add")}
          </button>
        )}
      </div>
      {pairingError && <p className="text-small text-error mb-3">{pairingError}</p>}
      {loadError && <p className="text-small text-error mb-3">{loadError}</p>}
      {list.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-20 text-center">
          <div className="w-14 h-14 rounded-full bg-border-light flex items-center justify-center mb-4">
            <Radio size={28} strokeWidth={1.5} className="text-text-tertiary" />
          </div>
          <h3 className="text-lead font-medium text-text mb-1">{t("devices.empty.title")}</h3>
          <p className="text-small text-text-secondary max-w-[320px]">
            {t("devices.empty.message")}
          </p>
        </div>
      ) : (
        <ul className="space-y-2">
          {list.map((device) => (
            <DeviceRow
              key={device.id}
              device={device}
              zones={zones}
              isNew={newIds.includes(device.id)}
            />
          ))}
        </ul>
      )}
    </div>
  );
}
