# Spec 041: Script recipes in JavaScript, run by QuickJS (the proof)

## Context

First increment of milestone F (`docs/roadmap.md`, ranked first by the user on 2026-11-26):
recipes that live outside the firmware, so that a recipe is added or updated without a new
firmware. Decision D1 named WebAssembly (`wasmi`); D17 kept recipes in native Rust "until milestone
F".

This spec first followed D1. Measured, a WebAssembly recipe costs far too much memory on the box:
about 1.1 MB in `wasmi` as built, 376 KB after a week of shrinking, 150 to 215 KB in WAMR with a C
runtime (`research.md`). The user then asked for another way; the research and a spike on the
32-bit target found **QuickJS** (a small JavaScript engine in C, through the Rust crate `rquickjs`):
about 60 KB per recipe in PSRAM, about 0.5 KB of internal RAM, 752 KB of flash (measured; 422 KB was the estimate), its limits
(memory, time, stack) working without bringing the firmware down. And Sowel's own roadmap runs
**script recipes in QuickJS** (`roadmap/ai-script-recipes/spec.md` in Sowel): one JavaScript recipe
can run in both. Decided with the user on 2026-11-26: rewrite this spec around QuickJS, build it in
autonomy up to the test on the box (the flash itself stays the user's, D6).

What exists and stays (built on this branch before the change of runtime, "pass A"): the recipe
contract in `crates/recipe-sdk` (the `Recipe` and `Host` traits, events, views, results),
`motion-light` moved to `crates/recipes/motion-light` on that contract, the core's `NativeHost`, a
registry of recipe providers, the state text and listen set cached by the engine, settle through
`Recipe::prune`, the params depth check, and today's JSON pinned byte for byte. What goes:
everything WebAssembly (the SDK's `guest` feature, `crates/recipes/motion-light-wasm`,
`scripts/build-recipes.sh`, the `wasm32` target, the `recipe` profile).

Choices made in the autonomous run, after the spec review (recorded in D73 to D76): D11 stays (no
`unsafe` in the core or the host: the memory limit is QuickJS's own, the box's allocator is the
firmware's); one QuickJS runtime per script recipe, shared by its instances (one instance's runaway
stops the recipe's instances); a motion-report hook added to Sowel's draft API so the safety
auto-off works; the radio image, and so the releases, carry QuickJS and the built-in script; the box
test stays in this spec.

## Goals

1. A recipe written in JavaScript, in Sowel's script-recipe format, runs in Solite through Sowel's
   recipe routes like a built-in one: on the Mac, in QEMU and on the box.
2. `motion-light-script`, a port of `motion-light` in that format, gives the same orders and the same
   state as the built-in `motion-light` in the behaviour scenarios they share (tested both ways); its
   differences are listed and tested.
3. A script sees only the equipments of its own settings; a script that loops, eats memory or
   recurses is stopped, the rest of the engine runs on, and it comes back by itself within a bound.
4. The cost is known on the target: RAM (PSRAM and internal), stacks, flash, the time of a call on
   the real box.

## Out of scope

- Downloading, signing, installing, updating or removing a script at run time (F2, with a key of
  its own). Here a script reaches the host by a command-line flag and the box by being built in.
- Porting other Sowel recipes (F3).
- Sowel's script API calls Solite cannot serve yet: `mode()` (no modes, C1), the tariff period in
  `now()`, `energy.claim/release`, history reads, the observation mode, a per-instance order rate
  limit, `state.alarm` semantics. A script calling them gets an error (R2.9).
- `await` on host calls: Solite's host calls are synchronous (a superset of Sowel's draft, where they
  resolve through promises); pending jobs are still run after each call (R2.10).
- TypeScript sources; a script is JavaScript.
- Replacing the built-in `motion-light` (unchanged).
- Any interface change: the room page lists `motion-light-script` and builds its form from its slots
  as for any recipe.

## Requirements

### R1: A script recipe, in Sowel's format

1. A script recipe is **one JavaScript source text** (at most `MAX_SCRIPT_BYTES`, 16 KB) that calls
   `recipe({...})` exactly once at its top level: `recipe({ api: 1, id, name, description, slots,
   icon?, i18n?, validate?, start })`, Sowel's draft shape, plus `id` (1 to 64 characters among
   `a-z`, `0-9`, `-`; Sowel's draft has none) and an optional `version` (shown in the start line only;
   Sowel's `RecipeInfo` has no such field). `api` other than 1 is refused.
2. **Slots**, an object by slot id, each Sowel's `RecipeSlotDef` (`src/shared/types.ts` T:680):
   `type` among `zone`, `equipment`, `number`, `duration`, `time`, `boolean`, `select` (`text` and
   `data-key` refused in this spec), `name` (default: the id), `description` (default `""`),
   `required` (default `false`), `list`, `defaultValue` (or `default`, as Sowel's draft example
   writes), `constraints` (`equipmentType` one or several, `min`, `max`, `crossZone`,
   `includeDescendants`; `equipmentType` at the slot's top level is read too, as in the draft's
   example), `options`. The slot `zone` (type `zone`, required) is added first when the script does
   not declare it. The engine serves Sowel's `RecipeInfo` (`id`, `name`, `description`, `slots` as an
   array in declaration order, `icon` and `i18n` passed through), from JSON cached at load, never by
   a call into the script.
