# Spec 046: the native motion-light recipe removed, `motion-light` a script recipe

Built on top of spec 042 (F2). Reviewed by the spec reviewer and approved by the user on 2026-11-27.

## Context

The roadmap inbox line "Right after F2" (user, 2026-11-27): remove the native (Rust) `motion-light`
recipe so that every recipe on the box comes from the catalogue; keep the recipe SDK
(`crates/recipe-sdk`, the engine's contract). Since F2 the radio image carries no script but still
carries the native recipe's code, and two implementations of one behaviour are tested against each
other (`crates/core/tests/script_parity.rs`). The user's principle (2026-11-27): **the recipes are
Sowel's, only adapted for performance to Solite's context.**

## Goals

1. No recipe is built into the engine: `crates/recipes/motion-light/` and its registration go.
2. The script recipe takes Sowel's id `motion-light`, Sowel 1.1.2's info and texts, version `1.1.2`:
   a home saved with native instances finds its recipe again once the script is installed.
3. Everything that used the native recipe uses the script, and the memory ceilings still measure a
   home with its recipes running.

## Out of scope

- An id alias (`motion-light-script` → `motion-light`) at load: no box but the user's runs a script
  instance (F2 is not released).
- The box installing on its own the recipes it proposes (a new box has no recipe until it reaches
  the catalogue or receives a file): accepted for this spec, a roadmap inbox line (R9).
- QuickJS made unconditional (the `script-recipes` feature stays; a build without it has no recipe).
- Spec 045 (the script stacks in PSRAM), the watchdog bug.

## Requirements

### R1: no recipe built in
- `RecipeEngine::new()` registers no native recipe (`crates/core/src/recipes/mod.rs:572-580`;
  `placeholder()` merges into it). The crate `solite-recipe-motion-light` leaves the workspace
  (`Cargo.toml:7`) and the core (`crates/core/Cargo.toml:23`), its 7 tests with it (R4).
- `GET /api/v1/recipes` lists only the loaded scripts: `[]` on a box with nothing installed, and in
  any build without `script-recipes`, where creating an instance answers 404 `Recipe not found:
  motion-light` (`recipes/mod.rs:1237`).
- Builds without `script-recipes` still compile under `-D warnings`: the feature-off
  `is_built_in_recipe` (`home/install_ops.rs:155-158`, called unconditionally at :128) returns
  `false`; `Native::new` (`recipes/mod.rs:191`), now used only by tests, becomes `#[cfg(test)]`.
- `<id> is built in` (409) applies only to a script given on the Mac with `--recipe-script`: an
  installed `scripts/motion-light.srs` beside `--recipe-script` of the same id is refused at boot
  with that text, the `--recipe-script` one runs.

### R2: the script is Sowel's motion-light 1.1.2
- File `recipes/scripts/motion-light.js` (renamed from `motion-light-script.js`), declaring
  `id: "motion-light"`, `version: "1.1.2"`, and Sowel 1.1.2's `name` (`Motion Light`),
  `description`, `icon` and `i18n` (French included), as pinned in
  `crates/core/tests/pinned/recipe_motion_light.json` (from the native `info.rs:14-50`).
- `GET /api/v1/recipes/motion-light` gives the pinned bytes; a slot constraint whose `min` comes
  out as `0.0` from a script (`script_parity.rs:953-954`) is written so the bytes match, or the
  spec's test names the difference (decided at build, stated in the plan).
- The file's header comment says it is the port of Sowel's motion-light 1.1.2.

### R3: Sowel's validation texts (user, question 1)
A refused setting answers Sowel 1.1.2's text, as the native did (`crates/recipes/motion-light/src/
params.rs`): `Zone parameter is required`, `At least one light is required`, `Light equipment not
found: <id>`, `Invalid duration format: <v>. Use e.g. "10m", "30s", "1h"`, the list bound's
`At most 16 lights`, and the others of `script_parity.rs:971-1016`. The host runs the script's
`validate` before its generic slot check, so a refusal the script makes carries the script's text;
the generic check stays for what a script does not check (any other script keeps the host's texts).
A `validate` that throws, or runs out of its 200 ms, on settings of the wrong shape is not a
refusal text: the generic check then answers with its own text (tested), so a script can never
make a setting pass that the slot definitions refuse.

### R4: the script's behaviour where it differed (user, 2026-11-27)
Kept as the script does it, and stated in the living spec: a deleted equipment leaves the settings
by the generic settle; a button that loses its `action` binding disables the instance (the native
dropped the button, D46); the legacy `light` key is not read; a refused order's reason is logged;
the 2 s echo window is counted on the wall clock. These five are adaptations of Solite's engine,
not of the recipe's rules.

### R5: at most 8 instances (user, question 2)
`motion-light` follows the script bound: at most 8 instances, all zones together
(`MAX_INSTANCES_PER_SCRIPT`, `recipes/mod.rs:62`, D74), against 16 for the native. A ninth is
refused: 400 `Too many instances of motion-light (max 8)`; at boot, the ninth and later stay
stopped with that text. `MAX_INSTANCES` (16) still bounds all recipes together.

### R6: a home saved with native instances
At boot, an instance whose recipe is not installed stays stopped with `Failed to restore: Recipe
not found: motion-light` (`recipes/mod.rs:1706-1710`), keeps its settings, and starts as soon as
`motion-light` is installed (F2 starts the kept instances of an installed recipe) or at the next
boot. Its settings are checked by the script on start; an instance with only the legacy `light` key
stays stopped with `Failed to restore: At least one light is required` (the restore path adds no `Invalid params:`) (exact text
fixed by R3, checked in a test).

### R7: behaviour tests
`script_parity.rs` becomes the recipe's suite, `crates/core/tests/motion_light.rs` (the old native
suite of that name is deleted): every `parity_*` scenario on the script; the `difference_*` tests
become plain tests of R3 and R4; the native-only tests are ported where the script must behave the
same, among them `a_boot_by_day_starts_the_recipe_with_the_daylight_before_any_aggregate` (a box
restarting by day must not switch the light, now through F2's boot load). Each test dropped is
listed in the plan with its reason.

### R8: memory ceilings measure the recipes (user, question 5, option A)
- QEMU (D27, 350 KB): the script phase loads `motion-light` first, then starts the realistic home's
  3 kept instances, then measures; the qemu.sh lines are checked as today.
- Mac (D20, 300 KB, `memory_ceiling.rs`; `memory_worst_case.rs`, 676 KB, D80): the realistic home's
  3 instances and the worst case's instances are script instances. QuickJS's memory is invisible to
  the Mac's counting allocator (`memory_worst_case.rs:44-46`), so these tests count the Rust side,
  and the scripts' runtimes are a line of their own in `docs/memory-budget.md` (224 KB each on the
  box). The worst case at 16 lights needs 2 instances of 8 lights, or 2 script recipes: its header's
  bound lines (32, 57-59, enforced by `every_memory_bound_is_listed_in_the_header`) name the
  script's bounds instead of `params.rs`'s.
- `lock_time.rs`: its native run (:40-99) and its 16-instance case (:207) go; the script run
  (:102-185) covers 8.
- The new figures go into `docs/memory-budget.md` and the decision.

### R9: Mac tools, box session, fixtures
- Every Mac launcher passes `--recipe-script recipes/scripts/motion-light.js` (the host's recipe
  key is compiled in, `crates/host/src/serve.rs:56`: no throwaway-key install on the Mac):
  `scripts/e2e-sim.sh:55`, `crates/host/tests/{demo_script.rs:71,sim.rs:289-301}`, `CLAUDE.md:57`,
  `README.md:55`. `scripts/demo-garage.sh` stops with `the box has no motion-light recipe: install
  it first` when `GET /recipes` lists none.
- `crates/host/tests/recipe_script.rs:72` (an id `motion-light` refused as built in) now loads.
- The realistic home (`crates/core/tests/realistic/mod.rs:115-162`, `firmware/fixtures/
  recipes.json`) keeps its 3 `motion-light` instances, unchanged on disk.
- `box-session.sh` B11 checks the garage's own `motion-light` instance (a wave: light on, then off
  after its timeout), installing the signed file given to the session first if the recipe is
  missing; it never uninstalls a recipe an instance on the box uses. The copy-and-compare flow
  (:838-996) and its cleanup (:884-904) go; `scripts/lib/firmware.sh:237-299` (the ELF search for
  `id: "motion-light-script"`, the log lines) and `scripts/qemu.sh:155` search the new id.
- A roadmap inbox line: "the box installs on its own the recipes it proposes" (user, question 4).

### R10: an instance without its recipe says so (user, after the box demo of 2026-11-27)
- On the zone page a row whose recipe id is absent from `GET /recipes` (the row decides it from
  the list, never from the log text) shows its switch off and disabled (it cannot run, although
  the instance is `enabled` in the saved home), the row dimmed, a "Recipe not installed" /
  "Recette non installée" label and a visible Delete with the existing confirmation. A row whose
  recipe exists is unchanged.
- An uninstall deletes the instances (spec 042), so these rows come from a restore on another box
  or a home saved with the native recipe before this spec. The "No recipe is installed" hint with
  the link to `#/plugins` stays when no recipe at all is installed.
- `ui/src/components/recipes/RecipeInstanceRow.tsx`; the journal's header offers Settings only
  (Delete moved to the row).

## Living spec delta

### docs/living-spec/README.md
- 105-117 (QEMU phase) and its log line at 109: the phase loads the script, then starts the kept
  instances; no swap of native instances.
- 760-765 "Runs the motion-light recipe ... the same id, settings, messages and behaviour as
  Sowel's 1.1.2 ... It is built in" → `motion-light` is a script recipe installed from the
  catalogue, Sowel 1.1.2's id, settings and messages, with the five behaviours of R4.
- 788 "an entry beyond 16": unchanged in text (Sowel's `At most 16 lights`, R3).
- 791-803 (legacy `light` emptied at settle; a button that leaves the settings instead of disabling,
  D46) → R4's behaviours.
- 813 "16 instances" → 8 (R5).
- 821-822 "like the built-in one", "after the built-in recipes" → the order of loaded scripts.
- 920 "for a native recipe", 953 "(the built-in recipes get each event at once ...)" → removed.
- 940-944 "`motion-light-script` ... differs from the built-in recipe" → removed (R2-R4 say it).
- 1826 (B11's `motion-light-script`) → B11 as R9.
- 1919 "`motion-light` is its only built-in recipe" → no recipe is built in.

### No longer true
- `docs/architecture.md:44,46,65,101` (the motion-light crate row and list).
- `docs/memory-budget.md:19,23-24,62,130-132,194-199,236`, the image size.
- `docs/sowel-subset.md:78,91,92,132`.
- `docs/roadmap.md` 208 (B6 "native Rust"), 220, 326; the inbox line 246 goes (done).
- `crates/recipe-sdk/src/lib.rs:6`, `bounds.rs:6,10`; `crates/core/src/memory.rs:170`;
  `crates/core/src/recipes/script/limits.rs:25,29` (comments naming the native recipe).
- `CLAUDE.md:57`, `README.md:55` (R9).
- Decisions: a new D81 supersedes D17 (native first) and D46 (button pruned), and states its effect
  on D20 and D27 (what the ceilings measure, R8) and D74 (8 instances now bound `motion-light`).
  Never an edit of the old ones.

## Acceptance criteria
- [x] `cargo tree -p solite-core` shows no `solite-recipe-motion-light`; the radio image is smaller
      (figure in the memory budget).
- [x] `GET /recipes/motion-light` gives the pinned bytes of Sowel 1.1.2 (or the named difference).
- [x] Each refused setting of R3 answers Sowel's text (tested).
- [x] A saved home with 3 native instances: stopped with `Recipe not found: motion-light`, then
      running after the script is installed, settings unchanged (Mac test and Mac demo).
- [x] Every scenario of the former parity suite passes on the script.
- [x] validate green, QEMU included, its memory gate measured with the 3 instances running.
- [x] Box: the garage runs `motion-light` 1.1.2 from a signed file and still runs after B11.

## Edge cases
- A ninth `motion-light` instance, any zone: refused (R5).
- An instance with only the legacy `light` key: stopped, Sowel's text (R6).
- A box holding `motion-light-script` 1.0.0 (F2's file) and `motion-light` 1.1.2: two recipes (the
  limit, `MAX_SCRIPT_RECIPES` = 2); a third install is refused `Too many script recipes (at most
  2)`; the user uninstalls `motion-light-script`.
- `GET /recipes` empty: the zone page's recipe section shows no recipe and points to the Plugins page of Administration
  (UI test).
- On the Mac, `--recipe-script` and an installed file of the same id: the file refused at boot (R1).

## Sowel contract
Sowel's recipe `motion-light` (`../sowel/plugins/registry.json:262-269`, repository
`mchacher/sowel-recipe-motion-light`, version 1.1.2): the same id, slots, info and validation
texts. The five behaviours of R4 are the engine's adaptations (D81), as the native's D17
divergences were.

## Hardware and demo
- Mac, no box: start without `--recipe-script` on a saved home with motion-light instances
  (stopped, `Recipe not found: motion-light`), restart with it (running, settings unchanged).
- Box, with the user: the image built with `release/recipe-key.pub` (made on 2026-11-27, in the F2
  PR), flashed with `--keep-storage` on the user's `--yes-flash`; `motion-light` 1.1.2 signed with
  the recipe key and uploaded (the catalogue `mchacher/solite-recipes`, the user's to create,
  publishes `motion-light` 1.1.2 and drops `motion-light-script` when it exists); the garage's
  instance runs; B11 green.

### Result (2026-11-27)
- Mac, `firmware/fixtures` realistic home (3 `motion-light` instances): without `--recipe-script`,
  `GET /recipes` is `[]`, the three instances stay `enabled` with their settings, each log says
  `Failed to restore: Recipe not found: motion-light`, and the zone page shows the rows dimmed,
  "Recette non installée", the switch off and disabled (seen by the user). Restarted with
  `--recipe-script recipes/scripts/motion-light.js`: `motion-light` 1.1.2 loaded, the three
  instances running (`Motion detected — 16 light(s) turned on`), their settings byte for byte the
  saved ones. A new instance without a zone: 400 `Invalid params: Zone parameter is required`.
  The 16 `queue of recipe orders is full` warnings at the start are the known ones
  (`docs/memory-budget.md`, orders at the start).
- Box (Dongle Max, the user's garage): `motion-light` 1.1.2 installed from a file signed with the
  recipe key, the garage's instance running on it; the Plugins page of Administration; an
  uninstall deleting the instances, their number shown in its confirmation; B11 green.

## Decided by the user (2026-11-27)
1. The id is `motion-light`, Sowel's.
2. Where the script differs in behaviour, the script's is kept (R4); the recipes are Sowel's, only
   adapted for performance to Solite.
3. Sowel's validation texts (R3).
4. At most 8 instances (R5).
5. Version 1.1.2, the port of Sowel's 1.1.2.
6. A new box has no recipe until the catalogue or an upload; the box installing its proposed
   recipes on its own is a roadmap line.
7. The memory ceilings measure the home with its recipes running (option A, R8).