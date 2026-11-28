# Spec 053: Script recipes up to 32 KB

Asked by the user on 2026-11-28 ("oui, monte à 32 Ko et enchaîne"), to bring Sowel's larger
recipes (F3).

## Context
A script recipe's source is at most 16 KB (`MAX_SCRIPT_BYTES`,
`crates/core/src/recipes/script/limits.rs:15`), its signed file at most a request body
(`MAX_FILE_BYTES` = `MAX_BODY_BYTES`, 16 KB, `crates/core/src/api/mod.rs:40`; the catalogue's
`size` at most 16 KB). `motion-light` is 15.3 KB. Sowel's other recipes, ported, are about 17 to
26 KB (`schedule-on-off` ~17, `state-trigger-light` ~20, `presence-thermostat` ~22,
`motion-light-dimmable` ~26, from their TypeScript sizes and `switch-light`'s measured ratio).

## Goals
- A script recipe of up to 32 KB of source installs, uploads, loads and runs, on the Mac and the
  box, with the memory bounds measured.
- At most 4 script recipes loaded (2 before), 4 instances of each (8 before), 16 in all (unchanged):
  the user's decisions of 2026-11-28, added to this spec (6 then 12 in all was an earlier choice of
  the same day). The runtime's memory limit is checked against them (R2).

## Out of scope
- Porting the larger recipes themselves is out of scope (below); the 90 % rule of D74 is kept.
- Porting the larger recipes themselves (the next specs of F3).

## Requirements
### R1: the bounds
- `MAX_SCRIPT_BYTES` 32 KB; a request body (`MAX_BODY_BYTES`) 32 KB + 128 bytes (the signed
  header), so a signed file of a 32 KB script passes (the body bound is the largest signed file);
  the catalogue's `size` at most the same.
- `MAX_SCRIPT_RECIPES` 4 (a fifth is refused, `at most 4`), `MAX_INSTANCES_PER_SCRIPT` 4 (a fifth is
  refused, `max 4`), `MAX_INSTANCES` 16 (4 x 4; a 17th is refused).
- `SCRIPT_MEMORY_LIMIT` stays 224 KB: 4 runtimes of 224 KB are 54 KB over the PSRAM budget on
  paper (all four at their limit at once), which the memory budget says; 208 KB was tried and the
  32 KB script ran out of memory when loaded in QEMU.
- The box's HTTP workers' body buffers grow with `MAX_BODY_BYTES` (4 × 32 KB in PSRAM).
### R2: measured
- The engine's worst case (`memory_worst_case`, which counts the 4 bodies) stays under its 1 MiB
  ceiling; its figure written in `docs/memory-budget.md`.
- A 32 KB script (a test script of real code, not padding) loads and runs 4 instances under the
  224 KB limit, on the Mac's count and in QEMU (its `script sizes:` line), figures written.
- The radio image's PSRAM budget line updated (the bodies' 64 KB more).

## Living spec delta
### docs/living-spec/README.md
- Every "16 KB" of a script's source, a signed file, a request body, the catalogue's `size` → 32 KB
  (the file and the body, 32 KB + 128 bytes); 8 instances of a script → 4; 2 script recipes → 4.
### docs/memory-budget.md
- The bodies' line (4 × 32 KB), the worst case, the script sizes.
### No longer true
- D74's and D78's 16 KB, D74's 2 recipes, 8 instances and 224 KB, D81's 8 instances: a new decision D85 replaces them
  (never an edit).

## Acceptance criteria
- [ ] A 32 KB script is accepted, a 32 KB + 1 one refused with the existing text (tested).
- [ ] A signed file of a 32 KB script uploads (tested on the Mac's API).
- [ ] A request body of 32 KB + 128 bytes passes, one byte more is 413 (tested).
- [ ] A fifth instance of a script and a fifth script recipe are refused (tested); a 17th instance in all is refused (tested).
- [ ] `memory_worst_case` under 1 MiB, its figure written.
- [ ] The 32 KB script with 4 instances under 224 KB (Mac and QEMU), its figures written.
- [ ] validate green, QEMU included.

## Edge cases
- A catalogue entry of `size` over the bound: refused as today, the bound being 32 KB.
- The script's bytecode for 32 KB of source: counted in the 224 KB (measured).

## Sowel contract
None (a Solite bound).

## How the user will check it
The measured figures in the memory budget; then the next F3 recipe (`motion-light-dimmable`,
about 26 KB) installs on the box.
