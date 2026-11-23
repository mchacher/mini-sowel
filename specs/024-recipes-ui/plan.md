# Plan: Spec 024

## Steps

- [x] 1. Living spec updated (the delta, History row 024); D38 in `docs/decisions.md`; roadmap
      (J3, inbox lines); `docs/specs-index.md` row 024.
- [x] 2. Core types: not applicable (Sowel's recipe types exist since spec 012).
- [x] 3. Core logic: the tests of `crates/core/tests/recipe_settle.rs` first, then `settle`,
      `settle_recipes`, the calls in the equipment, binding, device and zone changes, the events of
      `remove_device`, the refused enable; `motion_light.rs`'s deleted-light test follows R4; the
      callers of the functions that gain `now` (`engine.rs`, `streaming.rs` tests, `api/`).
- [x] 4. Adapters (host, firmware): not applicable beyond the `now` argument passed through.
- [x] 5. API or WebSocket: not applicable (routes and events of specs 012 and 013).
- [x] 6. Interface: types, `api/recipes.ts`, `store/useRecipes.ts`, `lib/recipe-i18n.ts`,
      `lib/recipe-form.ts` with their tests first; then the components, `ZonePage`, `useLive`,
      messages; tests beside each. `npm run check:bundle`.
- [x] 7. Docs: `docs/design-system.md` (port rows, first-load figure), `docs/architecture.md` if a
      block changes.
- [x] 8. `sh scripts/validate.sh` green.
- [ ] 9. Code review (`code-reviewer`), demo on the simulator.
- [ ] 10. The box: radio image flashed with `--keep-storage`, the J3 flow over the API, result in
      the spec.

## Test plan

### Rust, core (`crates/core/tests/recipe_settle.rs`)

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| settle, delete | The only light of a running instance is deleted (`DELETE /equipments/:id`) | 204; the instance is `enabled: false`, not running (`actions` → 400 `Instance not running`); its `params.lights` is `[]`; its log, newest first: `Disabled: At least one light is required`, `Equipment removed from settings: <name>`; events `recipe.instance.stopped` then `recipe.instance.error`; `recipes.json` to be written holds the new params and `enabled: false` |
| settle, delete | One of two lights of a running instance is deleted | the instance stays enabled and running, `params.lights` is the other one, its log has the warning then a new start line; the next motion switches only the remaining relay |
| settle, delete | After the delete, motion and the timeout | no order is sent to the deleted light's device (nothing to a missing equipment) |
| settle, delete | A disabled instance names the deleted light | pruned (`lights` without it, warning line), stays disabled, no `Disabled:` line, no event `error` |
| settle, delete | A button of `buttons` is deleted | pruned from `buttons`; the instance keeps running after a restart |
| settle, delete | An equipment no instance names is deleted | instances untouched: no line, nothing to write, no event |
| settle, delete | Legacy `light` (a single id) names the deleted light | the key's value becomes `""`, the instance is disabled (`At least one light is required`) |
| settle, update | The light moves to another zone (`PUT /equipments/:id {zoneId}`) | the instance is disabled with `Disabled: Light "<name>" does not belong to the selected zone` |
| settle, update | The light is renamed | the instance is untouched (no line, no restart, nothing to write) |
| settle, binding | The light's `state` order binding is removed | disabled with `Disabled: Light "<name>" has no "state" order binding` |
| settle, device | The relay device is removed (`DELETE /devices/:id`) | `equipment.updated` for the light equipment, before `recipe.instance.stopped` and `recipe.instance.error`; the instance is disabled (no `state` binding) |
| settle, device | A device bound by no equipment is removed | no `equipment.updated`, instances untouched |
| settle, zone | The zone of a disabled instance is deleted (after its light was deleted) | the instance is deleted with its log; `recipe.instance.removed`; `recipes.json` written without it |
| settle, restore | An enabled instance that failed to restore (not running) and no longer validates after an equipment change | disabled, `Disabled: <reason>` |
| settle, bounds | 16 instances with 16 lights each, one of the lights deleted | every instance that named it pruned in one settle; the API answers 204 |
| enable, refused | `POST .../enable` on an instance disabled by a settle (its light deleted) | 400 `Invalid params: At least one light is required`; still `enabled: false`; nothing to write |
| enable, unchanged | `POST .../enable` on a disabled instance that validates | 200 `{"success":true}`, it runs (as today) |
| settle, never fails | A settle meets params that are not JSON (written by hand in `recipes.json`) | the instance is disabled with its reason; the request still answers its usual status |

### Rust, existing (`crates/core/tests/motion_light.rs`, `recipes.rs`)

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| motion_light | A light deleted while running (the test at `motion_light.rs:786`) | follows R4: pruned and restarted with the other light; unchanged otherwise |
| recipes | Every other recipe test | unchanged and green (restore, bounds, routes) |

### Interface (Vitest)

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| `api/recipes.test.ts` | Each call | the route, method and body of Sowel's (`GET /recipes`, `GET /recipe-instances`, `POST`, `PUT {params}`, `DELETE`, `POST .../enable`, `.../disable`, `GET .../log?limit=50`) |
| `lib/recipe-form.test.ts` | Defaults from the slots | zone = the room, `timeout` = `10m`, others `""` |
| `lib/recipe-form.test.ts` | Form → params | booleans as `true`/`false`, a list as an array (empty → `[]`), a duration as `"<n>m"`, others as text |
| `lib/recipe-form.test.ts` | Params → form (edit) | arrays as ticks, `true` as checked, `"20s"` as 0.33 min, an id of an equipment no longer in the room dropped |
| `lib/recipe-form.test.ts` | Required slot empty | the slot's name is returned as missing; nothing else |
| `lib/recipe-form.test.ts` | Visibility | `luxThreshold` hidden without the room's luminosity, shown with it; `buttons` hidden without a button equipment |
| `lib/recipe-form.test.ts` | Equipment candidates | only the room's equipments of the slot's type |
| `lib/recipe-i18n.test.ts` | Names | French from `i18n.fr`, English otherwise, fallback to `name` |
| `store/useRecipes.test.ts` | fetch, create, update, delete, enable, disable | the store follows the host's list after each |
| `RecipeForm.test.tsx` | Create | ticking the light and Create sends `POST` with Sowel's body; the form closes |
| `RecipeForm.test.tsx` | Required empty | "Lights is required" shown, no request |
| `RecipeForm.test.tsx` | Refusal | the host's text under the form; the form stays open with what was typed |
| `RecipeForm.test.tsx` | Edit | fields filled from the params; Save sends `PUT {params}` |
| `ZoneRecipesSection.test.tsx` | The room's instances only | an instance of another room is not shown; the count |
| `ZoneRecipesSection.test.tsx` | Empty room | "No recipe in <room>" and the add link |
| `ZoneRecipesSection.test.tsx` | No recipe on the host | no panel |
| `RecipeInstanceRow.test.tsx` | Switch | calls disable on an enabled instance, enable on a disabled one; a refusal shows its text |
| `RecipeInstanceRow.test.tsx` | Pills | the timer and override pills while enabled; none when disabled |
| `RecipeInstanceRow.test.tsx` | Unknown recipe | the id as its name, Delete in the journal's header |
| `RecipeJournal.test.tsx` | Lines | newest first, error and warning colours; "No entries"; the error text when the first read fails |
| `store/useLive.test.ts` | A `recipe.instance.*` event | the instances are read again (once for a burst) |
| `ZonePage.test.tsx` | Existing behaviour | unchanged; the Recipes panel present |

## Validation

- [x] Validate command green
- [x] Agent review done, blocking findings fixed (none blocking; should-fix 1, 2, 4 fixed; 3 kept: D39 was asked by the user during the run)
- [ ] Demo on the simulator; the J3 flow on the box over the API
