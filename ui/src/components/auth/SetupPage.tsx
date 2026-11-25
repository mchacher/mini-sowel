// Ported from Sowel ui/src/pages/SetupPage.tsx (Sowel commit 5da84dc1). Changes: no router (the
// App shows it); Solite's mark; the checks are 8 to 128 characters counted in Unicode code points,
// as the host counts them (spec 029, R9.9); the language is the interface's own; the host's refusal
// (the box's button text, for example) is shown under the form.
import { useState } from "react";
import { useTranslation } from "react-i18next";
import i18n from "../../i18n";
import { formErrorText } from "../../lib/form-error";
import { useAuth } from "../../store/useAuth";
import { AuthField, AuthScreen, AuthSubmit } from "./AuthScreen";

export const PASSWORD_MIN = 8;
export const PASSWORD_MAX = 128;

export function SetupPage() {
  const setup = useAuth((s) => s.setup);
  const [username, setUsername] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const { t } = useTranslation();

  const submit = async (e: Event) => {
    e.preventDefault();
    setError("");
    if (password !== confirm) {
      setError(t("auth.passwordMismatch"));
      return;
    }
    const length = [...password].length;
    if (length < PASSWORD_MIN) {
      setError(t("auth.passwordTooShort"));
      return;
    }
    if (length > PASSWORD_MAX) {
      setError(t("auth.passwordTooLong"));
      return;
    }
    setLoading(true);
    try {
      await setup({
        username,
        password,
        displayName,
        language: i18n.language === "fr" ? "fr" : "en",
      });
    } catch (err) {
      setError(formErrorText(err, t) || t("auth.setupFailed"));
      setLoading(false);
    }
  };

  return (
    <AuthScreen subtitle={t("auth.setupTitle")}>
      <form onSubmit={submit} className="card p-6">
        <p className="text-caption text-text-tertiary mb-4">{t("auth.setupHint")}</p>
        <AuthField
          id="setup-username"
          label={t("auth.username")}
          value={username}
          onInput={setUsername}
          autoComplete="username"
          autoFocus
        />
        <AuthField
          id="setup-display-name"
          label={t("auth.displayName")}
          value={displayName}
          onInput={setDisplayName}
          autoComplete="name"
        />
        <AuthField
          id="setup-password"
          label={t("auth.password")}
          secret
          value={password}
          onInput={setPassword}
          autoComplete="new-password"
        />
        <AuthField
          id="setup-confirm"
          label={t("auth.confirmPassword")}
          secret
          value={confirm}
          onInput={setConfirm}
          autoComplete="new-password"
        />
        {error && <p className="text-small text-error mb-4">{error}</p>}
        <AuthSubmit loading={loading} disabled={!username || !displayName || !password || !confirm}>
          {t("auth.createAccount")}
        </AuthSubmit>
      </form>
    </AuthScreen>
  );
}
