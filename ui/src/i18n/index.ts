// mini-Sowel's replacement for Sowel's ui/src/i18n/index.ts (i18next, its language detector and
// 67 KB of locales): a `t()` over the two small dictionaries of `messages.ts`, with the same
// default export (`i18n.language`, `i18n.t`) so that Sowel's `lib/format.ts` and `lib/api-error.ts`
// are ported unchanged. French when `navigator.language` starts with `fr`, English otherwise.
import { en, fr, type Messages } from "./messages";

export type Language = "en" | "fr";

export interface TOptions {
  /** Picks the `_one` or `_other` form of the key, and fills `{{count}}`. */
  count?: number;
  /** Shown when the key is unknown (then the key itself). */
  defaultValue?: string;
  [variable: string]: unknown;
}

export interface TFunction {
  (key: string, options?: TOptions): string;
}

const DICTIONARIES: Record<Language, Messages> = { en, fr };

let current: Language = "en";

/** The language the browser asks for. */
export function detectLanguage(): Language {
  const asked = typeof navigator === "undefined" ? "" : (navigator.language ?? "");
  return asked.toLowerCase().startsWith("fr") ? "fr" : "en";
}

/** Sets the language and `<html lang>`. */
export function setLanguage(language: Language): void {
  current = language;
  if (typeof document !== "undefined" && document.documentElement) {
    document.documentElement.lang = language;
  }
}

function interpolate(text: string, variables: Record<string, unknown>): string {
  return text.replace(/\{\{\s*(\w+)\s*\}\}/g, (whole, name: string) =>
    name in variables ? String(variables[name]) : whole,
  );
}

/** The text of a key: the plural form when `count` is given, `defaultValue`, then the key. */
export const t: TFunction = (key, options = {}) => {
  const { count, defaultValue, ...variables } = options;
  const table = DICTIONARIES[current];
  let text: string | undefined;
  if (typeof count === "number") {
    const form = new Intl.PluralRules(current).select(count);
    text = table[`${key}_${form}`] ?? table[`${key}_other`];
  }
  text ??= table[key];
  return interpolate(
    text ?? defaultValue ?? key,
    typeof count === "number" ? { ...variables, count } : variables,
  );
};

const i18n = {
  get language(): Language {
    return current;
  },
  t,
  /** i18next's `getFixedT`, for Sowel's `formatRelativeTime`: the one `t` whatever the language. */
  getFixedT: (...languages: string[]): TFunction => {
    void languages;
    return t;
  },
};

setLanguage(detectLanguage());

export default i18n;
