// Ported from Sowel ui/src/components/layout/admin-nav-items.ts (Sowel commit 5da84dc1, UI 1.87.0).
// Cut to Devices, Equipments and Topology (Settings added, spec 026); Solite's "Discovery" stands in the place of Sowel's
// "Integrations" entry (spec 019, R1). `to` is a hash (Solite has no router) and `page` the page it
// shows; `adminOnly` is cut (every Solite user is an administrator until J4).
import { Box, Map, Radar, Radio, Settings, type LucideIcon } from "lucide-react";

/** The administration pages, as `App` shows them. */
export type AdminPage = "devices" | "equipments" | "zones" | "discovery" | "settings";

export interface AdminNavItem {
  to: string;
  page: AdminPage;
  labelKey: string;
  icon: LucideIcon;
}

/** Single source of truth for the Administration navigation, rendered by the sidebar and the drawer. */
export const ADMIN_NAV_ITEMS: AdminNavItem[] = [
  { to: "#/devices", page: "devices", labelKey: "nav.devices", icon: Radio },
  { to: "#/equipments", page: "equipments", labelKey: "nav.equipments", icon: Box },
  { to: "#/zones", page: "zones", labelKey: "nav.zones", icon: Map },
  { to: "#/discovery/solite", page: "discovery", labelKey: "nav.discovery", icon: Radar },
  { to: "#/settings", page: "settings", labelKey: "nav.settings", icon: Settings },
];

/** The administration page a hash shows, `null` for a room page. */
export function adminPageFromHash(hash: string): AdminPage | null {
  if (hash === "#/discovery" || hash.startsWith("#/discovery/")) return "discovery";
  return ADMIN_NAV_ITEMS.find((item) => item.to === hash)?.page ?? null;
}
