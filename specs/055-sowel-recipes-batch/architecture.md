# Architecture: Spec 055

Script recipes only: `recipes/scripts/{schedule-on-off,state-trigger-light,presence-thermostat,state-watch}.js`,
ported as spec 054's. Tests per recipe in `crates/core/tests/<id_with_underscores>.rs` with
`crates/core/tests/pinned/recipe_<id>.json`. The only engine change allowed: fields of the home's
local calendar in `api.now()` (R1), next to spec 054's `localTime`.
