// Ported from Sowel ui/src/pages/SettingsPage.tsx, `HomeSettingsSection` (Sowel commit 5da84dc1,
// UI 1.87.0), spec 026. Changes: the page holds the Home card, a Backup card and a factory reset card (spec 029); the time zone
// is a select over `GET /api/v1/settings/timezones` (D41) in place of Sowel's read-only source line;
// no restart toast (the box applies a new location at once); a "Today" line from the root zone's
// aggregation; the backup is Solite's own (D56): a password, an encrypted file, a restore at the box's restart.
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { Loader2 } from "lucide-react";
import {
  exportBackup,
  getSettings,
  getSystemVersion,
  getTimezones,
  installUpdate,
  restoreBackup,
  updateSettings,
} from "../../api";
import { useAuth } from "../../store/useAuth";
import { useZoneAggregation } from "../../store/useZoneAggregation";
import { ApiError } from "../../api/client";
import { formErrorText } from "../../lib/form-error";
import { BUTTON_PRIMARY, BUTTON_SECONDARY, FIELD_LABEL, Modal } from "../ui/Modal";

/** The root zone's id, the same in Sowel and in Solite. */
const ROOT_ZONE_ID = "00000000-0000-0000-0000-000000000001";
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
 * The text a backup call shows (spec 035, R10): the server's own words, including for a 5xx
 * (`Busy saving, try again`, `Restore failed: <reason>`), which the generic text would hide.
 */
function backupErrorText(err: unknown, t: TFunction): string {
  if (err instanceof ApiError && err.fromServer && err.status >= 500 && err.message) {
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
      await updateSettings({
        "home.name": form.homeName,
        "home.latitude": form.latitude,
        "home.longitude": form.longitude,
        "home.sunriseOffset": form.sunriseOffset,
        "home.sunsetOffset": form.sunsetOffset,
        "home.timezone": form.timezone,
      });
      setInitial(form);
      void useZoneAggregation.getState().fetchAggregation();
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

/** Spec 032, R9: the running version and the install of a signed `.sup` file. */
function UpdateSection() {
  const { t } = useTranslation();
  const [version, setVersion] = useState<string | null>(null);
  const [file, setFile] = useState<File | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null);
  const [installed, setInstalled] = useState(false);

  useEffect(() => {
    let live = true;
    getSystemVersion()
      .then((v) => live && setVersion(v.current))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, []);

  const install = async () => {
    if (!file) return;
    setBusy(true);
    setMessage(null);
    setInstalled(false);
    try {
      const r = await installUpdate(file);
      setMessage({ text: r.message, error: false });
      setInstalled(true);
    } catch (err) {
      setMessage({ text: formErrorText(err, t), error: true });
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="card p-5">
      <h2 className="text-body font-semibold text-text mb-1">{t("settings.update")}</h2>
      <p className="text-caption text-text-tertiary mb-4">{t("settings.updateDescription")}</p>
      {version && (
        <p className="text-small text-text mb-3">
          {t("settings.updateVersion")} {version}
        </p>
      )}
      <div className="flex flex-wrap items-center gap-2">
        <input
          type="file"
          accept=".sup"
          aria-label={t("settings.updateFile")}
          disabled={busy}
          className="text-small text-text-secondary"
          onChange={(e) => setFile((e.currentTarget as HTMLInputElement).files?.[0] ?? null)}
        />
        <button type="button" disabled={!file || busy} onClick={install} className={BUTTON_PRIMARY}>
          {busy ? t("settings.updateInstalling") : t("settings.updateInstall")}
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
