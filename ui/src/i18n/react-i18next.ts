// What the ported Sowel files import from `react-i18next` and `i18next`: Vite and TypeScript alias
// both packages to this module (R9.4 of spec 014), so the ported files stay unchanged.
import i18n, { t, type TFunction } from "./index";

export type { TFunction };

export function useTranslation(): { t: TFunction; i18n: typeof i18n } {
  return { t, i18n };
}
