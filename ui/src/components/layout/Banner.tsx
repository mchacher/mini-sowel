// mini-Sowel's own file: the banner shown while the live connection has been closed for more than
// 3 seconds (a restart of the host shows nothing).
import { useEffect, useState } from "react";
import { WifiOff } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useLive } from "../../store/useLive";

/** How long the socket may stay closed before the banner shows, in milliseconds. */
export const BANNER_DELAY_MS = 3000;

export function Banner() {
  const { t } = useTranslation();
  const closed = useLive((s) => s.status === "closed");
  const [shown, setShown] = useState(false);

  useEffect(() => {
    if (!closed) {
      setShown(false);
      return;
    }
    const timer = setTimeout(() => setShown(true), BANNER_DELAY_MS);
    return () => clearTimeout(timer);
  }, [closed]);

  if (!shown) return null;
  return (
    <div
      role="status"
      className="flex items-center gap-2 px-4 py-2 mb-4 rounded-md bg-warning/10 text-warning-ink text-small font-medium"
    >
      <WifiOff size={16} strokeWidth={1.5} />
      {t("app.connectionLost")}
    </div>
  );
}
