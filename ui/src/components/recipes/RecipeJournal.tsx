// Ported from Sowel ui/src/components/recipes/RecipeJournal.tsx (Sowel commit 5da84dc1, UI 1.87.0).
// Cuts: the day-period grouping and the span label; the log is read through the API, not a store.
import { useEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { getRecipeInstanceLog } from "../../api";
import { currentLocale, formatClockTime } from "../../lib/format";
import type { RecipeLogEntry } from "../../types";

const LEVEL_CLASS: Record<RecipeLogEntry["level"], string> = {
  error: "text-error",
  warn: "text-warning-ink",
  info: "text-text-secondary",
};

/** How often the log is read again while the journal is open. */
const REFRESH_MS = 5000;

/**
 * A recipe instance's journal, shown inline under its row: newest first (the order the host gives),
 * `HH:MM` and the message of each line. Read again every 5 s while shown.
 */
export function RecipeJournal({ instanceId, action }: { instanceId: string; action?: ReactNode }) {
  const { t } = useTranslation();
  const locale = currentLocale();
  const [entries, setEntries] = useState<RecipeLogEntry[] | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let alive = true;
    let inFlight = false;
    const load = async () => {
      // One request at a time: a slow answer never overwrites a newer one.
      if (inFlight) return;
      inFlight = true;
      try {
        const next = await getRecipeInstanceLog(instanceId);
        if (alive) {
          setEntries(next);
          setFailed(false);
        }
      } catch {
        // A failed refresh keeps the lines shown; a failed first read says so.
        if (alive) setFailed(true);
      } finally {
        inFlight = false;
      }
    };
    void load();
    const id = setInterval(() => void load(), REFRESH_MS);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, [instanceId]);

  return (
    <div>
      <div className="flex items-center justify-between gap-2 min-h-7">
        <span className="text-caption font-medium text-text-tertiary">{t("recipes.journal")}</span>
        {action}
      </div>
      {entries === null && failed && (
        <p className="text-caption text-warning-ink text-center py-2">
          {t("recipes.journalError")}
        </p>
      )}
      {entries !== null && entries.length === 0 && (
        <p className="text-caption text-text-tertiary text-center py-2">{t("recipes.noEntries")}</p>
      )}
      <ul className="max-h-[220px] overflow-y-auto">
        {(entries ?? []).map((entry) => (
          <li
            key={entry.id}
            data-level={entry.level}
            className={`grid grid-cols-[2.75rem_1fr] gap-2 py-0.5 ${LEVEL_CLASS[entry.level]}`}
          >
            <time
              dateTime={entry.timestamp}
              className={`text-caption tabular-nums ${entry.level === "info" ? "text-text-tertiary" : ""}`}
            >
              {formatClockTime(new Date(entry.timestamp), { locale })}
            </time>
            <span className="text-micro font-mono leading-5 break-words min-w-0">
              {entry.message}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}
