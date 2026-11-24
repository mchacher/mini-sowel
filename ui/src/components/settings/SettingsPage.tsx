// Ported from Sowel ui/src/pages/SettingsPage.tsx, `HomeSettingsSection` (Sowel commit 5da84dc1,
// UI 1.87.0), spec 026. Changes: the page holds the Home card and a Backup card only; the time zone
// is a select over `GET /api/v1/settings/timezones` (D41) in place of Sowel's read-only source line;
// no restart toast (the box applies a new location at once); a "Today" line from the root zone's
// aggregation; the backup is Solite's own (D42).
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Loader2 } from "lucide-react";
import {
  exportBackup,
  getSettings,
  getTimezones,
  importBackup,
  updateSettings,
  type ImportResult,
} from "../../api";
import { useZoneAggregation } from "../../store/useZoneAggregation";
import { useZones } from "../../store/useZones";
import { formErrorText } from "../../lib/form-error";
import { BUTTON_PRIMARY, BUTTON_SECONDARY, FIELD_LABEL, Modal } from "../ui/Modal";

/** The root zone's id, the same in Sowel and in Solite. */
const ROOT_ZONE_ID = "00000000-0000-0000-0000-000000000001";
/** The API's body bound (`MAX_BODY_BYTES`): a larger file is refused before it is sent. */
export const MAX_BACKUP_BYTES = 16384;

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

export function SettingsPage() {
  const { t } = useTranslation();
  return (
    <div>
      <h1 className="text-display font-semibold text-text mb-4">{t("settings.title")}</h1>
      <div className="space-y-6 max-w-[640px]">
        <HomeSettingsSection />
        <BackupSection />
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
    window.addEventListener("solite:settings-imported", load);
    return () => window.removeEventListener("solite:settings-imported", load);
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

function BackupSection() {
  const { t } = useTranslation();
  const input = useRef<HTMLInputElement>(null);
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null);
  const [pending, setPending] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const doExport = async () => {
    setMessage(null);
    try {
      const text = await exportBackup();
      const url = URL.createObjectURL(new Blob([text], { type: "application/json" }));
      const link = document.createElement("a");
      link.href = url;
      link.download = `solite-backup-${today()}.json`;
      // In the page while clicked, and revoked later: Safari may abort a download otherwise.
      document.body.appendChild(link);
      link.click();
      link.remove();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch (err) {
      setMessage({ text: formErrorText(err, t), error: true });
    }
  };

  const pick = async (e: Event) => {
    const field = e.currentTarget as HTMLInputElement;
    const file = field.files?.[0];
    field.value = "";
    if (!file) return;
    setMessage(null);
    if (file.size > MAX_BACKUP_BYTES) {
      setMessage({ text: t("settings.fileTooLarge"), error: true });
      return;
    }
    setPending(await file.text());
  };

  const doImport = async () => {
    const text = pending;
    if (text === null) return;
    setPending(null);
    setBusy(true);
    try {
      const r: ImportResult = await importBackup(text);
      setMessage({
        text: t("settings.imported", {
          settings: r.settings,
          created: r.zonesCreated,
          updated: r.zonesUpdated,
        }),
        error: false,
      });
      void useZones.getState().fetchZones();
      window.dispatchEvent(new Event("solite:settings-imported"));
      void useZoneAggregation.getState().fetchAggregation();
    } catch (err) {
      setMessage({ text: formErrorText(err, t), error: true });
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="card p-5">
      <h2 className="text-body font-semibold text-text mb-1">{t("settings.backup")}</h2>
      <p className="text-caption text-text-tertiary mb-4">{t("settings.backupDescription")}</p>
      <div className="flex flex-wrap gap-2">
        <button type="button" onClick={doExport} className={BUTTON_SECONDARY}>
          {t("settings.export")}
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={() => input.current?.click()}
          className={BUTTON_SECONDARY}
        >
          {t("settings.import")}
        </button>
        <input
          ref={input}
          type="file"
          accept=".json,application/json"
          aria-label={t("settings.import")}
          className="hidden"
          onChange={pick}
        />
      </div>
      {message && (
        <p className={`text-small mt-3 ${message.error ? "text-error" : "text-text-secondary"}`}>
          {message.text}
        </p>
      )}
      {pending !== null && (
        <Modal title={t("settings.importConfirmTitle")} onClose={() => setPending(null)}>
          <div className="px-6 py-4">
            <p className="text-body text-text-secondary">{t("settings.importConfirm")}</p>
          </div>
          <div className="flex justify-end gap-2 px-6 py-4 border-t border-border-light">
            <button type="button" onClick={() => setPending(null)} className={BUTTON_SECONDARY}>
              {t("common.cancel")}
            </button>
            <button type="button" onClick={doImport} className={BUTTON_PRIMARY}>
              {t("settings.import")}
            </button>
          </div>
        </Modal>
      )}
    </section>
  );
}
