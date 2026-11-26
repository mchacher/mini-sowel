// Ported from Sowel ui/src/pages/SettingsPage.tsx, `HomeSettingsSection` (Sowel commit 5da84dc1,
// UI 1.87.0), spec 026. Changes: the page holds the Home card, a Backup card and a factory reset card (spec 029); the time zone
// is a select over `GET /api/v1/settings/timezones` (D41) in place of Sowel's read-only source line;
// no restart toast (the box applies a new location at once); a "Today" line from the root zone's
// aggregation; the backup is Solite's own (D56): a password, an encrypted file, a restore at the box's restart.
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { ArrowUpCircle, ExternalLink, Loader2, RefreshCw } from "lucide-react";
import {
  checkSystemVersion,
  exportBackup,
  getSettings,
  getSystemVersion,
  getTimezones,
  installUpdate,
  restoreBackup,
  saveSettings,
  triggerSystemUpdate,
  type Settings,
  type VersionInfo,
} from "../../api";
import { useAuth } from "../../store/useAuth";
import { useZoneAggregation } from "../../store/useZoneAggregation";
import { useHomeSettings } from "../../store/useHomeSettings";
import { useTimezone } from "../../store/useTimezone";
import { ROOT_ZONE_ID } from "../../zones";
import { ApiError } from "../../api/client";
import { formErrorText } from "../../lib/form-error";
import { compareVersions } from "../../lib/version";
import { BUTTON_PRIMARY, BUTTON_SECONDARY, FIELD_LABEL, Modal } from "../ui/Modal";

/** The backup password's length, in characters, as the host checks it (spec 035, R1). */
const BACKUP_PASSWORD_MIN = 8;
const BACKUP_PASSWORD_MAX = 128;

const INPUT =
  "w-full px-3 py-2 text-body bg-background border border-border rounded-sm text-text placeholder:text-text-tertiary focus:outline-none focus:border-primary";

interface Form {
  homeName: string;
  latitude: string;
  longitude: string;
  sunriseOffset: string;
  sunsetOffset: string;
  timezone: string;
}

const EMPTY: Form = {
  homeName: "",
  latitude: "",
  longitude: "",
  sunriseOffset: "30",
  sunsetOffset: "45",
  timezone: "UTC",
};

function formOf(all: Record<string, string>): Form {
  return {
    homeName: all["home.name"] ?? "",
    latitude: (all["home.latitude"] ?? "").replace(",", "."),
    longitude: (all["home.longitude"] ?? "").replace(",", "."),
    sunriseOffset: all["home.sunriseOffset"] ?? "30",
    sunsetOffset: all["home.sunsetOffset"] ?? "45",
    timezone: all["home.timezone"] ?? "UTC",
  };
}

/**
 * The text a backup call shows (spec 035, R10): the server's own words for a 5xx (`Busy saving, try
 * again`, `Restore failed: <reason>`), which the generic text would hide.
 */
function backupErrorText(err: unknown, t: TFunction): string {
  if (err instanceof ApiError && err.fromServer && err.status >= 500 && err.message) {
    return err.message;
  }
  return formErrorText(err, t);
}

/**
 * The text an update call shows (spec 040 R4.3): the server's own words for a 4xx or a 5xx
 * (`Please wait 12s before checking again`, `Download failed: <reason>`); the client's own text
 * only when the server gave none.
 */
function serverErrorText(err: unknown, t: TFunction): string {
  if (err instanceof ApiError && err.fromServer && err.status >= 400 && err.message) {
    return err.message;
  }
  return formErrorText(err, t);
}

export function SettingsPage() {
  const { t } = useTranslation();
  return (
    <div>
      <h1 className="text-display font-semibold text-text mb-4">{t("settings.title")}</h1>
      <div className="space-y-6 max-w-[640px]">
        <HomeSettingsSection />
        <BackupSection />
        <UpdateSection />
        <FactoryResetSection />
      </div>
    </div>
  );
}

