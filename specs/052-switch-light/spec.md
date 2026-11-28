# Spec 052: `switch-light`, Sowel's recipe as a script (F3, first recipe)

Written in an autonomous run (user, 2026-11-28: "enchaîne le dev en autonomie"). The user's rule
(2026-11-27): Solite's recipes are Sowel's (the same id, settings and texts); only their
performance is adapted to the box.

## Context
F3 (the roadmap's rank 1): Sowel's recipes brought as scripts. `switch-light` 1.0.0
(`mchacher/sowel-recipe-switch-light`, `src/index.ts`, 317 lines; `../sowel/plugins/registry.json`)
toggles lights on any press of a button: a room without a motion sensor, driven by wall switches,
remotes or smart buttons. Solite can now feed it: a button's `action` (spec 049).
`motion-light` 1.1.2 is the model of a port (`recipes/scripts/motion-light.js`, spec 046).

## Goals
- `switch-light` installed from the catalogue (or a signed file), set up like Sowel's, a press of
  any of its buttons toggles its lights.

## Out of scope
- Sowel's other recipes (the next parts of F3).
- Device-specific button names (generic `action` values, spec 049).

## Requirements
### R1: the script
- `recipes/scripts/switch-light.js`, in Sowel's script-recipe format, as `motion-light.js`: id
  `switch-light`, version `1.0.0`, Sowel's name, description, icon, i18n, slots (ids, types,
  constraints, required, defaults) and validation texts, in Sowel's order, read from Sowel's
  `src/index.ts` and `manifest.json`.
- Behaviour: Sowel's, line for line, through the script API's calls (`api.on`, orders, timers,
  log). Where the script API lacks something Sowel's code uses, the spec's result says what and how
  it is done instead (the user's rule: only performance is adapted).
- It fits the script limits (16 KB of source, 224 KB of memory, 200 ms a call).
### R2: published
- Signed with the recipe key and added to the catalogue `mchacher/solite-recipes` (a release with
  `recipes.json` holding `motion-light` 1.1.2 and `switch-light` 1.0.0).

## Living spec delta
### docs/living-spec/README.md
- The recipes section: `switch-light`, Sowel's 1.0.0, installed from the catalogue; what it does.
### No longer true
- The roadmap's F3 line: `switch-light` done.

## Acceptance criteria
- [ ] `GET /recipes/switch-light` (with `--recipe-script`) gives Sowel's info: id, name, slots,
      i18n (tested against a pinned file written from Sowel's source).
- [ ] Each refusal of Sowel's validation answers Sowel's text (tested).
- [ ] A simulated button's `toggle`, then another press: lights on, then off (tested on the Mac).
- [ ] The memory of the script with 8 instances under the 224 KB limit (QEMU or the Mac's count).
- [ ] Signed, in the catalogue's release; the box lists it.
- [ ] validate green.

## Edge cases
- A button that is not in the slot: ignored.
- A light deleted: the generic settle (as `motion-light`).
- Two presses within a call: each toggles (each press is an event, spec 049).

## Sowel contract
Sowel's `switch-light` 1.0.0 (`mchacher/sowel-recipe-switch-light`): id, slots, info and texts.

## How the user will check it
`--sim scenarios/button.json --serve` with `--recipe-script recipes/scripts/switch-light.js`:
an instance on the simulated button and light; the button's presses toggle the light.
