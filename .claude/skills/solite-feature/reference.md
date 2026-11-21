# Solite Feature Reference

Templates for the feature workflow: [SKILL.md](SKILL.md).

## Key paths

| Topic | Path |
| --- | --- |
| Rules and commands | `CLAUDE.md` |
| What the tool does | `docs/living-spec/` |
| Specs | `specs/NNN-name/`, index in `docs/specs-index.md` |
| Decisions, roadmap, architecture | `docs/decisions.md`, `docs/roadmap.md`, `docs/architecture.md` |
| Code | `crates/core/src/` (core), `crates/host/src/` (Mac), `firmware/src/` (ESP32), `ui/src/` (interface) |
| Tests | `crates/<crate>/tests/`, in-module `#[cfg(test)]`, `ui/src/**/*.test.tsx` |
| Design system | `docs/design-system.md`, `ui/src/styles/` |
| Sowel contract | `../sowel/src/shared/types.ts`, `../sowel/docs/technical/` (see `CLAUDE.md`) |

## Commit scopes

`core`, `host`, `firmware`, `ui`, `tools`, `docs`, `ci`. Example: `feat(core): zones aggregate motion`.

## spec.md

```markdown
# Spec NNN: <name>

## Context

Why this change, for whom. Link to the documents in `inputs/` if any.

## Goals

1. ...

## Out of scope

- ...

## Requirements

### R1: <name>

Behaviour, inputs, outputs (with units), what happens with missing or wrong data.

## Living spec delta

### docs/living-spec/<page>.md

- New: <what the tool now does, in business words>
- Changed: <before> → <after>, why

### No longer true

Sentences elsewhere in the docs (living spec, architecture, README, CLAUDE.md, skills) that this
change makes false, found by the search of the feature skill (3.4):

- <file>: <the sentence> → <rewritten in the present tense>, or "none found: <the names searched>"

## Acceptance criteria

- [ ] <something the user can see or check>

## Edge cases

- ...

## Sowel contract

The Sowel types, fields, events or recipe slots this change mirrors, with their file in the
Sowel repository; any divergence, and its decision. "None" if the change exposes nothing.

## How the user will check it

The demo: <what will be shown, with which data: the simulated radio's scenario, a real device
(model), the box>. With real hardware: the result of the run (date, device, what was seen).
```

## architecture.md

```markdown
# Architecture: Spec NNN

## Overview

A short diagram: inputs → processing → outputs.

## Data

New or changed data: name, type, unit, where it is stored.

## Files changed

| File | Change |
| ---- | ------ |
```

## plan.md

```markdown
# Plan: Spec NNN

## Steps

- [ ] 1. Living spec updated
- [ ] 2. ...

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| ... | Nominal case | ... |
| ... | Missing data | ... |
| ... | Existing behaviour unchanged | ... |

## Validation

- [ ] Validate command green
- [ ] Agent review done, blocking findings fixed
- [ ] Demo seen and approved by the user
```

## specs-index.md row

```markdown
| NNN | <name> | <one line> | <date> | done / in progress |
```