function HomeSettingsSection() {
  const { t } = useTranslation();
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [form, setForm] = useState<Form>(EMPTY);
  const [initial, setInitial] = useState<Form>(EMPTY);
  const [timezones, setTimezones] = useState<string[]>(["UTC"]);
  const root = useZoneAggregation((s) => s.data[ROOT_ZONE_ID]);

  // Read again after an import (the page below dispatches this event).
  const load = () =>
    Promise.all([getSettings(), getTimezones()])
      .then(([all, zones]) => {
        const next = formOf(all);
        setTimezones(zones);
        setForm(next);
        setInitial(next);
        setError(null);
      })
      .catch((err) => setError(formErrorText(err, t)))
      .finally(() => setLoading(false));

  useEffect(() => {
    void load();
  }, []);

  const set = (key: keyof Form, value: string) => setForm((f) => ({ ...f, [key]: value }));
  const dirty = (Object.keys(form) as (keyof Form)[]).some((k) => form[k] !== initial[k]);

  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      await saveSettings({
        "home.name": form.homeName,
        "home.latitude": form.latitude,
        "home.longitude": form.longitude,
        "home.sunriseOffset": form.sunriseOffset,
        "home.sunsetOffset": form.sunsetOffset,
        "home.timezone": form.timezone,
      });
      setInitial(form);
      void useZoneAggregation.getState().fetchAggregation();
      // The top bar shows the name, the location hint and the home's time: read them again.
      void useHomeSettings.getState().fetch();
      void useTimezone.getState().fetch();
    } catch (err) {
      setError(formErrorText(err, t));
    } finally {
      setSaving(false);
    }
  };

  if (loading) {
    return (
      <section className="card p-5">
        <h2 className="text-body font-semibold text-text mb-4">{t("settings.home")}</h2>
        <Loader2 size={16} className="animate-spin text-text-tertiary" />
      </section>
    );
  }

  const zones = timezones.includes(form.timezone) ? timezones : [form.timezone, ...timezones];
  const hasSun = root != null && root.sunrise != null && root.sunset != null;

  return (
    <section className="card p-5">
      <h2 className="text-body font-semibold text-text mb-1">{t("settings.home")}</h2>
      <p className="text-caption text-text-tertiary mb-4">{t("settings.homeDescription")}</p>
      <div className="space-y-3">
        <div>
          <label htmlFor="home-name" className={FIELD_LABEL}>
            {t("settings.homeName")}
          </label>
          <input
            id="home-name"
            type="text"
            value={form.homeName}
            onChange={(e) => set("homeName", e.currentTarget.value)}
            placeholder={t("settings.homeNamePlaceholder")}
            className={INPUT}
          />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <div>
            <label htmlFor="home-latitude" className={FIELD_LABEL}>
              {t("settings.latitude")}
            </label>
            <input
              id="home-latitude"
              type="text"
              inputMode="decimal"
              value={form.latitude}
              onChange={(e) => set("latitude", e.currentTarget.value.replace(",", "."))}
              placeholder="48.8566"
              className={INPUT}
            />
          </div>
          <div>
            <label htmlFor="home-longitude" className={FIELD_LABEL}>
              {t("settings.longitude")}
            </label>
            <input
              id="home-longitude"
              type="text"
              inputMode="decimal"
              value={form.longitude}
              onChange={(e) => set("longitude", e.currentTarget.value.replace(",", "."))}
              placeholder="2.3522"
              className={INPUT}
            />
          </div>
        </div>
        <div className="grid grid-cols-2 gap-3">
          <div>
            <label htmlFor="home-sunrise" className={FIELD_LABEL}>
              {t("settings.sunriseOffset")}
            </label>
            <input
              id="home-sunrise"
              type="number"
              value={form.sunriseOffset}
              onChange={(e) => set("sunriseOffset", e.currentTarget.value)}
              className={INPUT}
            />
            <p className="text-caption text-text-tertiary mt-1">
              {t("settings.sunriseOffsetHelp")}
            </p>
          </div>
          <div>
            <label htmlFor="home-sunset" className={FIELD_LABEL}>
              {t("settings.sunsetOffset")}
            </label>
            <input
              id="home-sunset"
              type="number"
              value={form.sunsetOffset}
              onChange={(e) => set("sunsetOffset", e.currentTarget.value)}
              className={INPUT}
            />
            <p className="text-caption text-text-tertiary mt-1">{t("settings.sunsetOffsetHelp")}</p>
          </div>
        </div>
        <div className="pt-3 border-t border-border-light">
          <label htmlFor="home-timezone" className={FIELD_LABEL}>
            {t("settings.timezone")}
          </label>
          <select
            id="home-timezone"
            value={form.timezone}
            onChange={(e) => set("timezone", e.currentTarget.value)}
            className={INPUT}
          >
            {zones.map((zone) => (
              <option key={zone} value={zone}>
                {zone}
              </option>
            ))}
          </select>
        </div>
        <div className="pt-3 border-t border-border-light">
          <h3 className="text-small font-medium text-text-secondary mb-1">{t("settings.today")}</h3>
          {hasSun ? (
            <p className="text-small text-text">
              {t("settings.sunrise")} {root.sunrise} · {t("settings.sunset")} {root.sunset} ·{" "}
              {root.isDaylight ? t("settings.day") : t("settings.night")}
            </p>
          ) : (
            <p className="text-caption text-text-tertiary">{t("settings.sunHint")}</p>
          )}
        </div>
        {error && <p className="text-small text-error">{error}</p>}
        {dirty && (
          <button onClick={save} disabled={saving} className={BUTTON_PRIMARY}>
            {saving ? t("common.saving") : t("common.save")}
          </button>
        )}
      </div>
    </section>
  );
}

