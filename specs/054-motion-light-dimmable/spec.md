# Spec 054: `motion-light-dimmable`, Sowel's recipe as a script (F3)

Written in an autonomous run (user, 2026-11-28: "enchaîne suivant roadmap en autonomie"). The
user's rule (2026-11-27): Solite's recipes are Sowel's (the same id, settings and texts); only
their performance is adapted to the box.

## Context
F3: Sowel's recipes brought as scripts. `motion-light-dimmable` (`mchacher/sowel-recipe-motion-light-dimmable`,
`src/index.ts`, 38 KB of TypeScript; `../sowel/plugins/registry.json`) turns dimmable lights on
when motion is detected, with a brightness per time slot. It is the largest of Sowel's recipes,
the first to use spec 053's 32 KB. Models of a port: `recipes/scripts/motion-light.js` (spec 046),
`recipes/scripts/switch-light.js` (spec 052).

## Goals
- `motion-light-dimmable` installed from the catalogue (or a signed file), set up like Sowel's;
  motion turns its lights on at the brightness of the current time slot, off after the delay.

## Out of scope
- Sowel's other recipes (the next parts of F3).

## Requirements
### R1: the script
- `recipes/scripts/motion-light-dimmable.js`, in Sowel's script-recipe format: Sowel's id,
  version, name, description, icon, i18n, slots (ids, types, constraints, required, defaults) and
  validation texts, in Sowel's order, read from Sowel's `src/index.ts` and `manifest.json`.
- Behaviour: Sowel's, line for line, through the script API. Where the script API lacks something
  Sowel's code uses, the spec's result says what and how it is done instead.
- It fits the script limits (32 KB of source, 224 KB of memory with 4 instances, 200 ms a call).
### R2: published
- Signed with the recipe key and added to the catalogue `mchacher/solite-recipes` (a release with
  `recipes.json` holding `motion-light`, `switch-light` and `motion-light-dimmable`).

## Living spec delta
### docs/living-spec/README.md
- The recipes section: `motion-light-dimmable`, Sowel's, installed from the catalogue; what it does.
### No longer true
- The roadmap's F3 line and `docs/roadmap.html`: `motion-light-dimmable` done.

## Acceptance criteria
- [ ] `GET /recipes/motion-light-dimmable` (with `--recipe-script`) gives Sowel's info, tested
      against a pinned file written from Sowel's source.
- [ ] Each refusal of Sowel's validation answers Sowel's text (tested).
- [ ] Motion in a time slot: lights on at that slot's brightness; no motion for the delay: off
      (tested on the Mac with the simulated clock).
- [ ] The script under 32 KB; its memory with 4 instances under 224 KB (the Mac's count and QEMU).
- [ ] Signed, in the catalogue's release; the box lists it and installs it.
- [ ] validate green.

## Edge cases
- A light deleted, a sensor deleted: the generic settle (as `motion-light`).
- A time slot crossing midnight; no slot matching the time: Sowel's behaviour.
- A manual change of the light while the recipe holds it: Sowel's behaviour.

## Sowel contract
Sowel's `motion-light-dimmable` (`mchacher/sowel-recipe-motion-light-dimmable`): id, slots, info
and texts.

## How the user will check it
`--sim scenarios/garage.json --serve` with
`--recipe-script recipes/scripts/motion-light-dimmable.js`, or installed from the catalogue on the
box: an instance on the garage's motion sensor and a dimmable light.
