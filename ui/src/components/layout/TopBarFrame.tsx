// Ported from Sowel ui/src/components/layout/TopBarFrame.tsx (Sowel commit 2d360726). Changed: the
// switch point `lg` is `md` (Solite's sidebar switch point); no `useShadowMode` (Solite has no
// shadow or takeover banner), so below `md` the bar is always fixed.
import type { ReactNode } from "react";

// iOS 26 blurs the top ~40pt of the page (title and pills included) unless a
// fixed, full-width box of one solid colour covers the top edge. Below md the
// safe-area band and the top bar are that box, and a spacer keeps their place.
// WebKit only samples that box while the status bar obscures the page: under the
// black-translucent status bar style nothing the page paints stops the blur (#1164).
// The fixed box sits at z-30: an overlay rendered inside <main> under a stacking
// context (transform, opacity, filter) ends up below it, so portal overlays.
export function TopBarFrame({ children }: { children: ReactNode }) {
  return (
    <>
      <div
        data-testid="topbar-frame"
        className="flex-shrink-0 bg-surface max-md:fixed max-md:inset-x-0 max-md:top-0 max-md:z-30"
      >
        <div style={{ height: "env(safe-area-inset-top, 0px)" }} />
        {children}
      </div>
      <div
        aria-hidden
        data-testid="topbar-spacer"
        className="md:hidden flex-shrink-0"
        style={{ height: "calc(env(safe-area-inset-top, 0px) + var(--spacing-topbar))" }}
      />
    </>
  );
}