/** `YYYY-MM-DD`, the local date, for the name of the exported file. */
function today(): string {
  const d = new Date();
  const two = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())}`;
}

/** Spec 035, R10: the full backup, exported with a password and restored with it. */
function BackupSection() {
  const { t } = useTranslation();
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [exporting, setExporting] = useState(false);
  const [exportMessage, setExportMessage] = useState<string | null>(null);
  const [file, setFile] = useState<File | null>(null);
  const [restorePassword, setRestorePassword] = useState("");
  const [wifi, setWifi] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [restoring, setRestoring] = useState(false);
  const [restoreMessage, setRestoreMessage] = useState<{ text: string; error: boolean } | null>(
    null,
  );

  const length = [...password].length;
  const reason =
    length < BACKUP_PASSWORD_MIN
      ? t("auth.passwordTooShort")
      : length > BACKUP_PASSWORD_MAX
        ? t("auth.passwordTooLong")
        : password !== confirm
          ? t("auth.passwordMismatch")
          : null;

  const doExport = async () => {
    setExporting(true);
    setExportMessage(null);
    try {
      const blob = await exportBackup(password);
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = `solite-backup-${today()}.slb`;
      // In the page while clicked, and revoked later: Safari may abort a download otherwise.
      document.body.appendChild(link);
      link.click();
      link.remove();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
      setPassword("");
      setConfirm("");
    } catch (err) {
      setExportMessage(backupErrorText(err, t));
    } finally {
      setExporting(false);
    }
  };

  const doRestore = async () => {
    if (!file) return;
    setConfirming(false);
    setRestoring(true);
    setRestoreMessage(null);
    try {
      const r = await restoreBackup(file, restorePassword, wifi);
      setRestoreMessage({ text: r.message, error: false });
      setRestorePassword("");
    } catch (err) {
      setRestoreMessage({ text: backupErrorText(err, t), error: true });
    } finally {
      setRestoring(false);
    }
  };

  return (
    <section className="card p-5">
      <h2 className="text-body font-semibold text-text mb-1">{t("settings.backup")}</h2>
      <p className="text-caption text-text-tertiary mb-4">{t("settings.backupDescription")}</p>

      <h3 className="text-small font-semibold text-text mb-2">{t("settings.backupExport")}</h3>
      <div className="grid gap-3 sm:grid-cols-2 mb-2">
        <div>
          <label htmlFor="backup-password" className={FIELD_LABEL}>
            {t("settings.backupPassword")}
          </label>
          <input
            id="backup-password"
            type="password"
            autoComplete="new-password"
            value={password}
            disabled={exporting}
            className={INPUT}
            onInput={(e) => setPassword((e.currentTarget as HTMLInputElement).value)}
          />
        </div>
        <div>
          <label htmlFor="backup-confirm" className={FIELD_LABEL}>
            {t("settings.backupPasswordConfirm")}
          </label>
          <input
            id="backup-confirm"
            type="password"
            autoComplete="new-password"
            value={confirm}
            disabled={exporting}
            className={INPUT}
            onInput={(e) => setConfirm((e.currentTarget as HTMLInputElement).value)}
          />
        </div>
      </div>
      <p className="text-caption text-text-tertiary mb-2">{t("settings.backupPasswordHint")}</p>
      <button
        type="button"
        disabled={reason !== null || exporting}
        onClick={doExport}
        className={BUTTON_SECONDARY}
      >
        {exporting ? t("settings.backupExporting") : t("settings.export")}
      </button>
      {reason !== null && (password !== "" || confirm !== "") && (
        <p className="text-small text-text-secondary mt-2">{reason}</p>
      )}
      {exportMessage && <p className="text-small text-error mt-2">{exportMessage}</p>}

      <h3 className="text-small font-semibold text-text mt-6 mb-2">
        {t("settings.backupRestore")}
      </h3>
      <div className="grid gap-3 sm:grid-cols-2 mb-3">
        <div>
          <label htmlFor="backup-file" className={FIELD_LABEL}>
            {t("settings.backupFile")}
          </label>
          <input
            id="backup-file"
            type="file"
            accept=".slb"
            disabled={restoring}
            className="text-small text-text-secondary"
            onChange={(e) => setFile((e.currentTarget as HTMLInputElement).files?.[0] ?? null)}
          />
        </div>
        <div>
          <label htmlFor="backup-restore-password" className={FIELD_LABEL}>
            {t("settings.backupRestorePassword")}
          </label>
          <input
            id="backup-restore-password"
            type="password"
            autoComplete="off"
            value={restorePassword}
            disabled={restoring}
            className={INPUT}
            onInput={(e) => setRestorePassword((e.currentTarget as HTMLInputElement).value)}
          />
        </div>
      </div>
      <label className="flex items-center gap-2 text-small text-text mb-3">
        <input
          type="checkbox"
          checked={wifi}
          disabled={restoring}
          onChange={(e) => setWifi((e.currentTarget as HTMLInputElement).checked)}
        />
        {t("settings.backupWifi")}
      </label>
      <button
        type="button"
        disabled={!file || restorePassword === "" || restoring}
        onClick={() => setConfirming(true)}
        className={BUTTON_SECONDARY}
      >
        {restoring ? t("settings.backupRestoring") : t("settings.backupRestoreAction")}
      </button>
      {restoreMessage && (
        <p className={`text-small mt-3 ${restoreMessage.error ? "text-error" : "text-success"}`}>
          {restoreMessage.text}
        </p>
      )}
      {confirming && (
        <Modal title={t("settings.backupConfirmTitle")} onClose={() => setConfirming(false)}>
          <div className="px-6 py-4">
            <p className="text-body text-text-secondary">{t("settings.backupConfirm")}</p>
            {wifi && (
              <p className="text-body text-text-secondary mt-2">
                {t("settings.backupConfirmWifi")}
              </p>
            )}
          </div>
          <div className="flex justify-end gap-2 px-6 py-4 border-t border-border-light">
            <button type="button" onClick={() => setConfirming(false)} className={BUTTON_SECONDARY}>
              {t("common.cancel")}
            </button>
            <button type="button" onClick={doRestore} className={BUTTON_PRIMARY}>
              {t("settings.backupRestoreAction")}
            </button>
          </div>
        </Modal>
      )}
    </section>
  );
}

/** The polling of the update card (spec 040, R4.6): 3 s between two polls, 5 s for one, 3 min or 11 min in all. */
const POLL_GAP_MS = 3000;
const POLL_TIMEOUT_MS = 5000;
const WAIT_AFTER_ANSWER_MS = 3 * 60 * 1000;
const WAIT_AFTER_LOST_MS = 11 * 60 * 1000;

type Notice = { text: string; error: boolean };

/**
 * Spec 040, R4: the version, the check, the pull of a release and the install of a signed `.sup`
 * file (spec 032, R9). Modelled on Sowel's `VersionBadge` and `UpdateOverlay`.
 */
function UpdateSection() {
  const { t } = useTranslation();
  const [info, setInfo] = useState<VersionInfo | null>(null);
  const [settings, setSettings] = useState<Settings>({});
  const [manifest, setManifest] = useState("");
  const [settingsNotice, setSettingsNotice] = useState<Notice | null>(null);
  const [checking, setChecking] = useState(false);
  const [phase, setPhase] = useState<"idle" | "pulling" | "waiting">("idle");
  const [elapsed, setElapsed] = useState(0);
  const [confirming, setConfirming] = useState(false);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [file, setFile] = useState<File | null>(null);
  const [uploading, setUploading] = useState(false);
  const [message, setMessage] = useState<Notice | null>(null);
  const [installed, setInstalled] = useState(false);
  const alive = useRef(true);
  const poll = useRef<AbortController | null>(null);

  useEffect(() => {
    alive.current = true;
    getSystemVersion()
      .then((v) => alive.current && setInfo(v))
      .catch(() => {});
    getSettings()
      .then((all) => {
        if (!alive.current) return;
        setSettings(all);
        setManifest(all["update.manifestUrl"] ?? "");
      })
      .catch(() => {});
    // The card leaves (the 401 after the restart sends to the login screen): no further poll.
    return () => {
      alive.current = false;
      poll.current?.abort();
    };
  }, []);

  // The elapsed count of the install: seconds since the click, not a countdown.
  useEffect(() => {
    if (phase !== "pulling") return;
    const started = Date.now();
    setElapsed(0);
    const id = setInterval(() => setElapsed(Math.floor((Date.now() - started) / 1000)), 1000);
    return () => clearInterval(id);
  }, [phase]);

  const pulling = phase !== "idle";
  const locked = pulling || uploading;

  const check = async () => {
    setNotice(null);
    setChecking(true);
    try {
      setInfo(await checkSystemVersion());
    } catch (err) {
      setNotice({ text: serverErrorText(err, t), error: true });
    } finally {
      setChecking(false);
    }
  };

  /** Polls the version until it changes or the time is up; the ending is chosen from the last poll. */
  const watch = async (before: string, limitMs: number, minutes: number, endKey: string) => {
    const started = Date.now();
    let lastSame: boolean;
    for (;;) {
      await new Promise((resolve) => setTimeout(resolve, POLL_GAP_MS));
      if (!alive.current) return;
      const controller = new AbortController();
      poll.current = controller;
      const timer = setTimeout(() => controller.abort(), POLL_TIMEOUT_MS);
      try {
        const v = await getSystemVersion(controller.signal);
        if (!alive.current) return;
        setInfo(v);
        if (v.current !== before) {
          setNotice({ text: t("settings.updateNowRunning", { version: v.current }), error: false });
          setPhase("idle");
          return;
        }
        lastSame = true;
      } catch {
        if (!alive.current) return;
        lastSame = false; // a network error, a timeout or a 5xx: the box is down or busy
      } finally {
        clearTimeout(timer);
      }
      if (Date.now() - started >= limitMs) {
        const text = lastSame
          ? t(endKey, { version: before })
          : t("settings.updateNoAnswer", { minutes });
        setNotice({ text, error: true });
        setPhase("idle");
        return;
      }
    }
  };

  const pull = async () => {
    if (!info) return;
    setConfirming(false);
    setNotice(null);
    setPhase("pulling");
    const before = info.current;
    try {
      await triggerSystemUpdate();
    } catch (err) {
      if (!alive.current) return;
      if (err instanceof ApiError && err.status === 0) {
        // The connection dropped; the pull may still run in the box (up to 600 s) and restart it.
        setNotice({ text: t("settings.updateLost"), error: false });
        setPhase("waiting");
        await watch(before, WAIT_AFTER_LOST_MS, 11, "settings.updateNotComplete");
      } else {
        setNotice({ text: serverErrorText(err, t), error: true });
        setPhase("idle");
      }
      return;
    }
    if (!alive.current) return;
    setNotice({
      text: t("settings.updateInstalledWait", { version: info.latest ?? "" }),
      error: false,
    });
    setPhase("waiting");
    await watch(before, WAIT_AFTER_ANSWER_MS, 3, "settings.updateNotKept");
  };

  const saveSetting = async (changes: Settings) => {
    setSettingsNotice(null);
    try {
      await saveSettings(changes);
      setSettings((s) => ({ ...s, ...changes }));
    } catch (err) {
      setSettingsNotice({ text: serverErrorText(err, t), error: true });
    }
  };

  const install = async () => {
    if (!file) return;
    setUploading(true);
    setMessage(null);
    setInstalled(false);
    try {
      const r = await installUpdate(file);
      setMessage({ text: r.message, error: false });
      setInstalled(true);
    } catch (err) {
      setMessage({ text: formErrorText(err, t), error: true });
    } finally {
      setUploading(false);
    }
  };

  const newer = info?.latest != null && compareVersions(info.latest, info.current) > 0;
  const notes = info?.releaseUrl ? (
    <a
      href={info.releaseUrl}
      target="_blank"
      rel="noopener noreferrer"
      className="inline-flex items-center gap-1 text-small text-primary hover:underline"
    >
      {t("settings.updateNotes")}
      <ExternalLink size={14} strokeWidth={1.5} />
    </a>
  ) : null;

  return (
    <section className="card p-5">
      <h2 className="text-body font-semibold text-text mb-1">{t("settings.update")}</h2>
      <p className="text-caption text-text-tertiary mb-4">{t("settings.updateDescription")}</p>
      {info && (
        <div className="mb-3 space-y-1">
          <p className="text-small text-text">
            {t("settings.updateVersion")} {info.current}
          </p>
          {info.latest === null ? (
            <p className="text-small text-text-secondary">{t("settings.updateNoCheck")}</p>
          ) : info.updateAvailable ? (
            <p className="text-small text-text-secondary flex flex-wrap items-center gap-x-3">
              <span className="inline-flex items-center gap-1">
                <ArrowUpCircle size={14} strokeWidth={1.5} className="text-primary" />
                {t("settings.updateAvailable", { version: info.latest })}
              </span>
              {notes}
            </p>
          ) : newer ? (
            <p className="text-small text-text-secondary flex flex-wrap items-center gap-x-3">
              <span>{t("settings.updateBlocked", { version: info.latest })}</span>
              {notes}
            </p>
          ) : (
            <p className="text-small text-text-secondary">
              {t("settings.updateUpToDate", { version: info.latest })}
            </p>
          )}
        </div>
      )}
      <div className="flex flex-wrap items-center gap-2">
        {info?.updateAvailable && info.latest && (
          <button
            type="button"
            disabled={locked || checking}
            onClick={() => setConfirming(true)}
            className={BUTTON_PRIMARY}
          >
            {t("settings.updateTo", { version: info.latest })}
          </button>
        )}
        <button
          type="button"
          disabled={locked || checking}
          onClick={check}
          className={`${BUTTON_SECONDARY} inline-flex items-center gap-2 disabled:opacity-50 disabled:cursor-not-allowed`}
        >
          {checking ? (
            <Loader2 size={16} className="animate-spin" />
          ) : (
            <RefreshCw size={16} strokeWidth={1.5} />
          )}
          {t("update.checkNow")}
        </button>
      </div>
      {phase === "pulling" && (
        <p className="text-small text-text-secondary mt-3 flex items-center gap-2" role="status">
          <Loader2 size={16} className="animate-spin text-text-tertiary" />
          {t("settings.updateProgress", { seconds: elapsed })}
        </p>
      )}
      {notice && (
        <p
          className={`text-small mt-3 flex items-center gap-2 ${notice.error ? "text-error" : "text-text-secondary"}`}
          role="status"
        >
          {phase === "waiting" && <Loader2 size={16} className="animate-spin text-text-tertiary" />}
          {notice.text}
        </p>
      )}
      <label className="flex items-center gap-2 mt-4 text-small text-text">
        <input
          type="checkbox"
          checked={settings["update.autoCheck"] === "true"}
          onChange={(e) =>
            void saveSetting({
              "update.autoCheck": (e.currentTarget as HTMLInputElement).checked ? "true" : "false",
            })
          }
        />
        {t("settings.updateAutoCheck")}
      </label>
      <details className="mt-3">
        <summary className="text-small text-text-secondary cursor-pointer">
          {t("settings.updateAdvanced")}
        </summary>
        <div className="mt-2">
          <label htmlFor="update-manifest-url" className={FIELD_LABEL}>
            {t("settings.updateManifestUrl")}
          </label>
          <div className="flex gap-2">
            <input
              id="update-manifest-url"
              type="url"
              value={manifest}
              placeholder="https://github.com/mchacher/solite/releases/latest/download/manifest.json"
              className={INPUT}
              onInput={(e) => setManifest((e.currentTarget as HTMLInputElement).value)}
            />
            <button
              type="button"
              aria-label={t("settings.updateManifestSave")}
              disabled={manifest === (settings["update.manifestUrl"] ?? "")}
              onClick={() => void saveSetting({ "update.manifestUrl": manifest.trim() })}
              className={BUTTON_PRIMARY}
            >
              {t("common.save")}
            </button>
          </div>
          <p className="text-caption text-text-tertiary mt-1">{t("settings.updateManifestHelp")}</p>
        </div>
      </details>
      {settingsNotice && <p className="text-small text-error mt-3">{settingsNotice.text}</p>}
      <h3 className="text-small font-semibold text-text mt-5 mb-2">
        {t("settings.updateFileHeading")}
      </h3>
      <div className="flex flex-wrap items-center gap-2">
        <input
          type="file"
          accept=".sup"
          aria-label={t("settings.updateFile")}
          disabled={locked}
          className="text-small text-text-secondary"
          onChange={(e) => setFile((e.currentTarget as HTMLInputElement).files?.[0] ?? null)}
        />
        <button
          type="button"
          disabled={!file || locked}
          onClick={install}
          className={BUTTON_SECONDARY}
        >
          {uploading ? t("settings.updateInstalling") : t("settings.updateInstall")}
        </button>
      </div>
      {message && (
        <p className={`text-small mt-3 ${message.error ? "text-error" : "text-success"}`}>
          {message.text}
        </p>
      )}
      {installed && (
        <p className="text-small text-text-secondary mt-1">{t("settings.updateRestart")}</p>
      )}
      {confirming && info?.latest && (
        <Modal
          title={t("update.confirmTitle", { version: info.latest })}
          onClose={() => setConfirming(false)}
        >
          <div className="px-6 py-4">
            <p className="text-body text-text-secondary">
              {t("settings.updateConfirm", { version: info.latest })}
            </p>
          </div>
          <div className="flex justify-end gap-2 px-6 py-4 border-t border-border-light">
            <button type="button" onClick={() => setConfirming(false)} className={BUTTON_SECONDARY}>
              {t("common.cancel")}
            </button>
            <button type="button" onClick={pull} className={BUTTON_PRIMARY}>
              {t("update.confirmAction")}
            </button>
          </div>
        </Modal>
      )}
    </section>
  );
}

/** Solite's own (spec 029, R9.7): erases the home and the password; the host may ask for the box's button. */
function FactoryResetSection() {
  const { t } = useTranslation();
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reset = async () => {
    setConfirming(false);
    setBusy(true);
    setError(null);
    try {
      // Success: the store clears the tokens and the App shows the first-start screen.
      await useAuth.getState().factoryReset();
    } catch (err) {
      setError(formErrorText(err, t));
      setBusy(false);
    }
  };

  return (
    <section className="card p-5">
      <h2 className="text-body font-semibold text-text mb-1">{t("settings.factoryReset")}</h2>
      <p className="text-caption text-text-tertiary mb-4">
        {t("settings.factoryResetDescription")}
      </p>
      <button
        type="button"
        disabled={busy}
        onClick={() => setConfirming(true)}
        className={BUTTON_SECONDARY}
      >
        {t("settings.factoryReset")}
      </button>
      {error && <p className="text-small text-error mt-3">{error}</p>}
      {confirming && (
        <Modal title={t("settings.factoryResetConfirmTitle")} onClose={() => setConfirming(false)}>
          <div className="px-6 py-4">
            <p className="text-body text-text-secondary">{t("settings.factoryResetConfirm")}</p>
          </div>
          <div className="flex justify-end gap-2 px-6 py-4 border-t border-border-light">
            <button type="button" onClick={() => setConfirming(false)} className={BUTTON_SECONDARY}>
              {t("common.cancel")}
            </button>
            <button type="button" onClick={reset} className={BUTTON_PRIMARY}>
              {t("settings.factoryResetAction")}
            </button>
          </div>
        </Modal>
      )}
    </section>
  );
}
