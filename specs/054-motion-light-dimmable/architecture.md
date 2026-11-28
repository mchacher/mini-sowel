# Architecture: Spec 054

A script recipe only: `recipes/scripts/motion-light-dimmable.js`, ported from Sowel's
`src/index.ts` as `motion-light.js` was (spec 046). No change to the engine, its API or the
interface; if the script API lacks a call Sowel's code uses, the spec stops and says so.

Tests: `crates/core/tests/motion_light_dimmable.rs` (info against
`crates/core/tests/pinned/recipe_motion_light_dimmable.json`, validation texts, the behaviour on
the simulated clock), as `crates/core/tests/switch_light.rs`.
