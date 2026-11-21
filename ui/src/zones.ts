// mini-Sowel's own file: which zone the page shows. The zone is kept in `location.hash`
// (`#/zone/<id>`), there is no router (spec 014).
import type { EquipmentWithDetails, ZoneWithChildren } from "./types";

export interface FlatZone {
  zone: ZoneWithChildren;
  depth: number;
}

/** The zones in depth-first order, each with its depth (the root is 0). */
export function flattenZones(tree: ZoneWithChildren[], depth = 0): FlatZone[] {
  return tree.flatMap((zone) => [{ zone, depth }, ...flattenZones(zone.children, depth + 1)]);
}

const HASH_PREFIX = "#/zone/";

export function zoneIdFromHash(hash: string): string | null {
  if (!hash.startsWith(HASH_PREFIX)) return null;
  let id: string;
  try {
    id = decodeURIComponent(hash.slice(HASH_PREFIX.length));
  } catch {
    return null; // a malformed escape (`#/zone/%`): no zone asked, the first one is shown
  }
  return id === "" ? null : id;
}

export function hashForZone(id: string): string {
  return `${HASH_PREFIX}${encodeURIComponent(id)}`;
}

/**
 * The zone to show: the one of the hash when it exists, else the first zone (depth-first) that has
 * equipments, else the first zone (the root). `null` when there is no zone at all.
 */
export function chooseZone(
  flat: FlatZone[],
  equipments: EquipmentWithDetails[],
  hashId: string | null,
): ZoneWithChildren | null {
  const wanted = flat.find((f) => f.zone.id === hashId);
  if (wanted) return wanted.zone;
  const withEquipments = flat.find((f) => equipments.some((e) => e.zoneId === f.zone.id));
  return (withEquipments ?? flat[0])?.zone ?? null;
}
