// Ported from Sowel ui/src/components/recipes/recipe-form-fields.tsx (Sowel commit 5da84dc1, UI 1.87.0).
// Cuts: everything but `EquipmentCheckboxList`, `DurationInput` and `CountdownTimer`. The checkbox
// list holds an array of ids (not a comma-joined text) and shows no zone beside a name (the form
// offers the room's equipments only); the duration field shows the minutes of the stored text,
// rounded to 2 decimals (`lib/recipe-form.ts`).
import { useEffect, useState } from "react";
import { Timer } from "lucide-react";
import { useTranslation } from "react-i18next";
import { formatDuration } from "../../lib/format";
import { durationFromMinutes, durationMinutesText } from "../../lib/recipe-form";

export function CountdownTimer({ expiresAt }: { expiresAt: string }) {
  const [remaining, setRemaining] = useState(() => computeRemaining(expiresAt));

  useEffect(() => {
    setRemaining(computeRemaining(expiresAt));
    const id = setInterval(() => setRemaining(computeRemaining(expiresAt)), 1000);
    return () => clearInterval(id);
  }, [expiresAt]);

  if (remaining <= 0) return null;

  return (
    <span
      data-testid="recipe-countdown"
      className="flex items-center gap-1 text-micro font-medium text-active-ink tabular-nums flex-shrink-0"
    >
      <Timer size={12} strokeWidth={1.5} />
      {formatDuration(remaining)}
    </span>
  );
}

function computeRemaining(iso: string): number {
  return Math.max(0, Math.floor((new Date(iso).getTime() - Date.now()) / 1000));
}

/** A number of minutes, with « min » after it; the form stores `"<n>m"`. */
export function DurationInput({
  value,
  onChange,
  placeholder,
  id,
}: {
  value: string;
  onChange: (durationText: string) => void;
  placeholder?: string;
  id?: string;
}) {
  const { t } = useTranslation();
  return (
    <div className="flex items-center gap-1.5">
      <input
        id={id}
        type="number"
        min={1}
        value={durationMinutesText(value)}
        onInput={(e) => onChange(durationFromMinutes((e.target as HTMLInputElement).value))}
        placeholder={placeholder ?? "10"}
        className="flex-1 px-3 py-1.5 text-small border border-border bg-surface rounded-sm text-text placeholder:text-text-tertiary"
      />
      <span className="text-caption text-text-tertiary font-medium">{t("time.min")}</span>
    </div>
  );
}

/** A checkbox per equipment: the value is the ids ticked, in the order they were ticked. */
export function EquipmentCheckboxList({
  equipments,
  value,
  onChange,
}: {
  equipments: { id: string; name: string }[];
  value: string[];
  onChange: (value: string[]) => void;
}) {
  return (
    <div className="space-y-1">
      {equipments.map((eq) => {
        const checked = value.includes(eq.id);
        return (
          <label
            key={eq.id}
            className="flex items-center gap-2 px-3 py-1.5 text-small border border-border bg-surface rounded-sm text-text cursor-pointer hover:bg-border-light/30 transition-colors duration-150"
          >
            <input
              type="checkbox"
              checked={checked}
              onChange={() =>
                onChange(checked ? value.filter((id) => id !== eq.id) : [...value, eq.id])
              }
            />
            <span className="truncate">{eq.name}</span>
          </label>
        );
      })}
    </div>
  );
}
