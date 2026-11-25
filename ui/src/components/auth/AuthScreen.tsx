// Solite's own file (spec 029): the frame of the first-start and login screens, from the markup
// of Sowel's SetupPage and LoginPage (centred column, logo, one card).
import type { ComponentChildren } from "preact";
import { Loader2 } from "lucide-react";
import { FIELD_LABEL } from "../ui/Modal";
import { SoliteLogo } from "../layout/SoliteLogo";

export const AUTH_INPUT =
  "w-full px-3 py-2 text-body bg-background border border-border rounded-sm text-text placeholder:text-text-tertiary focus:outline-none focus:border-primary";

export function AuthScreen({
  subtitle,
  children,
}: {
  subtitle?: string;
  children: ComponentChildren;
}) {
  return (
    <div className="flex items-center justify-center min-h-dvh bg-background">
      <div className="w-full max-w-sm mx-4">
        <div className="text-center mb-8">
          <SoliteLogo size={96} showWordmark animated className="inline-block mb-2" />
          {subtitle && <p className="text-small text-text-secondary mt-2">{subtitle}</p>}
        </div>
        {children}
      </div>
    </div>
  );
}

export function AuthField({
  id,
  label,
  secret,
  value,
  onInput,
  autoComplete,
  autoFocus,
}: {
  id: string;
  label: string;
  /** A password field (its characters are hidden). */
  secret?: boolean;
  value: string;
  onInput: (value: string) => void;
  autoComplete: string;
  autoFocus?: boolean;
}) {
  const common = {
    id,
    value,
    onInput: (e: { currentTarget: HTMLInputElement }) => onInput(e.currentTarget.value),
    className: AUTH_INPUT,
    autoFocus,
    autoComplete,
  };
  return (
    <div className="mb-4">
      <label htmlFor={id} className={FIELD_LABEL}>
        {label}
      </label>
      {secret ? <input type="password" {...common} /> : <input type="text" {...common} />}
    </div>
  );
}

export function AuthSubmit({
  loading,
  disabled,
  children,
}: {
  loading: boolean;
  disabled: boolean;
  children: ComponentChildren;
}) {
  return (
    <button
      type="submit"
      disabled={loading || disabled}
      className="w-full flex items-center justify-center gap-2 px-4 py-2.5 bg-primary-fill text-white text-body font-medium rounded-sm hover:bg-primary-fill-hover transition-colors duration-150 disabled:opacity-50"
    >
      {loading && <Loader2 size={16} className="animate-spin" />}
      {children}
    </button>
  );
}