3. **Validation is the host's, from the slots**, before the script runs; Solite's own texts (Sowel
   has no generic slot validation: each recipe checks itself), all of the form `Invalid params:
   <text>`: `<slot> is required`; `<slot> must be a number between <min> and <max>`; `<slot>: invalid
   duration "<value>"`; `<slot> must be one of <options>`; `<slot>: equipment not found: <id>`;
   `<slot>: <name> is not a <type>`; `<slot>: <name> is not in the zone` (scope as Sowel's
   constraints: the zone itself; the zone or below with `includeDescendants`; anywhere with
   `crossZone`); `<slot>: at most 16 equipments`. Then the script's own `validate(params)` (params
   parsed as in R1.4) may return an error text, served as `Invalid params: <text>`.
4. `start(api)` runs once per instance start. `api.params` holds the instance's settings, parsed (a
   duration in milliseconds, equipment slots as arrays of ids or one id). Handlers given to `on`,
   `onZone`, `onMotionReport` and the timers run later, one at a time. Per-instance data lives inside
   `start`'s closure: the script's top-level variables are shared by every instance of the recipe
   (one context per recipe, R3.2), and the living spec says so.

### R2: What a script may do (API version 1)

1. `on(slot, alias, fn)`: `fn(value, equipmentId)` when the data `alias` of an equipment of the slot
   is updated (Solite's `equipment.data.changed`, sent on each update, changed or not). The number of
   (equipment, alias) pairs an instance listens to is at most `MAX_LISTEN_EQUIPMENTS` (32); one more
   throws.
2. `onZone(alias, fn)`: `fn(value)` on **every** `zone.data.changed` of the instance's zone, with the
   aggregate's current `alias` (Sowel's draft: "when an aggregated data of the recipe zone changes");
   aliases `motion`, `luminosity`, `motionSensors` in this spec, another throws. All `onZone`
   handlers of an instance get the same event in one call, in the order they were added.
