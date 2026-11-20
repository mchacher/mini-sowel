import { SowelLogo } from "./SowelLogo";

/** The navigation column, in the look of Sowel's sidebar: the logo and the tool's name. */
export function Sidebar() {
  return (
    <aside className="flex flex-col h-full w-[260px] bg-surface border-r border-border">
      <div className="flex items-center h-[49px] px-4 border-b border-border-light">
        <SowelLogo size={40} className="flex-shrink-0" />
        <span className="font-extrabold text-title tracking-[0.18em] text-primary ml-2.5 mt-1">
          MINI-SOWEL
        </span>
      </div>
      <nav aria-label="Main" className="flex-1 overflow-y-auto py-3 px-2" />
    </aside>
  );
}
