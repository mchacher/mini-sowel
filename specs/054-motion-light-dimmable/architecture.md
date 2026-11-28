# Architecture: Spec 054

A script recipe: `recipes/scripts/motion-light-dimmable.js`, ported from Sowel's `src/index.ts`
as `motion-light.js` was (spec 046). The script API lacked what Sowel's code reads from the server:
the time of day. Two small changes of the engine (the session's decision, 2026-11-28):
- `now().localTime` (`crates/core/src/recipes/script/shim.js`): `"HH:MM"` of the wall clock of the
  call in the home's zone, from a host function `offset` (`runtime.rs`) that reads the host's
  `utc_offset_seconds`, the one `at` uses.
- A slot's `group` text (`crates/core/src/recipes/script/slots.rs`) is kept and served in the
  recipe's info, as Sowel's `RecipeSlotDef.group`.
No change to the API's routes or the interface.

Tests: `now().localTime` in `crates/core/tests/script_recipes.rs` (a zone that is not UTC, the day
boundary); the recipe in `crates/core/tests/motion_light_dimmable.rs` (info against
`crates/core/tests/pinned/recipe_motion_light_dimmable.json`, validation texts, the behaviour on
the simulated clock), as `crates/core/tests/switch_light.rs`.
