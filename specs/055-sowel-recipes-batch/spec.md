# Spec 055: four of Sowel's recipes as scripts, in one batch (F3)

Written in an autonomous run (user, 2026-11-28: "continue en autonomie"; recipes "par paquet",
one CI loop and one box install for several). The user's rule (2026-11-27): Solite's recipes are
Sowel's (the same id, settings and texts); only their performance is adapted to the box.

## Context
F3, after `motion-light-dimmable` (spec 054). Four of Sowel's recipes (`../sowel/plugins/registry.json`),
ported as `motion-light-dimmable.js` was:
- `schedule-on-off` (`mchacher/sowel-recipe-schedule-on-off`): on and off for on/off equipments at a
  fixed time, sunrise or sunset, up to 3 windows a day;
- `state-trigger-light` (`mchacher/sowel-recipe-state-trigger-light`): lights on for a fixed
  duration when a watched equipment's state turns to a target value;
- `presence-thermostat` (`mchacher/sowel-recipe-presence-thermostat`): a thermostat's setpoint by
  the zone's presence, with night mode, preheat and cocoon;
- `state-watch` (`mchacher/sowel-recipe-state-watch`): an alarm when an equipment's data stays in a
  watched state.

## Goals
- Each of the four installed from the catalogue, set up like Sowel's, behaving as Sowel's.

## Out of scope
- `presence-heater`: it needs the `heater` equipment type (I2).

## Requirements
### R1: the scripts
- `recipes/scripts/<id>.js` for each, in Sowel's script-recipe format: Sowel's id, version, name,
  description, icon, i18n, slots and validation texts, in Sowel's order.
- Behaviour: Sowel's, line for line, through the script API. The local time is
  `api.now().localTime` (spec 054). Where a recipe needs more of the home's local calendar (the
  weekday, the date), `api.now()` gains it the same way (`localWeekday` 0 = Sunday as
  `Date.getDay()`, `localDate` "YYYY-MM-DD"), in the home's time zone.
- Each fits the script limits (32 KB of source, 224 KB of memory with 4 instances, 200 ms a call).
### R2: published
- Signed with the recipe key and added to the catalogue in one release with the other recipes.

## Living spec delta
### docs/living-spec/README.md
- The recipes section: the four recipes, Sowel's, installed from the catalogue; what each does.
- The script API: any field added to `api.now()`.
### No longer true
- The roadmap's F3 line and `docs/roadmap.html`: the four recipes done.

## Acceptance criteria
- [ ] Each recipe's info matches a pinned file generated from Sowel's `createRecipe()` (tested).
- [ ] Each of Sowel's validation refusals answers Sowel's text (tested).
- [ ] Each recipe's main behaviour on the Mac with the simulated clock (tested): a window's on and
      off; a state change turning lights on for the duration; the setpoint by presence; the alarm
      after the watched duration.
- [ ] Each under 32 KB; 4 instances of each under 224 KB (the Mac's count).
- [ ] Signed, in the catalogue's release; the box lists them and installs them.
- [ ] validate green.

## Edge cases
- A deleted equipment: the generic settle. A time zone other than UTC: the local slots.
- A window or slot crossing midnight: Sowel's behaviour.

## Sowel contract
The four repositories above: ids, slots, info and texts.

## How the user will check it
Installed from the catalogue on the box, or `--recipe-script recipes/scripts/<id>.js` on the Mac.
