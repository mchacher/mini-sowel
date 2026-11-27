# Plan, spec 046

## Steps
- [x] 0. The user's answers (2026-11-27): id `motion-light`; the script's behaviour kept; Sowel's
      texts; 8 instances; version 1.1.2; no recipe on a new box (inbox line); ceilings option A.
- [x] 1. Living spec delta, D81, the inbox line.
- [x] 2. The script: rename, id, version, Sowel's info and i18n, Sowel's texts; the host runs the
      script's `validate` first (R3). Pinned JSON test on the script.
- [x] 3. Tests moved to the script (parity → `motion_light.rs`). Done with the native registration already gone (the script takes the id `motion-light`, which the native held: the two cannot be loaded together), so the oracle was the old parity run, green at `HEAD`.
- [x] 4. Core and host tests: `required-features`, the script loaded, the new id and paths.
- [x] 5. Memory ceilings (R8): Mac tests on script instances, headers; QEMU phase loads then starts.
- [x] 6. Remove the crate and the registration; feature-off builds (`-D warnings`).
- [x] 7. Mac launchers, demo-garage, e2e, box-session B11 and its tests, firmware.sh, qemu.sh.
- [x] 8. Docs "no longer true"; the image size and the scripts' line in the memory budget.
- [ ] 9. validate; code review; the box check with the user (recipe key, flash, signed 1.1.2).

## Tests dropped (filled at step 3)
| Test | Reason |
| --- | --- |
| native `crates/recipes/motion-light/tests/motion_light.rs` (7: info, motion words, motion sources, state text, listen set, no-sensor warning, prune) | the crate goes; the info is pinned by `recipe_json_bytes.rs` on the script, the state keys and the no-sensor warning by `motion_light.rs`; the motion words (`on`, `occupied`, `detected`...) reach the script only through `onMotionReport`, whose radio events carry booleans: not testable without a new seam; the sources, the listen set and `prune` are the native's internals |
| native crate's 0 unit tests in `src/` | none existed |
| old `crates/core/tests/motion_light.rs`: `the_recipe_list_is_the_json_of_the_reference`, the two motion-sources tests (`resolve_motion_sources`) | native internals; the list is `recipe_json_bytes.rs` and `the_recipe_is_sowels_motion_light_with_its_seven_slots` |
| old `motion_light.rs`: `every_validate_message_of_the_reference_in_its_order` | rewritten as `every_refused_setting_answers_sowels_text_in_sowels_order` and `what_the_script_cannot_see_keeps_the_hosts_texts...`: the zone-not-found, light-type and light-zone texts are the host's (the script's `validate` sees an equipment's name and binding aliases only); `Invalid duration: true` is now the duration text |
| old `motion_light.rs`: `the_legacy_light_is_used_only_when_lights_is_not_a_list...`, the legacy-`light` settle test of `recipe_settle.rs` | R4: the legacy `light` key is not read (`the_legacy_light_key_is_not_read`, `an_instance_with_only_the_legacy_light_key_stays_stopped...`) |
| old `motion_light.rs`: `the_lux_threshold_follows_javascripts_number` (strings, `" "`, `true` accepted) | the slot is a number: only numbers pass (`the_lux_threshold_is_a_number_and_it_behaves`) |
| `script_parity` native flavour (every `parity_*` played twice) | one implementation left: each scenario runs once on the script, its `expect` lines kept |
| `script_parity`'s `difference_*` native halves | R3 and R4 tests of `motion_light.rs` |
| `lock_time` native run and 16-instance case | 8 instances at most (R5); `one_batch_of_measures_holds_the_home_lock` stays, on the realistic home that now runs script instances |
| `recipe_settle.rs`: seven button-pruning tests (two instances, timer kept, binding removed, device removed, disabled instance, press on a pruned button, the same button twice) | R4: a button that loses its `action` binding disables the instance, a deleted one leaves the settings by the generic settle (`a_deleted_equipment_leaves_the_settings...` in `motion_light.rs`) |
| `recipe_install.rs`: the "native recipe's id" refusals (install, upload, uninstall of `motion-light`) | no native recipe; the `--recipe-script` id is refused the same way (`the_motion_light_script_given_by_the_program_is_built_in_for_every_source`) |
| `script_recipes.rs`: a script claiming the id `motion-light` is refused as built in; the built-in recipe in the recipe lists | `motion-light` loads like any id |

## Test plan
| Module | Scenario | Expected |
| --- | --- | --- |
| recipes engine | `RecipeEngine::new()` | no recipe listed |
| build without `script-recipes` | `GET /recipes`; POST an instance | `[]`; 404 `Recipe not found: motion-light` |
| recipe info | `GET /recipes/motion-light` | pinned Sowel 1.1.2 bytes (or the named `min` difference) |
| validation | each refused setting of R3 | Sowel's text, exact |
| validation | another script without its own check | the host's generic text |
| validation | `lights` given as a string (the script's `validate` throws) | the generic check's text, never accepted |
| restore | home with 3 native instances, nothing installed | 3 stopped, `Failed to restore: Recipe not found: motion-light`, settings kept |
| install | same home, `motion-light` 1.1.2 installed | the 3 start, settings unchanged |
| restore | an instance with only the legacy `light` key | stopped, `Failed to restore: At least one light is required` |
| bounds | a ninth instance, another zone | 400 `Too many instances of motion-light (max 8)` |
| bounds | 9 instances at boot | the ninth stopped with that text |
| install | `motion-light-script` 1.0.0 and `motion-light` 1.1.2 installed, a third | refused `Too many script recipes (at most 2)` |
| Mac | `--recipe-script` with id `motion-light` plus `scripts/motion-light.srs` | the file refused at boot `motion-light is built in` |
| Mac | `recipe_script.rs:72` case | the script loads (no native to clash with) |
| behaviour | every former `parity_*` scenario | passes |
| behaviour | the five of R4 | as stated |
| behaviour | boot by day with the installed file | the light is not switched |
| contract | `sowel_contract` on the script's recipe | Sowel's field names and slot types |
| memory | `memory_ceiling`, `memory_worst_case`, `lock_time` | under their ceilings, headers listing the script's bounds |
| QEMU | script phase | script loaded, 3 kept instances started, memory gate measured with them |
| build | radio image | no native symbol, no `id: "motion-light"` string; smaller |
| UI | zone page with `GET /recipes` empty | no recipe, the store card pointed to |
| box session | B11 on a box with the recipe missing, then present | installs from the given file; checks the garage's wave; never uninstalls |
| tools | `demo-garage.sh` against a host without the recipe | stops with its message |

## Validation
`sh scripts/quiet.sh sh scripts/validate.sh` green, QEMU included.
