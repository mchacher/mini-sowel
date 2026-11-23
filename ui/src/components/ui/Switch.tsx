// Ported from Sowel ui/src/components/ui/Switch.tsx (Sowel commit 5da84dc1, UI 1.87.0). The click
// event's type changes, for Preact; the `hit-44` tap-area class is cut (Solite has no such utility).
/**
 * The on/off switch (spec 196). One drawing for every switch in the app.
 *
 * Off: a pale track with a visible outline and a grey thumb, so an off switch
 * still reads as a control. On: a primary track with a white thumb. 36 x 20 px
 * drawn.
 */
export function Switch({
  checked,
  onChange,
  disabled,
  label,
  title,
  className = "",
}: {
  checked: boolean;
  onChange: (next: boolean, event: MouseEvent) => void;
  disabled?: boolean;
  /** Accessible name, when no visible label is tied to the switch. */
  label?: string;
  title?: string;
  /** Layout classes from the caller (grid placement, margins). */
  className?: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      title={title}
      disabled={disabled}
      onClick={(e) => onChange(!checked, e)}
      className={`relative inline-flex items-center w-9 h-5 px-[1.5px] shrink-0 rounded-full border-[1.5px] transition-colors duration-200 cursor-pointer disabled:opacity-50 disabled:cursor-default ${
        checked ? "bg-primary-fill border-primary-fill" : "bg-[var(--n-100)] border-text-tertiary"
      } ${className}`}
    >
      <span
        aria-hidden="true"
        className={`w-3.5 h-3.5 rounded-full shadow-sm transition-transform duration-200 ${
          checked ? "translate-x-4 bg-white" : "translate-x-0 bg-text-tertiary"
        }`}
      />
    </button>
  );
}
