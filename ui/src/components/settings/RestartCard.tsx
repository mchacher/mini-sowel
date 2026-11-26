// The Restart card (spec 043, D79): a button, a confirmation, then a wait while the box is away.
// Sowel's route (Sowel's `triggerSystemRestart` calls it to restart its container); Sowel's `card` and tokens, the Modal of the other cards.
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { getSystemVersion, restartBox } from "../../api";
import { ApiError } from "../../api/client";
import { formErrorText } from "../../lib/form-error";
import { BUTTON_PRIMARY, BUTTON_SECONDARY, Modal } from "../ui/Modal";

/** The version is read this often while the box is away (never two at once). */
const POLL_GAP_MS = 2000;
/** A poll that hangs is given up after this long. */
const POLL_TIMEOUT_MS = 5000;
/** Without an answer after this long, the card asks to reload by hand. */
const GIVE_UP_MS = 90_000;

type Phase = "idle" | "confirming" | "restarting" | "lost";

interface Props {
  /** The page's reload; replaced in tests. */
  reload?: () => void;
}

export function RestartCard({ reload = () => window.location.reload() }: Props) {
  const { t } = useTranslation();
  const [phase, setPhase] = useState<Phase>("idle");
  const [error, setError] = useState<string | null>(null);
  const alive = useRef(true);
  const poll = useRef<AbortController | null>(null);

  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
      poll.current?.abort();
    };
  }, []);

  /**
   * Polls until the box answers again, then reloads. The session ends with the restart, so a 401
   * is an answer. The box still answers while it stops its engine (up to 10 s): only an answer
   * after a poll that failed counts, or the page would reload onto a box about to go away.
   */
  const waitForBox = async () => {
    const started = Date.now();
    let wasAway = false;
    while (Date.now() - started < GIVE_UP_MS) {
      await new Promise((resolve) => setTimeout(resolve, POLL_GAP_MS));
      if (!alive.current) return;
      const controller = new AbortController();
      poll.current = controller;
      const timer = setTimeout(() => controller.abort(), POLL_TIMEOUT_MS);
      // A 401 is the box, back, with no session; anything else is the box still away or busy.
      const answered = await getSystemVersion(controller.signal).then(
        () => true,
        (err: unknown) => err instanceof ApiError && err.status === 401,
      );
      clearTimeout(timer);
      if (!alive.current) return;
      if (answered && wasAway) {
        reload();
        return;
      }
      if (!answered) wasAway = true;
    }
    setPhase("lost");
  };

  const restart = async () => {
    setPhase("restarting");
    setError(null);
    try {
      await restartBox();
    } catch (err) {
      if (!alive.current) return;
      // The server's own text (409: an update, a backup or a probation; 501: the Mac's host).
      setError(
        err instanceof ApiError && err.fromServer && err.status >= 400 && err.message
          ? err.message
          : formErrorText(err, t),
      );
      setPhase("idle");
      return;
    }
    if (!alive.current) return;
    await waitForBox();
  };

  const busy = phase === "restarting";

  return (
    <section className="card p-5">
      <h2 className="text-body font-semibold text-text mb-1">{t("settings.restart")}</h2>
      <p className="text-caption text-text-tertiary mb-4">{t("settings.restartDescription")}</p>
      <button
        type="button"
        disabled={busy}
        onClick={() => setPhase("confirming")}
        className={BUTTON_SECONDARY}
      >
        {t("settings.restart")}
      </button>
      {busy && (
        <p className="text-small text-text-secondary mt-3">{t("settings.restartWaiting")}</p>
      )}
      {phase === "lost" && (
        <p className="text-small text-error mt-3">{t("settings.restartNoAnswer")}</p>
      )}
      {error && <p className="text-small text-error mt-3">{error}</p>}
      {phase === "confirming" && (
        <Modal title={t("settings.restartConfirmTitle")} onClose={() => setPhase("idle")}>
          <div className="px-6 py-4">
            <p className="text-body text-text-secondary">{t("settings.restartConfirm")}</p>
          </div>
          <div className="flex justify-end gap-2 px-6 py-4 border-t border-border-light">
            <button type="button" onClick={() => setPhase("idle")} className={BUTTON_SECONDARY}>
              {t("common.cancel")}
            </button>
            <button type="button" onClick={restart} className={BUTTON_PRIMARY}>
              {t("settings.restartAction")}
            </button>
          </div>
        </Modal>
      )}
    </section>
  );
}
