// Solite's own file (spec 037): where you are, as the top bar says it. Sowel's `AppLayout` builds
// the same parts inside `TopbarBreadcrumb` and `MobileTopbarTitle` from its router; Solite has no
// router, so the page comes from the hash and the zone from `chooseZone`, and the parts are made
// here, apart from the components.
import type { ZoneWithChildren } from "../../types";
import { ADMIN_NAV_ITEMS, type AdminPage } from "./admin-nav-items";

/** The zones from the root down to `id`, empty when `id` is not in the tree. */
export function zonePath(tree: ZoneWithChildren[], id: string): ZoneWithChildren[] {
  for (const node of tree) {
    if (node.id === id) return [node];
    const below = zonePath(node.children, id);
    if (below.length > 0) return [node, ...below];
  }
  return [];
}

/**
 * The parts of the context after the home's name: an administration page's label (an equipment's
 * or a device's page is its list), or the path of the zone the page shows, the root left out as
 * Sowel's (the home's name already says it). The root zone's own page, and no zone, give none.
 */
export function topBarContext(input: {
  page: AdminPage | null;
  zoneId: string | null;
  tree: ZoneWithChildren[];
  t: (key: string) => string;
}): string[] {
  const { page, zoneId, tree, t } = input;
  if (page) {
    const item = ADMIN_NAV_ITEMS.find((i) => i.page === page);
    return item ? [t(item.labelKey)] : [];
  }
  if (zoneId === null) return [];
  const path = zonePath(tree, zoneId);
  const segments = path.length > 0 && path[0].parentId === null ? path.slice(1) : path;
  return segments.map((z) => z.name);
}
