# Architecture: Spec 052

## Overview
A script recipe only: no change to the engine unless the script API lacks a call Sowel's code
needs (then the smallest addition, stated).

## Data
None new.

## Files changed
| File | Change |
| --- | --- |
| `recipes/scripts/switch-light.js` | the port |
| `crates/core/tests/switch_light.rs`, `crates/core/tests/pinned/` | its tests, its pinned info |
| `scenarios/button.json` or a new scenario | a button and a light |
| the catalogue (`mchacher/solite-recipes`) | its signed file and entry |
| docs | living spec, roadmap |

## Reused
`recipes/scripts/motion-light.js` (structure, shim calls, validation), its tests
(`crates/core/tests/motion_light.rs`, `recipe_json_bytes.rs`), the simulated button (spec 049).
