// Ported from Sowel ui/src/components/layout/SunTimes.tsx (Sowel commit 2d360726). Unchanged, but for
// `SunLocationHint` (Solite's, D62: the link in the place of the sun times when no location is set,
// spec 037 R2) and `lg` in the comment of `MobileSunTimes`, which is `md` here.
import { useTranslation } from "react-i18next";
import { Moon, Sun } from "lucide-react";
import type { ZoneAggregatedData } from "../../types";
import { formatRange } from "../../lib/format";

/**
 * Sunrise and sunset of the home, as plain text in the top bar on every page
 * (spec 196, decision pr6-soleil, amended by #1133 for phones and tablets): a sun
 * by day, a moon by night, and « 07:22 – 17:17 ».
 */
export function SunTimes({ data }: { data: ZoneAggregatedData | undefined }) {
  const { t } = useTranslation();

  if (!data || data.sunrise === null || data.sunset === null) return null;

  const isDay = data.isDaylight === true;
  const range = formatRange(data.sunrise, data.sunset);

  return (
    <div
      className="flex items-center gap-1 text-caption text-text-secondary tabular-nums whitespace-nowrap"
      title={`${t("header.sun")} ${range}`}
    >
      {isDay ? (
        // The sun glyph is the one tolerated amber outside "on" (spec 196).
        <Sun size={14} strokeWidth={1.5} className="text-active" aria-hidden="true" />
      ) : (
        <Moon size={14} strokeWidth={1.5} aria-hidden="true" />
      )}
      <span className="sr-only">{t("header.sun")}</span>
      <span>{range}</span>
    </div>
  );
}

/**
 * The sun in the top bar below md, beside what else shares it (alarms, updates,
 * restart pills, the home time, the link indicator). At 360 px the bar holds the
 * title, the sun and one of them; with two or more the sun waits for sm, so the
 * title does not shrink to a few letters and the bar never wraps (#1133).
 */
export function MobileSunTimes({
  data,
  headerPills,
}: {
  data: ZoneAggregatedData | undefined;
  headerPills: number;
}) {
  return (
    <div className={headerPills <= 1 ? "flex" : "hidden sm:flex"}>
      <SunTimes data={data} />
    </div>
  );
}

/**
 * Solite's addition (D62, spec 037 R2): where Sowel shows nothing because no location is set, a
 * link to Settings in the place of the sun times. The words from `sm` up; below, the sun alone,
 * with the words as its label, in a 44 px tap area.
 */
export function SunLocationHint() {
  const { t } = useTranslation();
  const label = t("header.setLocation");
  return (
    <a
      href="#/settings"
      aria-label={label}
      title={label}
      className="flex items-center justify-center gap-1 min-h-11 min-w-11 sm:min-h-0 sm:min-w-0 text-caption text-text-tertiary hover:text-text-secondary whitespace-nowrap transition-colors duration-150"
    >
      <Sun size={14} strokeWidth={1.5} aria-hidden="true" />
      <span className="hidden sm:inline" aria-hidden="true">
        {label}
      </span>
    </a>
  );
}

/** {@link SunLocationHint} below `md`, under the rule of {@link MobileSunTimes}. */
export function MobileSunLocationHint({ headerPills }: { headerPills: number }) {
  return (
    <div className={headerPills <= 1 ? "flex" : "hidden sm:flex"}>
      <SunLocationHint />
    </div>
  );
}
