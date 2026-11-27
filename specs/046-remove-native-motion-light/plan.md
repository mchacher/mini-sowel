# Plan, spec 046

## Steps
- [x] 0. The user's answers (2026-11-27): id `motion-light`; the script's behaviour kept; Sowel's
      texts; 8 instances; version 1.1.2; no recipe on a new box (inbox line); ceilings option A.
- [ ] 1. Living spec delta, D81, the inbox line.
- [ ] 2. The script: rename, id, version, Sowel's info and i18n, Sowel's texts; the host runs the
      script's `validate` first (R3). Pinned JSON test on the script.
- [ ] 3. Tests moved to the script (parity → `motion_light.rs`), green while the native still exists.
- [ ] 4. Core and host tests: `required-features`, the script loaded, the new id and paths.
- [ ] 5. Memory ceilings (R8): Mac tests on script instances, headers; QEMU phase loads then starts.
- [ ] 6. Remove the crate and the registration; feature-off builds (`-D warnings`).
- [ ] 7. Mac launchers, demo-garage, e2e, box-session B11 and its tests, firmware.sh, qemu.sh.
- [ ] 8. Docs "no longer true"; the image size and the scripts' line in the memory budget.
- [ ] 9. validate; code review; the box check with the user (recipe key, flash, signed 1.1.2).

## Tests dropped (filled at step 3)
| Test | Reason |
| --- | --- |
| native `crates/recipes/motion-light/tests/*` (7) | the crate goes; reference JSON kept by `recipe_json_bytes` on the script, motion-word parsing kept if the script parses it |
| `script_parity` native flavour | one implementation left |
| `lock_time` 16-instance case | 8 instances at most (R5) |
| native settle of a button losing `action`, legacy `light` emptied at settle | R4: the script's behaviour |

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
| restore | an instance with only the legacy `light` key | stopped, `Failed to restore: Invalid params: At least one light is required` |
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
