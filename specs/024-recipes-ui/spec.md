# Spec 024: recipes-ui

## Context

Roadmap item **J3** (milestone J, the autonomous box): "Recipes from the interface: the list, the
form built from the slots as Sowel's, enable and disable, the log. Done when: motion-light set
without `curl`." Today the recipe routes exist (spec 012) and the box serves the interface (spec
023), but a recipe instance is made only with `curl` or `scripts/demo-garage.sh`.

The architecture review of 2026-11-22 left three inbox lines that touch recipes and point here:

- a running recipe keeps stale ids when an equipment it uses is deleted or its bindings change
  ("settle in J3's spec");
- `remove_device` drops bindings without `equipment.updated` (and `set_device_zone` emits
  nothing);
- a plain `GET` can recompute zones and emit events ("look at it in J3 and J6").

This run is autonomous (the user is away): the open questions are decided here and recorded
(decision D38, and "Decisions taken in this spec" below).

## Goals

1. The user creates, configures, enables, disables and deletes a recipe instance, and reads its
   log, from a room's page of the interface, as in Sowel's room page.
2. The form is built from the recipe's slots (`GET /api/v1/recipes`), as Sowel's form, so a later
   recipe needs no new screen.
3. A recipe that uses an equipment that is deleted, or that no longer fits it, settles at once to
   a clear state: the deleted id leaves its settings, and an instance that can no longer run is
   disabled with the reason in its log; never a crash, never orders to a missing equipment.

## Out of scope

- Sowel's mobile bottom sheet, duplicate dialog, recipe groups (several instances of one recipe
  folded), dashboard tiles, mode-cycle pills and `select`/`time`/`data-key` refinements beyond a
  plain field: Solite has one recipe; they come with the recipes that need them (milestone F).
- `set_device_zone` emitting an event: Sowel's `EngineEvent` has no `device.updated`; the inbox
  line stays for that half.
- The `GET` side effect for J6 (the half of the inbox line that names J6 stays).
- A recipe that resolves new motion sensors after its start (it resolves them when it starts, as
  Sowel's). Likewise a motion sensor (or its binding) deleted under a running instance is not
  settled: the instance keeps its sources; their data simply stops (no crash, no order to a missing
  equipment); a line goes to the roadmap inbox.

## Requirements

### R1: The recipes of a room

On a room's page, under its equipments, a panel **Recipes** ("Recettes") with the number of
instances of that room and a "+" button ("Add a recipe"). It is shown when `GET /recipes` lists at
least one recipe.

- The instances shown are those whose `params.zone` is the room's id (Sowel's
  `ZoneRecipesSection`), oldest first.
