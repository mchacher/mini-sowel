# Architecture: Spec 024

## Overview

```
Room page (ZonePage)
  └─ ZoneRecipesSection ── useRecipes (store) ── api/recipes.ts ── /api/v1/recipes, /recipe-instances
       ├─ RecipePickerPopover        (GET /recipes)
       ├─ RecipeForm (create | edit)  slots → fields → params   (POST | PUT)
       └─ RecipeInstanceRow ── RecipeJournal (GET .../log every 5 s)
                            └─ Settings dialog (Modal + RecipeForm + Delete)
useLive: topic "recipes"; recipe.instance.* → useRecipes.fetchInstances (coalesced)

Core (crates/core)
  Home::delete_equipment / update_equipment / remove_*_binding / remove_device / delete_zone
    └─ Home::settle_recipes(change, wall)              (home/recipe_ops.rs)
         └─ RecipeEngine::settle(home, gone_equipment, gone_zone, wall, mono)   (recipes/mod.rs)
              1. prune the gone equipment's id from every instance's equipment slots (params text)
              2. delete the instances of a gone zone
              3. validate every enabled instance: invalid → stop, disable, log, events
              4. running and pruned and valid → restart
```

## Data

No new stored data. `recipes.json` is written when a settle changed an instance (pruned params,
`enabled: false`, an instance deleted), through the existing `recipes_changed` flag.

The params stay JSON text (`RecipeInstance.params: Box<str>`, spec 012): a prune parses them for a
moment, removes the id from each array slot of type `equipment` (and a string slot equal to it),
and writes the text back (it can only get shorter, so it stays under `MAX_PARAMS_BYTES`).

Which slots hold equipments comes from the recipe's info (`slots[].type == "equipment"`), plus
motion-light's legacy `light`; no recipe-specific code in the settle.

## Core

- `recipes/mod.rs`: `RecipeEngine::settle(&mut self, home, change: &Settle, wall, mono) -> bool`
  (true when something is to be written). `Settle { equipment: Option<(Id, name)>, zone:
  Option<Id> }`. A helper `prune_equipment(params_text, slots, id) -> Option<Box<str>>` (the new
  text when it changed). `set_enabled(.., true)` validates first and refuses (R4.7). The stop-and-disable path reuses `announce_stopped` and the
  `RecipeInstanceError` event; the restart reuses `start_slot` (failure text `Failed to restart`;
  a restart that fails disables the instance as step 3 does).
- `home/recipe_ops.rs`: `Home::settle_recipes(&mut self, change, wall)`: runs the settle with the
  engine taken out (`with_engine`), sets `recipes_changed` when it returns true. The interest is
  recomputed by `with_engine`.
- `home/equipment_ops.rs`: `delete_equipment` (with the removed id and name), `update_equipment`,
  `remove_data_binding`, `remove_order_binding` call it. Their signatures gain nothing visible to
  the API except `now` where they lack it (the API passes `now` already for update; delete and the
  binding removals take it now).
- `home/mod.rs`: `remove_device` emits `equipment.updated` for each equipment whose bindings it
  dropped (`announce_equipment_updated`), then settles; `delete_zone` settles with the zone.
  `remove_device` gains a `now` argument (its callers: `api/devices.rs`, the host's and the
  firmware's none: checked by `cargo build`).

## Interface

Ported from Sowel (`ui/src/components/recipes/`, commit of `../sowel` at the time of the port),
cut down, each file's header saying what was cut:

| File | From Sowel | Cuts |
| ---- | ---------- | ---- |
| `ui/src/api/recipes.ts` | `ui/src/api/recipes.ts` (or the recipe part of `api/index.ts`) | only the routes of spec 012 |
| `ui/src/store/useRecipes.ts` | `store/useRecipes.ts` | no actions, no duplicate |
| `ui/src/lib/recipe-i18n.ts` | `lib/recipe-i18n.ts` | groups and select options kept only as needed |
| `ui/src/lib/recipe-form.ts` (Solite's) | the params logic of `AddRecipeForm` and `RecipeSettings` | the form state ↔ params conversion, the slot visibility, the equipment candidates, tested alone |
| `ui/src/components/recipes/ZoneRecipesSection.tsx` | same | no auth, no groups |
| `ui/src/components/recipes/RecipePickerPopover.tsx` | same | plain list |
| `ui/src/components/recipes/RecipeForm.tsx` | `AddRecipeForm` + the form of `RecipeSettings` | one form for create and edit; no slot groups, no `data-key`/`time` pickers (a text field) |
| `ui/src/components/recipes/RecipeInstanceRow.tsx` | same | no bottom sheet, no duplicate, no cycle pills, no marquee |
| `ui/src/components/recipes/RecipeJournal.tsx` | same | no day-period grouping |
| `ui/src/components/recipes/recipe-fields.tsx` | `recipe-form-fields.tsx` | `EquipmentCheckboxList`, `DurationInput`, `CountdownTimer` only |
| `ui/src/components/home/ZonePage.tsx` | (Solite's) | renders `ZoneRecipesSection` |
| `ui/src/store/useLive.ts` | (Solite's) | topic `recipes`, the events, refetch on open |
| `ui/src/types.ts` | `src/shared/types.ts` T:680-853 | `RecipeInfo`, `RecipeSlotDef`, `RecipeLangPack`, `RecipeSlotI18n`, `RecipeInstance` (+ `state`), `RecipeLogEntry` |
| `ui/src/i18n/messages.ts` | `recipes.*` of Sowel's locales | the keys used |

The switch is Sowel's `Switch` if Solite has none ported: a small port of `ui/src/components/ui/Switch.tsx`.
The icon tile uses lucide's `Lightbulb` for `icon: "Lightbulb"` and `ChefHat` otherwise (by name,
the icon rule of the design system).

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/src/recipes/mod.rs` | `settle`, `prune_equipment`, the `Settle` change |
| `crates/core/src/home/recipe_ops.rs` | `settle_recipes` |
| `crates/core/src/home/equipment_ops.rs` | settle after delete, update, binding removals |
| `crates/core/src/home/mod.rs` | `remove_device` events and settle; `delete_zone` settle |
| `crates/core/src/api/*.rs` | pass `now` where the home now takes it |
| `crates/core/tests/recipe_settle.rs` | new: R4, R5 |
| `crates/core/tests/motion_light.rs` | the test of a light deleted while running follows R4 |
| `crates/core/tests/engine.rs`, `streaming.rs` | `remove_device` takes `now` |
| `ui/src/...` | the table above, with a test beside each logic file and component |
| `docs/living-spec/README.md` | the delta |
| `docs/decisions.md` | D38 |
| `docs/design-system.md` | the port table rows, the first-load figure |
| `docs/roadmap.md` | J3 status, inbox lines |
| `docs/specs-index.md` | row 024 |

## Reused

- The Modal, `FIELD_LABEL`, `FIELD_INPUT`, `BUTTON_PRIMARY`, `BUTTON_SECONDARY` of
  `ui/src/components/ui/Modal.tsx`; `Panel`, `SliceHeader`, `SliceAddButton` of `home/Panel.tsx`;
  `coalesceCalls`; `apiErrorText`; the 5 s refusal text pattern of the Devices and Equipments
  pages.
- In the core: `start_slot`, `announce_stopped`, `push_log`, the `with_engine` pattern, and
  motion-light's `validate` through the engine's `validate`.
