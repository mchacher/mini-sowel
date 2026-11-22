// Solite's own file: the modal shell of Sowel's ZoneForm and EquipmentForm (spec 018, R7): the
// `fixed inset-0 bg-black/40` backdrop, the `bg-surface rounded-lg` panel with its title and cross.
// Sowel's forms each write it out; this one also closes on Escape and on a click outside.
import { useEffect } from "react";
import type { ComponentChildren } from "preact";
import { X } from "lucide-react";
import { useTranslation } from "react-i18next";

/** Sowel's `FIELD_LABEL` (ui/src/components/ui/text-roles.ts, commit 5da84dc1). */
export const FIELD_LABEL = "block text-small font-medium text-text-secondary mb-1";
export const FIELD_INPUT =
  "w-full px-3 py-2 text-body border border-border bg-surface rounded-sm outline-none placeholder:text-text-tertiary focus:border-primary transition-colors duration-150";
export const BUTTON_PRIMARY =
  "px-4 py-2 text-small font-medium text-white bg-primary-fill rounded-sm hover:bg-primary-fill-hover transition-colors duration-150 disabled:opacity-50 disabled:cursor-not-allowed";
export const BUTTON_SECONDARY =
  "px-4 py-2 text-small font-medium text-text-secondary hover:text-text border border-border rounded-sm hover:bg-border-light transition-colors duration-150";

export function Modal({
  title,
  onClose,
  maxWidth = "max-w-[440px]",
  children,
}: {
  title: string;
  onClose: () => void;
  maxWidth?: string;
  children: ComponentChildren;
}) {
  const { t } = useTranslation();

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div
      className="fixed inset-0 bg-black/40 flex items-center justify-center z-50"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-label={title}
        className={`bg-surface rounded-lg w-full ${maxWidth} mx-4 max-h-[90vh] flex flex-col shadow-lg`}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-6 py-4 border-b border-border-light flex-shrink-0">
          <h2 className="text-lead font-semibold text-text">{title}</h2>
          <button
            type="button"
            onClick={onClose}
            aria-label={t("common.close")}
            className="text-text-tertiary hover:text-text-secondary transition-colors"
          >
            <X size={18} strokeWidth={1.5} />
          </button>
        </div>
        {children}
      </div>
    </div>
  );
}
