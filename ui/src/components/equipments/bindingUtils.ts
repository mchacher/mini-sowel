// Subset of Sowel ui/src/components/equipments/bindingUtils.ts (Sowel commit 5da84dc1, UI 1.87.0):
// `findOrderByCategory`, `findMainOnOffOrder` and `relayStateOn`, copied unchanged with the
// minimal binding types they take. The rest of the file (binding candidates, the device API, the
// EV contracts) is not used by Solite's screen.
import type { DataCategory, OrderCategory } from "../../types";

interface MinimalOrderBinding {
  alias: string;
  category?: OrderCategory;
}

interface MinimalDataBinding {
  alias: string;
  category?: DataCategory;
}

/**
 * Find an order binding by category, with optional alias and regex fallbacks.
 *
 * Resolution order:
 *   1. First binding whose `category` is in `categories`.
 *   2. First binding whose `alias` is in `aliasFallbacks` (legacy
 *      compatibility for bindings that pre-date category typing).
 *   3. First binding whose `alias` matches any of `aliasPatterns` (e.g.
 *      `^shutter\d*_state$` for indexed channels).
 *
 * Always pass at least one category. The fallbacks are for migration —
 * once every plugin emits a category and every binding carries one,
 * fallbacks can be dropped.
 *
 * @example
 * const move = findOrderByCategory(
 *   eq.orderBindings,
 *   ["pool_cover_move", "shutter_move"],
 *   ["state"],
 *   [/^shutter\d*_state$/],
 * );
 * if (move) await onExecuteOrder(move.alias, "OPEN");
 */
export function findOrderByCategory<T extends MinimalOrderBinding>(
  bindings: readonly T[],
  categories: readonly OrderCategory[],
  aliasFallbacks?: readonly string[],
  aliasPatterns?: readonly RegExp[],
): T | undefined {
  const byCategory = bindings.find(
    (b) => b.category !== undefined && categories.includes(b.category),
  );
  if (byCategory) return byCategory;
  if (aliasFallbacks && aliasFallbacks.length > 0) {
    const byAlias = bindings.find((b) => aliasFallbacks.includes(b.alias));
    if (byAlias) return byAlias;
  }
  if (aliasPatterns && aliasPatterns.length > 0) {
    return bindings.find((b) => aliasPatterns.some((re) => re.test(b.alias)));
  }
  return undefined;
}

/**
 * Spec 152 — resolve the MAIN on/off command of a light/switch/water_heater,
 * mirroring exactly what `LightControl` binds to (category-first, then the loose
 * boolean/`state`-enum fallback), while never matching the dedicated solar
 * channel. Callers use it as the render gate for the main toggle so the gate and
 * the control resolve identically (no "gate hides a toggle LightControl would
 * still render" regression for legacy uncategorized bindings).
 */
export function findMainOnOffOrder<T extends MinimalOrderBinding & { type?: string }>(
  bindings: readonly T[],
): T | undefined {
  return (
    findOrderByCategory(bindings, ["light_toggle", "toggle_power"], ["state"]) ??
    bindings.find(
      (b) =>
        b.alias !== "solar" &&
        b.category !== "solar_toggle" &&
        (b.type === "boolean" || (b.alias === "state" && b.type === "enum")),
    )
  );
}

/**
 * Whether a relay-style equipment (light, heater, plug, valve, pump) is on: the
 * first binding with alias `state` or category `light_state`, `true` or « ON »
 * in any case. The controls and the row's tile (spec 208) read it here, so they
 * never disagree. False when there is no such binding.
 */
export function relayStateOn(
  bindings: readonly (MinimalDataBinding & { value?: unknown })[],
): boolean {
  const state = bindings.find((b) => b.alias === "state" || b.category === "light_state");
  if (!state) return false;
  return state.value === true || String(state.value).toUpperCase() === "ON";
}