- Each instance is a row: the recipe's icon tile, its name in the interface's language (the
  recipe's `i18n.fr` in French, its `name` otherwise), and a second line with the names of the
  equipments of its first `equipment` slot (the lights), three at most then `+N`; a timer pill
  (the time left to `state.timerExpiresAt`, counting down each second) and an "Override" pill
  (`state.overrideMode`) while the instance is enabled; a switch on the right that enables or
  disables it (`POST .../enable` or `.../disable`); a disabled instance is dimmed.
- A click on the row's name opens its **journal** under it: the log, newest first
  (`GET .../log`), each line its time (`HH:MM`) and message, errors in the error colour, warnings
  in the warning colour; read again every 5 s while open; "No entries" when empty; "The journal
  could not be read" when the first read fails (a failed refresh keeps the lines shown). The
  journal's header has a **Settings** button.
- **Settings** opens a dialog with the form of R2 filled with the instance's settings, a
  **Delete** button (confirmed by the browser's dialog, `DELETE .../:id`) and **Save** (`PUT
  .../:id {params}`).
- With no instance in the room: "No recipe in <room>" and an "Add a recipe" link.
- "+" opens a list of the recipes (name and description); picking one opens the form of R2 under
  the panel, to create.
- A request refused (switch, delete) shows its text for 5 s, as the other pages do; the switch
  then shows the instance as the host has it.

### R2: The form, built from the slots

For the picked recipe (create) or the instance's recipe (edit), the form shows the recipe's
description, then one field per slot, in the slots' order, with its name (in the interface's
language), a `*` when it is required and its description under it:

| Slot type | Field | Value sent |
| --------- | ----- | ---------- |
| `zone` | none: the room of the page | the room's id |
| `equipment` with `list` | a checkbox per equipment of the room whose type matches `constraints.equipmentType` | an array of ids, in the order ticked |
| `equipment` | a select of the same equipments, "Select" first | the id, or `""` |
| `duration` | a number of minutes, with "min" after it; the default as placeholder | `"<n>m"`, or `""` |
| `number` | a number, `min`/`max` from the constraints | the text typed, or `""` |
| `boolean` | a checkbox with the slot's name | `true` / `false` |
| `select` | a select of `options` | the value |
| other (`text`, `time`, `data-key`) | a text field | the text |

- **Defaults**: a slot with `defaultValue` starts with it (the timeout `10m` shows `10`).
- **As Sowel's form for motion-light**: the `luxThreshold` field is shown only when the room's
  aggregation has a `luminosity`; the `buttons` field only when the home has a `button`
  equipment. A hidden slot sends `""` (or `[]` for a list).
- **Edit**: the fields start from the instance's `params` (a list as its ticks, a boolean as its
  checkbox, a duration in minutes: `"20s"` shows `0.33…`, rounded to 2 decimals in the field; a
  value the form cannot show as minutes shows empty and is sent back unchanged unless edited).
  An id in the settings that no equipment of the home has any more is not shown and not sent back;
  an id of an equipment outside the room (a button set through the API) is not shown and is sent
  back unchanged (code review).
- **Submit**: a required slot left empty shows "<slot name> is required" and sends nothing;
  otherwise `POST /recipe-instances {recipeId, params}` (create) or `PUT /recipe-instances/:id
  {params}` (edit). A refusal (`Invalid params: <reason>`, `Too many recipe instances (max 16)`...)
  is shown under the form, which stays open with what was typed; the button waits during the
  request. Success closes the form; the list shows the instance from the host.
- **Cancel** (and Escape, and a click outside the dialog) closes without a request.

### R3: Live

The interface subscribes to the `recipes` topic of `/ws`. Every `recipe.instance.*` event
(created, removed, started, stopped, state changed, error) reads the instances again; a burst of
events is coalesced (Sowel's `coalesceCalls`, 1 s window: one read at once, one more at the end of
the window if events came during it). Every reconnection reads the
recipes and instances again, as the other stores.

### R4: A recipe settles when what it uses changes (decision D38)

The core settles the recipe instances right after each change that can make their settings stale:
an equipment deleted, an equipment changed (`PUT /equipments/:id`), a binding removed, a device
removed (its bindings dropped), a zone deleted.

1. **A deleted equipment leaves the settings**: every instance, enabled or not, whose `params`
   name the deleted equipment in an `equipment` slot (a list, or a single id, the legacy `light`
   included) loses that id from its `params`; the instance is written to `recipes.json`; its log
   gets the warning `Equipment removed from settings: <equipment name>`.
2. **An instance that can no longer run is disabled**: every enabled instance is checked again
   with its recipe's validation (the one `POST` runs). When it fails, the instance is stopped (if
   it ran), set to `enabled: false`, written, and logs the error `Disabled: <reason>` (for example
   `Disabled: At least one light is required`, `Disabled: Light "Garage" does not belong to the
   selected zone`); the events are `recipe.instance.stopped` (if it ran) and
   `recipe.instance.error` `{error: <reason>}`. It never sends an order after that.
3. **A running instance whose settings changed restarts**: when step 1 changed the settings of a
   running instance that still validates, it is stopped and started again with them (its start
   line, its state reset, the lights set to what the zone calls for), as a `PUT` does. A start
   fails only on validation, which has just passed, so this restart cannot fail today; the code
   still treats a failure as step 2 does (disabled, `Disabled: <reason>`), untested since no input
   reaches it.
4. **An instance whose zone is deleted is deleted with it**: it is stopped if it ran, removed with
   its log (`recipe.instance.removed`), and `recipes.json` is written. Such an instance cannot be
   made valid again (a new room has a new id) and would be shown nowhere.
5. A running instance that still validates and whose settings did not change is untouched (no
   restart, no line). A disabled instance is only pruned (step 1), never validated.
6. A settle never fails a request: the API's answer to the change is the one it gives today.
7. **Enable refuses an instance that cannot run** (divergence from Sowel, D38): `POST
   .../enable` on a disabled instance whose settings no longer validate answers 400 `Invalid
   params: <reason>`, and the instance stays disabled (no log line, no event). So an enabled
   instance always runs, except one that failed to restore at boot (which logs why). The interface
   shows the refusal's text for 5 s and the switch stays off.

### R5: The events of a removed device

`DELETE /devices/:id` (and any removal of a device) emits `equipment.updated` for each equipment
whose bindings it dropped, as Sowel does when a device's bindings go; then R4 runs, so the
equipment events come before the recipe events. The equipment's status is computed when it is
read, as today (`GET /equipments` shows it `offline`, "no binding", when none is left).

### R6: The `GET` side effect, looked at

A request (any method) refreshes the devices' statuses and, when one changed, recomputes the
zones (`api/mod.rs`). Kept as it is: it is the moment a silent device is seen offline when no
radio turn has run since; it emits only when a status changed; the journal's read every 5 s
adds at most what the radio loop emits within the same second. The inbox line keeps its J6 half.

## Decisions taken in this spec

The run is autonomous; each choice below is the recommended one, recorded for the user's review.

- **Q1, where the recipes are shown**: on the room's page, as Sowel's (`HomePage` →
  `ZoneRecipesSection`), not in Administration. A recipe belongs to a room (`params.zone`).
- **Q2, stale ids** (inbox line): prune and disable, R4 (decision D38). Sowel keeps stale ids and
  lets the instance fail on each order; Solite's box has no one to read a log for a month, so it
  settles to a state the interface shows (the switch off) and the log explains.
- **Q3, a deleted zone**: its instances are deleted (R4.4). In practice a zone holding a
  motion-light's light cannot be deleted (it has equipments), so this happens only to an instance
  already disabled by R4.2.
- **Q4, the `GET` side effect**: kept (R6).
- **Q5, the box**: the user gave their yes (D6) in this run's brief: the radio image is flashed once (`--keep-storage`, Wi-Fi and garage kept) to
  carry the new interface and spec 023's `/ws` close fix, and the J3 flow is run against the box
  over the API (below).

## Living spec delta

### docs/living-spec/README.md

- **"Runs the motion-light recipe"**, Changed: "A light equipment deleted (from the API or the
  interface) while an instance uses it: the running instance counts it as off and logs an error
  for each order to it, the other lights still served; at the next start the instance no longer
  validates (`Light equipment not found: <id>`) and stays stopped." → the rule of R4: the deleted
  equipment leaves every instance's settings (warning line), an enabled instance that no longer
  validates after a change of equipments, bindings, devices or zones is disabled (`Disabled:
  <reason>`, events stopped and error), a running one whose settings changed restarts, an
  instance whose zone is deleted is deleted.
- **"Runs the motion-light recipe"**, New: the instances are also made, changed, enabled,
  disabled, deleted and their log read from a room's page (see "Shows the home in the browser").
- **"Shows the home in the browser"**, New: the room's **Recipes** panel (R1), the form built from
  the slots (R2), live (R3). The list of what the interface does ("pairs the devices..., makes the
  rooms... and the equipments") gains "sets the recipes".
- **"Live events"**, unchanged (the `recipes` topic exists); **"Organises the home"**, New: a
  device removed emits `equipment.updated` for each equipment that lost bindings (R5).
- **History**: row 024.

### No longer true

Searched for `curl`, `recipe`, `motion-light`, `J3`, `stale`, `Light equipment not found`,
`demo-garage` in `docs/` (roadmap and decisions excepted), `README.md`, `CLAUDE.md`,
`inputs/README.md` and `.claude/`:

- `docs/living-spec/README.md`, "Runs the motion-light recipe": the sentence on a deleted light
  (above) → rewritten.
- Other matches describe the routes and the demo script, which stay true.

## Acceptance criteria

The four criteria below on the interface are checked by the component tests (Vitest) and, on the
box, by the same calls over the API; seeing them clicked in a browser is the user's demo (the run
was autonomous).

- [ ] On the garage's page, "+" → Motion Light → tick the light, timeout 1 → Create: the instance
      appears, its journal shows "Instance created" and its start line.
- [ ] The switch disables then enables it; the row dims, then not; the journal shows each.
- [ ] Settings → change the timeout → Save: the instance restarts with it; Delete removes it.
- [ ] A required slot left empty shows "Lights is required" and sends nothing; a refusal from the
      host shows its text under the form.
- [x] Deleting the light of a running instance: the instance is disabled with "Disabled: At least
      one light is required" and "Equipment removed from settings: <name>" in its log; no order is
      sent to the deleted light; nothing crashes.
- [x] Deleting one of two lights of a running instance: it restarts with the other light only.
- [x] A device removed emits `equipment.updated` for the equipment that lost its bindings, and the
      recipe using it is disabled.
- [x] The first load of the interface stays under the budget (170 KB gzip).
- [x] On the box: the J3 flow over the API (below).

## Edge cases

- `GET /recipes` empty, or failing: no Recipes panel (the room page still works).
- A room with no light: the lights field is empty; Create shows "Lights is required" (a required
  list with nothing ticked).
- An instance of a recipe the host no longer has (`recipeId` unknown): the row shows the id as its
  name; its journal's header shows **Delete** instead of Settings (there are no slots to build a
  form); the switch stays.
- Two browsers: one deletes the instance the other shows; the other's next action gets 404,
  shown 5 s with the interface's text for a missing item, and the instances are read again.
- An instance whose recipe the host does not know (after a rollback) is disabled by the next
  settle (`Disabled: Recipe not found: <id>`), and enable then refuses it, even once the recipe is
  back (until it is created again).
- A button's device or `action` binding removed disables the whole instance (`Button "<name>" has
  no "action" data binding`), as a creation would refuse it; the light stops following motion
  until the button is taken out in Settings (roadmap inbox: prune such a button instead).
- The 17th instance: the host's refusal `Too many recipe instances (max 16)` under the form.
- A settle over 16 instances with 16 lights each: bounded by the existing bounds (16 x 16 ids
  checked per change), no new collection.
- An equipment changed to another room, or its type changed: R4.2 disables the instance that uses
  it (the light no longer belongs to the zone, or is no longer `light_onoff`).
- An equipment renamed: the instance is untouched (R4.5).
- An enabled instance that failed to restore (not running) and no longer validates after a change:
  disabled (R4.2), so the switch shows the truth.

## Memory, size and requests

- **Interface**: today's first load is 51.8 KB gzip (`docs/design-system.md`); this spec adds about
  10 to 14 KB (the recipe components, the store, the messages): about 65 KB expected, under the
  170 KB budget; the copy embedded in the radio image grows by the same (its partition holds
  4 MB). `npm run check:bundle` and the four firmware images stay green.
- **Requests**: one `GET .../log?limit=50` every 5 s while a journal is open (one at a time, none
  when it is closed; not paused in a hidden tab, as Sowel's); `GET /recipes` and `GET
  /recipe-instances` at each `/ws` opening; one `GET /recipe-instances` per burst of recipe events
  (coalesced). Within the box's limit of 4 requests at once.
- **Core**: no new collection; a settle walks at most 16 instances x 32 ids.

## Sowel contract

- Types: `RecipeInfo`, `RecipeSlotDef`, `RecipeLangPack`, `RecipeInstance`, `RecipeLogEntry` of
  `src/shared/types.ts` (T:680-853), mirrored in `ui/src/types.ts`; the instance list's `state` as
  Sowel's `GET /recipe-instances` (spec 012).
- Routes: Sowel's `src/api/routes/recipes.ts`, unchanged (spec 012).
- Interface: Sowel's `ui/src/components/recipes/` (`ZoneRecipesSection`, `RecipePickerPopover`,
  `AddRecipeForm`, `RecipeInstanceRow`, `RecipeJournal`, `recipe-form-fields`'s
  `EquipmentCheckboxList`, `DurationInput`, `CountdownTimer`, `recipe-slot-helpers`'s
  `durationToMinutes`), `ui/src/lib/recipe-i18n.ts`, `ui/src/store/useRecipes.ts`,
  `ui/src/api/recipes.ts` (the `recipes.*` messages of `ui/src/i18n/locales/*.json`), cut down as
  the architecture says.
- Events: `recipe.instance.*` of `EngineEvent` (unchanged); `equipment.updated` on a device's
  removal (R5).
- **Divergence** (decision D38): Sowel leaves stale ids in a running instance; Solite prunes them,
  disables an instance that no longer validates, deletes the instances of a deleted zone, and
  refuses to enable an instance that does not validate (R4.7).

## How the user will check it

**On the Mac** (simulator): `cargo run -p solite-host -- --sim scenarios/garage.json --serve
--speed 10`, then `sh scripts/demo-garage.sh` (it makes the garage and its recipe); open `http://127.0.0.1:8080/#/zone/<garage>`: the Recipes panel shows the
instance; disable/enable it, open its journal, change its timeout, create a second one, delete it.

**On the box** (Dongle Max at 192.168.0.46, garage with the ZBMINIR2 and the motion sensor):
flash the radio image with `--keep-storage` (the user's yes, D6, is in this run's brief), then, over the API, run the calls the interface makes:
create a test instance on the garage's light, read its log, disable and enable it, change its
timeout, make a throwaway light equipment on the relay, add it to the test instance, delete that
equipment (the instance restarts with the garage light only), then delete the test instance; the
garage's own instance is left as it was.

Result of the run: see "Real-hardware run" below.

### Real-hardware run

2026-11-23, the Dongle Max at 192.168.0.46 (ZBMINIR2 relay as "Garage light", SNZB-03PR2 motion
sensor, the garage's own `motion-light` instance of spec 023), one pass, no loop (D39, proposed):

- The radio image of this spec flashed with `box.sh flash --image radio --keep-storage`, booted:
  `solite-ui 4 files, 57 KB`, storage 20 KB used, Wi-Fi joined; the home, the network and the
  garage kept.
- `GET /recipes`: motion-light with its 7 slots (the form's source).
- Create a test instance (garage light, timeout `1m`, the form's body with empty optional slots):
  201; log `Instance created`, `Recipe activated — no motion, lights off`.
- Disable, enable: `{"success":true}` each. Settings (timeout `2m`): saved, restarted.
- A throwaway `J3 test light` made on the same relay and added to the instance, then deleted: 204;
  the instance stays enabled with the garage light only, its log `Equipment removed from settings:
  J3 test light` then a new start line (R4.1, R4.3).
- The test instance deleted: 204. The garage's own instance untouched (enabled, `20s`). `GET /`
  answers 200 (the interface with the Recipes panel is in the image).
- **Flash writes caused**: one image flash (bootloader, partition table, the 1.8 MB application;
  `nvs` and `otadata` erased; storage kept) and about 9 file saves on `/data` (7 of
  `recipes.json`, 2 of `equipments.json`), besides the home's usual saves of device data.
- Not run on the box: a deleted light disabling the instance (the garage light stays), a device
  removal, the interface clicked in a browser.
