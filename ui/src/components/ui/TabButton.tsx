// Sowel's `TabButton` (ui/src/pages/DevicesPage.tsx, Sowel commit 5da84dc1), unchanged: an
// integration's tab with the count of its devices. Used by Discovery and, past one integration,
// by the Devices page (spec 019).
export function TabButton({
  label,
  count,
  active,
  onClick,
}: {
  label: string;
  count: number;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-current={active ? "page" : undefined}
      className={`
        shrink-0 px-3 py-2 text-small font-medium transition-colors duration-150
        border-b-2 -mb-px cursor-pointer
        ${
          active
            ? "border-primary text-primary"
            : "border-transparent text-text-tertiary hover:text-text-secondary hover:border-border"
        }
      `}
    >
      {label}
      <span
        className={`ml-1.5 text-micro tabular-nums ${active ? "text-primary/70" : "text-text-tertiary"}`}
      >
        {count}
      </span>
    </button>
  );
}
