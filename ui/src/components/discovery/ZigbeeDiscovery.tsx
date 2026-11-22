// Solite's own file: the Zigbee screen of Discovery (spec 019, R2). Spec 017's pairing controls,
// moved from the devices page unchanged ("Add a device", the countdown, "Stop", the radio's
// refusal), and the devices discovered while the screen is open, newest first, marked "New", as
// rows of the Devices list.
import { useEffect } from "react";
import { Radio } from "lucide-react";
import { useTranslation } from "react-i18next";
import { usePairing } from "../../store/usePairing";
import { useDevices } from "../../store/useDevices";
import { DeviceList } from "../devices/DeviceList";

/** The id of Solite's Zigbee integration, as in the devices and the API's routes. */
export const SOLITE_INTEGRATION_ID = "solite";

export function ZigbeeDiscovery() {
  const { t } = useTranslation();
  const devices = useDevices((s) => s.devices);
  const open = usePairing((s) => s.open);
  const remaining = usePairing((s) => s.remaining);
  const pairingError = usePairing((s) => s.error);
  const newIds = usePairing((s) => s.newIds);

  // The "New" marks cover the time the screen is open, and last until it is left.
  useEffect(() => {
    usePairing.getState().setPageOpen(true);
    return () => usePairing.getState().setPageOpen(false);
  }, []);

  // Newest first: the discovery order reversed; a device removed meanwhile is gone from the list.
  const found = [...newIds]
    .reverse()
    .flatMap((id) => devices[id] ?? [])
    .filter((d) => d.integrationId === SOLITE_INTEGRATION_ID);

  return (
    <div>
      <div className="flex flex-wrap items-center justify-between gap-3 mb-4">
        <p className="text-small text-text-secondary">
          {open ? (
            <>
              {t("devices.pairing.open")}
              {" · "}
              <span className="tabular-nums">
                {t("devices.pairing.seconds", { count: remaining })}
              </span>
            </>
          ) : null}
        </p>
        {open ? (
          <button
            type="button"
            onClick={() => void usePairing.getState().stop()}
            className="px-3 py-1.5 rounded-sm border border-border text-small font-medium text-text hover:bg-border-light"
          >
            {t("devices.pairing.stop")}
          </button>
        ) : (
          <button
            type="button"
            onClick={() => void usePairing.getState().start()}
            className="px-3 py-1.5 rounded-sm bg-primary text-white text-small font-medium hover:bg-primary-hover"
          >
            {t("devices.add")}
          </button>
        )}
      </div>
      {pairingError && <p className="text-small text-error mb-3">{pairingError}</p>}
      {found.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-16 text-center">
          <div className="w-14 h-14 rounded-full bg-border-light flex items-center justify-center mb-4">
            <Radio size={28} strokeWidth={1.5} className="text-text-tertiary" />
          </div>
          <p className="text-small text-text-secondary max-w-[320px] mb-2">
            {t("discovery.empty")}
          </p>
          <a href="#/devices" className="text-small text-primary hover:underline">
            {t("discovery.seeAll")}
          </a>
        </div>
      ) : (
        <DeviceList devices={found} newIds={newIds} />
      )}
    </div>
  );
}
