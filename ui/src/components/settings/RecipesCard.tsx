// The Recipes card (spec 042, R5.3): the catalogue of script recipes with Install, Update and Remove,
// the files refused at start with their reason and Remove, the error of the last read, and an
// "Upload a signed recipe" button. Solite's own (Sowel's plugin store serves other packages): Sowel's
// `card` and tokens. The catalogue is read when the card opens; a 429 leaves the list held.
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  getRecipeStore,
  getResources,
  installRecipe,
  installRecipeFile,
  refreshRecipeStore,
  uninstallRecipe,
  updateRecipe,
  type RecipeStore,
  type ScriptResource,
  type StoreEntry,
} from "../../api";
import { ApiError } from "../../api/client";
import { formErrorText } from "../../lib/form-error";
import { compareVersions } from "../../lib/version";
import { BUTTON_PRIMARY, BUTTON_SECONDARY } from "../ui/Modal";

/** The server's own words for a refusal (`Not enough memory for a: ...`), else the generic text. */
function serverText(err: unknown, t: Parameters<typeof formErrorText>[1]): string {
  if (err instanceof ApiError && err.fromServer && err.status >= 400 && err.message) {
    return err.message;
  }
  return formErrorText(err, t);
}

export function RecipesCard() {
  const { t } = useTranslation();
  const [store, setStore] = useState<RecipeStore | null>(null);
  const [refused, setRefused] = useState<ScriptResource[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null);
  const upload = useRef<HTMLInputElement>(null);
  const alive = useRef(true);

  /** The files refused at start, from the resources (the catalogue may not list them). */
  const readRefused = () =>
    getResources().then(
      (r) => alive.current && setRefused(r.scripts.filter((s) => s.state === "refused")),
      () => undefined,
    );

  useEffect(() => {
    alive.current = true;
    // What the box holds first, then the read: a 429 (within a minute of the last) keeps the list.
    void getRecipeStore().then(
      (held) => alive.current && setStore(held),
      (err: unknown) => alive.current && setMessage({ text: serverText(err, t), error: true }),
    );
    void readRefused();
    void refreshRecipeStore().then(
      (fresh) => alive.current && setStore(fresh),
      () => undefined,
    );
    return () => {
      alive.current = false;
    };
  }, []);

  /** One action, then the list as the box holds it now. */
  const act = async (id: string, action: () => Promise<void>, done: string) => {
    setBusy(id);
    setMessage(null);
    try {
      await action();
      if (!alive.current) return;
      setMessage({ text: done, error: false });
    } catch (err) {
      if (!alive.current) return;
      setMessage({ text: serverText(err, t), error: true });
    }
    if (!alive.current) return;
    try {
      setStore(await getRecipeStore());
    } catch {
      // The list stays as it was.
    }
    await readRefused();
    if (alive.current) setBusy(null);
  };

  const buttons = (entry: StoreEntry) => {
    const installed = entry.installed;
    const newer = installed !== null && compareVersions(entry.version, installed) > 0;
    return (
      <div className="flex gap-2 shrink-0">
        {installed === null && !entry.builtIn && (
          <button
            type="button"
            disabled={busy !== null}
            onClick={() => act(entry.id, () => installRecipe(entry.id), t("recipeStore.installed"))}
            className={BUTTON_PRIMARY}
          >
            {t("recipeStore.install")}
          </button>
        )}
        {newer && (
          <button
            type="button"
            disabled={busy !== null}
            onClick={() => act(entry.id, () => updateRecipe(entry.id), t("recipeStore.updated"))}
            className={BUTTON_PRIMARY}
          >
            {t("recipeStore.update")}
          </button>
        )}
        {installed !== null && (
          <button
            type="button"
            disabled={busy !== null}
            onClick={() => act(entry.id, () => uninstallRecipe(entry.id), t("recipeStore.removed"))}
            className={BUTTON_SECONDARY}
          >
            {t("recipeStore.remove")}
          </button>
        )}
      </div>
    );
  };

  const chosen = (file: File | undefined) => {
    if (!file) return;
    void act("upload", () => installRecipeFile(file), t("recipeStore.uploaded"));
    if (upload.current) upload.current.value = "";
  };

  return (
    <section className="card p-5">
      <h2 className="text-body font-semibold text-text mb-1">{t("recipeStore.title")}</h2>
      <p className="text-caption text-text-tertiary mb-4">{t("recipeStore.description")}</p>
      {store?.error && (
        <p data-testid="read-error" className="text-small text-error mb-3">
          {t("recipeStore.readError", { reason: store.error })}
        </p>
      )}
      {store && store.recipes.length === 0 && !store.error && (
        <p className="text-small text-text-tertiary mb-3">{t("recipeStore.empty")}</p>
      )}
      <ul>
        {(store?.recipes ?? []).map((entry) => (
          <li
            key={entry.id}
            data-testid={`recipe-${entry.id}`}
            className="py-2 border-t border-border-light flex items-start justify-between gap-3"
          >
            <div>
              <p className="text-body text-text">
                {entry.name} <span className="text-text-tertiary">{entry.version}</span>
              </p>
              <p className="text-small text-text-secondary">{entry.description}</p>
              <p className="text-small text-text-tertiary">
                {entry.builtIn
                  ? t("recipeStore.builtIn")
                  : entry.installed
                    ? t("recipeStore.installedVersion", { version: entry.installed })
                    : t("recipeStore.notInstalled")}
              </p>
              {entry.refused && (
                <p className="text-small text-error">
                  {t("recipeStore.refused", { reason: entry.refused })}
                </p>
              )}
            </div>
            {buttons(entry)}
          </li>
        ))}
      </ul>
      {refused.map((file) => (
        <div
          key={file.id}
          data-testid={`refused-${file.id}`}
          className="py-2 border-t border-border-light flex items-start justify-between gap-3"
        >
          <div>
            <p className="text-body text-text">{file.id}</p>
            <p className="text-small text-error">
              {t("recipeStore.refused", { reason: file.reason ?? "" })}
            </p>
          </div>
          <button
            type="button"
            disabled={busy !== null}
            onClick={() => act(file.id, () => uninstallRecipe(file.id), t("recipeStore.removed"))}
            className={BUTTON_SECONDARY}
          >
            {t("recipeStore.remove")}
          </button>
        </div>
      ))}
      <div className="mt-4">
        <input
          ref={upload}
          type="file"
          accept=".srs"
          aria-label={t("recipeStore.uploadFile")}
          className="hidden"
          onChange={(event) => chosen(event.currentTarget.files?.[0])}
        />
        <button
          type="button"
          disabled={busy !== null}
          onClick={() => upload.current?.click()}
          className={BUTTON_SECONDARY}
        >
          {t("recipeStore.upload")}
        </button>
      </div>
      {message && (
        <p className={`text-small mt-3 ${message.error ? "text-error" : "text-success"}`}>
          {message.text}
        </p>
      )}
    </section>
  );
}