3. `onMotionReport(fn)`: `fn(value, equipmentId)` on every motion report of a motion sensor of the
   zone (or below), active or not (Solite's `device.data.updated` of a `motion` data). A Solite
   addition to Sowel's draft, so a safety auto-off can be reset by fresh motion; proposed to Sowel as
   an issue.
4. `get(slot, alias)`: the current value; for a list, an array in the slot's order. Read by the
   binding's alias, and, when the alias is `state` and the equipment has a data of category
   `light_state`, that data first (as Sowel's `isAnyLightOn`). `get("zone", alias)`: the aggregate
   (`motion`, `motionSensors`, `luminosity`).
5. `order(slot, alias, value)`: sends the value to every enabled equipment of the slot through its
   order binding `alias`; when the alias is `state`, a binding of category `light_toggle` or
   `toggle_power` first (as Sowel's `turnOnLights`). Returns an array of the equipments refused, each
   `{id, reason}` (`no order <alias>`, `queue full`); the recipe queue's bound (32) holds as for
   built-in recipes. Orders carry the recipe's id and name as their source.
6. **Only slot names**: `on`, `get` and `order` refuse anything but a slot of the recipe (`Unknown
   slot: <name>`). Ids are visible (in `params`, in handlers' arguments), but no call takes one. The
   engine's functions are captured by the shim and not reachable from the script's globals; the
   engine refuses an instance handle that is not one of this recipe's.
7. Timers: `after(ms or duration text, fn)` (at least 100 ms), `every(ms or duration text, fn)` (at
   least 1 s), `at("HH:MM", fn)` (daily, in the home's time zone; a time that does not exist on a
   daylight-saving day fires at the first minute after it, a time that occurs twice fires once), each
   returning an id for `cancel(id)`; all cleared when the instance stops; at most `MAX_TIMERS` (16)
   per instance; a shorter delay or one more timer throws. `after`/`every` run on the engine's
   monotonic clock, `at` on the wall clock and waits until the clock is set. The engine calls the
   script for its timers only when one is due.
8. `now()`: `{ time (ISO), isDaylight, sunrise, sunset }`. `Date.now()` and `new Date()` give the
   call's wall time (the engine's clock, not the operating system's).
9. `state.set(key, value)`, `state.get(key)`, `state.delete(key)`: served as the instance's `state` in
   `GET /recipe-instances`, keys in the order first set; at most 512 bytes serialized: `set` beyond
   it throws. `log(message)`, `log.warn`, `log.error`, and `console.log` as `log`: the instance's log
   (50 entries, 200 bytes each); at most 20 lines per call, then one warning `Log lines dropped`.
   `mode`, `energy`, `history`: throw `Not available in Solite: <name>`.
10. A throw that reaches the engine (from `start`, a handler, a timer) is an error of the instance:
    the log line `Script error: <message>` (level `error`; from `start`, `Failed to start: Script
    error: <message>` and the instance stays stopped, as a built-in recipe whose start fails); the
    instance keeps running otherwise. After each call the engine runs QuickJS's pending jobs (a
    promise's `then`), within the same limits.

### R3: The runtime (QuickJS) and its bounds

1. The core runs scripts through `rquickjs` (QuickJS-ng; its C sources compiled by the build, no
   OS), with its feature `parallel` (a runtime that can be moved between threads and resets its stack
   limit at each call), behind the core's cargo feature `script-recipes` (not a default feature).
   The firmware adds `rquickjs`'s `bindgen` and `allocator` features through its own dependency.
2. **One runtime and one context per script recipe**, shared by its instances, created at load from
   the source.
3. **Bounds** (constants beside the engine's, each with its worst case):
   - `MAX_SCRIPT_RECIPES` = 2 loaded at once;
   - `MAX_INSTANCES_PER_SCRIPT` = 8 instances of one script recipe (a ninth create answers 400
     `Too many instances of <recipe id> (max 8)`; on restore the ninth and later of a script recipe
     fail to restore with that text and stay stopped, they are not dropped);
   - `MAX_SCRIPT_BYTES` = 16 KB of source;
   - `SCRIPT_MEMORY_LIMIT` = 224 KB per runtime (set from the QEMU measure: `motion-light-script` loaded
     holds 107 KB, 8-10 KB per instance, 8 instances about 185 KB), QuickJS's own limit (`set_memory_limit`), which counts with the allocator's usable size:
     on the Mac the system's, on the box the firmware's PSRAM allocator (R3.4);
   - `SCRIPT_LOAD_LIMIT_MS` = 1000 ms to load a script (compile, top level, the `recipe()` call, its
     info): done once, and compiling is slow on the box; a script that runs past it is refused
     (`Script recipe invalid: the script took more than 1000 ms when loaded`);
   - `SCRIPT_CALL_LIMIT_MS` = 200 ms per call (the interrupt handler, reading a monotonic clock the
     engine is given; a fake one in tests). First 100 ms; the box's run measured 24 ms on average and
     83 ms at the most at 160 MHz, too close: raised with the user's agreement (2026-11-26);
   - `SCRIPT_TURN_LIMIT_MS` = 300 ms of script calls per turn of the engine's loop, checked before
     each delivery: none starts once 300 ms are spent, and the one under way finishes (one call, or
     one per equipment holding a reporting motion sensor, each of at most 200 ms); the script deliveries and timers left are carried
     to the next turn (the radio loop is never held longer than that by scripts, but for a restart's
     load, below); the built-in recipes always get each event at
     once, in the turn it came, whatever the scripts spent. A failed script's restart (R3.6) counts
     in the same budget: at most one script recipe is restarted per turn, and its load (up to
     `SCRIPT_LOAD_LIMIT_MS`, 1 s) and the starts of its instances are time of the turn: the starts stop
     when the 300 ms are spent and go on, in order, on the next turns, so a turn that restarts a
     script may last up to about 1 s (only then);
   - `SCRIPT_STACK_BYTES` = 16 KB of JavaScript stack;
   - QuickJS's small-object arena (4 KB pages per size class, about 85 KB per runtime) is off: the
     vendored `rquickjs-sys` carries the two-line patch of `research.md` and is built with
     `JS_ARENA_LARGE_BLOCKS_ONLY` (MIT licence kept; the root workspace and the firmware's both patch
     it);
   - the garbage collector: before each call its threshold is set to at most ¾ of the limit; after a
     call that leaves the runtime holding more than 85 % of the limit, or more than 32 KB more than
     the last collection left, it runs (what the runtime holds is read once per call: on the box the
     allocator's own count, on the Mac QuickJS's accounting, which walks the heap); a runtime that holds 90 % of the limit after it has failed in all but name;
   - `MAX_TIMERS`, the state's 512 bytes, 20 log lines per call, 32 listened pairs (R2).
   `MAX_INSTANCES` (16, all recipes together) and the engine's other bounds are unchanged and shared.
4. **The box**: the runtime's allocations go to PSRAM through the firmware's allocator
   (`heap_caps_*` with a correct usable size; its `unsafe` under D11). Every call into a script runs
   on a stack in internal RAM, measured: on the engine thread (its stack grows by what the measure
   asks), and every non-`GET` `/api/v1/*` request of the radio image but the account's (`/api/v1/auth/*`) runs its handler on a `script`
   job thread with an internal-RAM stack (D59's pattern at the HTTP server's edge), never on an HTTP
   worker's 10 KB PSRAM stack; its size is measured in QEMU and written in `docs/memory-budget.md`'s
   internal-RAM table and worst overlap.
5. **A failing recipe**: a call cut by the time limit; an allocation refused by the memory limit
   during the call (counted by the engine whether the script caught the error or not); a stack
   overflow; or a call to the engine with a forged handle: is a failure of **the recipe**. Its runtime
   is dropped; each running instance stops with `Script recipe failed: <reason>` and the events
   `recipe.instance.stopped` and `recipe.instance.error`; they stay enabled and are not written; a
   settle leaves them alone. The engine and the other recipes run on. The SDK's `Recipe` trait tells
   an instance error from a recipe failure (an error kind), and gives `next_deadline` (default: every
   turn, for built-in recipes).
6. **Coming back**: 1 minute after a failure the runtime is made again from the source (its load may
   take up to the 1 s load limit) and the enabled instances started, in order, within the turn's budget and then on the next turns (`Script recipe restarted (<n> of
   3 in 24 h)`), one script recipe per turn of the engine's loop, at most 3 times in a
   sliding 24 hours; then on a person's action (`POST .../enable` on an enabled stopped instance, a
   create or change of an instance of that recipe, a restart of the program), which starts all its
   enabled instances; a recipe that fails again at once answers 500 `Script recipe failed: <reason>`.
7. Events go to a script instance only when it listens to them.
8. A script whose id is a built-in recipe's (`motion-light`) or a loaded script's is refused.
   `GET /recipes` lists the built-in recipes first, then the scripts in load order.

### R4: Where scripts come from in this spec

1. **Mac**: `solite-host ... --serve [port] --recipe-script <file.js>`, repeatable up to
   `MAX_SCRIPT_RECIPES`, loads each script before the instances start and prints `recipes: script
   <id> <version> from <file> (<KB> KB of source, <KB> KB in its runtime)`. A missing, unreadable or
   refused file (`Script recipe invalid: <reason>`: too large, does not compile, no or two `recipe()`
   calls, `api` not 1, a bad id or slot, no `start`, a built-in's or loaded id), one too many, or the
   flag without `--serve`, stop the host with the reason and exit status 2.
2. **Box and QEMU**: the scripts of `recipes/scripts/*.js` are built into the image (as source) and
   loaded at start; one `recipes: script ...` line each. This spec ships `motion-light-script.js`. The
   radio image, and so every release, carries QuickJS and this script.

### R5: `motion-light-script`

1. A port of the built-in `motion-light` (Sowel's 1.1.2, `crates/recipes/motion-light`) to the script
   API, with the built-in's slot ids and defaults: `zone`, `lights` (list of `light_onoff`),
   `timeout` (`10m`), `luxThreshold`, `disableWhenDaylight`, `maxOnDuration`, `buttons` (list of
   `button`); its behaviour: on with motion, off after the timeout with no motion, the lux threshold
   with its 10 % hysteresis, the override when a light is switched by hand, the safety auto-off reset
   by fresh motion reports (`onMotionReport`), the buttons, "inactive during day", the lights set at
   start to what the zone calls for; its log texts; its state keys written in the built-in's order
   (`overrideMode`, `timerExpiresAt`, `failsafeExpiresAt`), rebuilt in that order at each change.
2. What differs, by the nature of script recipes (in its `description`, the living spec, and a test
   each):
   - validation texts are the host's (R1.3) and its own `validate`'s; it checks lights have a `state`
     order and buttons an `action` data in its `validate`;
   - a deleted equipment leaves its slots through the host's generic settle; a button that loses its
     `action` binding makes its `validate` fail (the instance is disabled), where the built-in prunes
     it in place;
   - the legacy `light` key is not read;
   - an order refused for a light logs `Error turning on some lights: <id>: <reason>` from `order`'s
     answer (the reason text is the host's).

### R6: The cost, measured

1. **Mac** (`script_memory.rs`, through QuickJS's own `memory_usage`, on its own line, apart from the
   engine's counting allocator and its ceilings): a loaded script recipe with 0, 1, 3 and 8
   instances; the peak of a motion event; the memory held after 10,000 events (grows by at most
   4 KB). In `lock_time.rs` (ignored, release): a motion event through the script against the
   built-in recipe; one event to 8 instances. `rquickjs-sys` is built at `opt-level = 3` in the dev
   profile.
2. **QEMU** (the image QEMU runs gets the scripts through a feature of its own, R3.1, never the
   measuring image): after the `engine:` line (its gate unchanged, D27) the image runs the realistic
   home's three `motion-light` instances' scenario again on three `motion-light-script` instances
   with the same settings (then, outside the line's window, again with 8 instances, the `script sizes:` line), then the deep-recursion fixture that calls `get` and `order` at its
   deepest, on the engine thread and on a `script` job thread; it logs `recipes: script
   motion-light-script <KB> KB source; psram +<KB> KB (peak +<KB> KB); internal +<KB> KB (min <KB> KB
   free); engine stack <n> of <n> unused; script thread stack <n> of <n> unused`. `scripts/qemu.sh`
   fails when the line is missing, the three instances' peak passes 75 % of `SCRIPT_MEMORY_LIMIT`,
   the internal RAM minimum is under 64 KB, or either stack keeps less than 2 KB unused.
3. **Box**: a `scripts:` line with each `heap:` line: `scripts: <id> <n> instances, <n> calls,
   average <µs> µs, max <µs> µs, heap <KB> KB (peak <KB> KB)`.
4. **Flash**: the radio image's application size with and without `scripts`, in the "Flash" section
   of `docs/memory-budget.md`; under 75 % of its 4 MB slot.
5. **Worst case**: `memory_worst_case.rs` loads `MAX_SCRIPT_RECIPES` scripts, each pushed to its
   memory limit, `MAX_INSTANCES_PER_SCRIPT` instances in each; their memory reported on its own line,
   under the scripts' ceiling, 512 KB (2 × 224 = 448 KB), a new row of `docs/memory-budget.md` counted in both states (worst-case margin
   842 − 512 = 330 KB). The engine's ceiling (D23) is unchanged.
6. The figures go into `docs/memory-budget.md` (rows M); D74 fixes the bounds from them.

### R7: The box check (the user present, one pass, after their flash)

`scripts/box-session.sh` gets a step (with `--with-gestures`): it reads the user's garage
`motion-light` instance, disables it, creates a `motion-light-script` instance with the same zone,
lights, buttons and lux settings and `timeout` `1m`; captures the serial log; asks the person to wave
at the motion sensor; checks within 10 s that the light went on (`GET /equipments/:id`), then off
within 90 s of its being seen on (the 1 minute timeout runs from the last motion, no later than that);
waits for the next `heap:` and `scripts:` lines (240 s: the serial log of `box.sh saves` is written when
the session ends, so the lines cannot be polled for to stop earlier);
then, also after a failure, deletes the script instance and enables `motion-light` again. It passes
when the light went on and off, both lines are read, and the internal RAM minimum is at least 64 KB.

## Living spec delta

### docs/living-spec/README.md, "Runs the motion-light recipe"

- Changed (written for the WebAssembly version on this branch, rewritten): built in; the host and the
  box also run recipes written as scripts (next section); `motion-light` unchanged.
- New (kept): settings nested deeper than 8 levels answer `Invalid params: params nested too
  deeply`.

### "Runs recipes from WebAssembly modules" → "Runs script recipes"

- Replaced: the format, the API and its slot-name rule, `onMotionReport`, what Solite lacks, the
  shared top-level variables, `--recipe-script`, the scripts built into the box, `motion-light-script`
  and its differences, the bounds, failures and restarts, the `recipes: script` and `scripts:` lines.

### "Starts and shows its name" (QEMU), "Serves Sowel's device API" (flags), "Stays within a memory budget", History

- Rewritten from the WebAssembly wording (the `module-check` feature disappears): the QEMU script
  phase, `--recipe-script`, the scripts' ceiling, the row of 041.

### No longer true

- `docs/decisions.md` D1, "Recipes run in `wasmi`": replaced by D73 (a new decision; D1 not edited).
- `docs/living-spec/README.md`: every WebAssembly sentence written on this branch.
- Head comments: `crates/core/src/recipes/mod.rs`, `crates/recipes/motion-light/src/lib.rs:4`
  ("built as a WebAssembly module"), `crates/core/src/recipes/native.rs:4`,
  `crates/recipe-sdk/src/host.rs:2`, `crates/recipe-sdk/src/abi.rs`.
- `docs/architecture.md` (Recipes, Recipe SDK, the new block "Script recipes", Builds: `rquickjs`, the
  vendored crate, the build variables, the features; Firmware: the `script` job thread, the engine
  stack, the `scripts:` line); `docs/memory-budget.md:131-132` ("No WebAssembly runtime");
  `CLAUDE.md` commands; `docs/roadmap.md` (F: F1 to F3).
- Others found by the search of Phase 3 (`wasm`, `WebAssembly`, `wasmi`, `module`, `build-recipes`).

## Acceptance criteria

- [x] `motion-light-script` gives the same orders and state as the built-in recipe in the shared
      behaviour scenarios (both ways); each difference of R5.2 has its test.
- [x] A script cannot read, listen to or command an equipment outside its slots, nor reach another
      instance (tested: unknown slot, raw id, forged handle, two instances in two zones).
- [x] `solite-host --sim scenarios/garage.json --serve --recipe-script recipes/scripts/motion-light-script.js`
      runs it; the garage's light switches through the script.
- [x] A script that loops, eats memory (caught or not) or recurses fails as R3.5 says; another recipe
      keeps running; it comes back after a minute, three times in 24 h at most.
- [x] Without a script loaded, `GET /api/v1/recipes`, `GET /recipes/:id` and the instances' JSON are
      byte for byte today's; the Sowel contract test is green; a script's `RecipeInfo` has Sowel's
      shape.
- [x] `sh scripts/qemu.sh` prints the `recipes: script` line and passes its gates; the measuring image
      and the box's own image have no QuickJS (checked by a symbol search); the figures of R6 are in
      `docs/memory-budget.md`.
- [ ] The box check of R7 is run by the user after their flash; its result is written in this spec.
      Met in two halves ("Result on the box"): the wave and the light in the first pass, the
      internal RAM over 64 KB in the second, without a person; a wave on the final image is left
      (roadmap inbox).
- [x] Decisions D73 to D77 written; `sh scripts/validate.sh` green.

## Edge cases

- The source is empty, over 16 KB, does not compile, calls `recipe()` twice or never, `api: 2`, a
  bad id, a built-in's id, an id already loaded, a slot of type `text`, no `start`: refused.
- `start` throws; a handler throws; a timer callback throws: R2.10.
- A handler loops; allocates without end (and catches the error); recurses without end: the recipe
  fails (R3.5); the firmware runs on.
- `on("nope", ...)`, `order(id, ...)`, a forged handle through any reachable function: refused.
- Two instances in two zones, one script recipe: neither sees the other's events or equipments.
- A 33rd listened pair; a 17th timer; `after(10)`; `every(500)`; a state over 512 bytes; 21 log
  lines; `mode()`: as R2 says.
- An equipment of a slot deleted while running: removed from the slots (settle), the instance
  restarted with the rest if it still validates.
- One event to 8 instances of a slow script: the turn's 300 ms bound carries the rest over.
- `at("02:30")` on a spring daylight-saving day, and on an autumn one; the time zone changed while an
  `at` waits; the clock not set yet.
- An API request creates a script instance on the box: runs on the `script` job thread.
- A promise's `then` scheduled by a handler: runs after the call, within its limits.

## Sowel contract

- The script format and API: Sowel's `roadmap/ai-script-recipes/spec.md` ("The idea", "Script API
  (first cut)", "Rules enforced by the host"), a draft with no code in Sowel yet. Solite implements
  the subset of R2, adds `id`, `version`, `onMotionReport`, `cancel`, `console.log` and `validate`,
  shares one runtime per recipe (Sowel's draft: one per instance), fails the recipe on an overrun
  (Sowel's: stops the instance with `state.alarm`), and makes host calls synchronous. Each is listed
  in `docs/sowel-subset.md` (a new section, the script API) and in a Sowel issue (inbox) proposing
  `id`, the zone slot rule and `onMotionReport` for the draft.
- The recipe's definition is Sowel's `RecipeInfo` and `RecipeSlotDef` (`src/shared/types.ts` T:680,
  T:824); instances, log and state are Sowel's `RecipeInstance` (with `state`,
  `src/recipes/engine/recipe-manager.ts:296`) and `RecipeLogEntry` (T:847); events
  `recipe.instance.*` (T:2028-2029). No route, field or event changes.

## How the user will check it

1. On the Mac, with the simulated garage: `cargo run -p solite-host -- --sim scenarios/garage.json
   --serve --speed 10 --recipe-script recipes/scripts/motion-light-script.js`; in the interface, a
   `motion-light-script` instance on the garage; the light goes on with motion and off after the
   timeout.
2. On the Mac, a test script that loops: its instance stops with `Script recipe failed: ...`, the
   home runs on, and a minute later `Script recipe restarted (1 of 3 in 24 h)`.
3. `sh scripts/qemu.sh`: the `recipes: script` line and the new rows of `docs/memory-budget.md`.
4. **On the box** (R7): the user types the box session with `--yes-flash --with-gestures` (D6, D55);
   result written here: date, devices, what was seen, the figures.

### Result on the box (2026-11-26)

Devices: the Dongle Max (ESP32-D0WDR2-V3, radio image), a SONOFF ZBMINIR2 relay (the garage's
light) and a SONOFF SNZB-03PR2 motion sensor, both paired that day, in the user's garage.

1. **First pass, the user's flash** (`box-session.sh --yes-flash --with-gestures`): 15 steps passed, 3
   failed, 9 skipped. B6 failed for want of a relay at that moment. B7.1 (a restore with a wrong
   password answers no status) and B7.6 (the busy test gets no answer) failed. Both come from the
   HTTP server, not from this spec, and went to the roadmap's inbox. Step B11, the script, was run
   by hand once the user had made the garage:
   - the light went on 8 s after the wave, and off 60 s after the sensor cleared, with the log lines
     `Motion detected — 1 light(s) turned on` and `No motion for 1min — lights turned off`;
   - `scripts:` gave 12 calls, 24.4 ms on average, 82.7 ms at the most;
   - `heap:` gave an internal RAM minimum of **47 KB**: R7's 64 KB was not met.
2. **Fixes**:
   - the recipe's info is kept as one JSON text, not twice as `Value` trees: loading took 30 KB of
     internal RAM, now 4 KB, in QEMU;
   - an instance keeps no copy of its listen set (`Update.listen` is `None` when unchanged): 1.5 KB
     less per instance;
   - the box's allocator gives the held bytes, so no walk of the heap after each call;
   - the limit per call is 200 ms (the user's agreement);
   - the CPU runs at 240 MHz, the flash and the PSRAM at 80 MHz (D77).
3. **Second pass, flashed by Claude with the user's leave** (`box.sh flash --image radio
   --keep-storage`, the home kept; the user away, so no wave):
   - the boot log reads `SPI Speed: 80MHz`, `esp_psram: Speed: 80MHz`, `SPI SRAM memory test OK`
     and `cpu freq: 240000000 Hz`;
   - with Wi-Fi up, 102 KB of internal RAM is free (75 KB before the fixes);
   - after 240 s with the garage's `motion-light-script` instance restored: **minimum 88 KB**;
   - then a login, the recipe list, the interface's page and four orders to the light through the
     API (eight `light_state` reports that call the script): **minimum 74 KB**, over the 64 KB of R7;
   - `scripts:` gave 15 calls, **13.9 ms on average and 36.1 ms at the most**, a held heap of 117 KB
     (this time includes the check after each call, the memory read and the collector, which the
     first pass's did not: the gain is larger than the figures show);
   - one flash write since boot.
