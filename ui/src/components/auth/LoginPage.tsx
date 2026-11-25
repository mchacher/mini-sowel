// Ported from Sowel ui/src/pages/LoginPage.tsx (Sowel commit 5da84dc1), without the second factor
// and the trusted device; no router (the App shows it).
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { formErrorText } from "../../lib/form-error";
import { useAuth } from "../../store/useAuth";
import { AuthField, AuthScreen, AuthSubmit } from "./AuthScreen";

export function LoginPage() {
  const login = useAuth((s) => s.login);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const { t } = useTranslation();

  const submit = async (e: Event) => {
    e.preventDefault();
    setError("");
    setLoading(true);
    try {
      await login(username, password);
    } catch (err) {
      setError(formErrorText(err, t) || t("auth.loginFailed"));
    } finally {
      setLoading(false);
    }
  };

  return (
    <AuthScreen>
      <form onSubmit={submit} className="card p-6">
        <AuthField
          id="login-username"
          label={t("auth.username")}
          value={username}
          onInput={setUsername}
          autoComplete="username"
          autoFocus
        />
        <AuthField
          id="login-password"
          label={t("auth.password")}
          secret
          value={password}
          onInput={setPassword}
          autoComplete="current-password"
        />
        {error && <p className="text-small text-error mb-4">{error}</p>}
        <AuthSubmit loading={loading} disabled={!username || !password}>
          {t("auth.login")}
        </AuthSubmit>
      </form>
    </AuthScreen>
  );
}
