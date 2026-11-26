# Architecture: Spec 041

## Overview

```
 recipes/scripts/motion-light-script.js         (Sowel's script format, JavaScript source)
        │ --recipe-script <file> (host)  |  include_str! of recipes/scripts/*.js (QEMU, radio image)
        ▼
 crates/core/src/recipes/script/   (feature script-recipes)
   load.rs    compile the source in a fresh runtime, run recipe({...}) once, read id/slots → RecipeInfo
   shim.js    the JS side of the API, built in (on, onZone, get, order, after/every/at/cancel, now,
              state, log, params); keeps each instance's handlers, timers and state apart
   runtime.rs ScriptRecipe: implements the SDK's Recipe trait (start, on_event, tick, refresh,
              prune, validate) by calling the shim; host functions → NativeHost, slot allow-list
   limits.rs  memory limit (QuickJS's own, runtimes from the ScriptRuntimes adapter), interrupt, stack, GC
        │ provider_of(): built-ins, then scripts
        ▼
 crates/core/src/recipes/mod.rs  RecipeEngine (unchanged contract: Runner, cache, settle, failures)
        │
 host: default allocator (QuickJS counts with malloc_size)   firmware: PsramAllocator (heap_caps,
                                                              counting, limit) + script calls on an
                                                              internal-RAM stack (engine thread,
                                                              a `script` job thread for API calls)
```

`ScriptRecipe` is one more implementation of the SDK's `Recipe` trait, beside the linked
`motion-light`: the engine's registry (`provider_of`), state cache, listen cache, settle and
failure paths built in pass A serve it without change, except the recipe-level failure and restart
(R3.5, R3.6), which are new.

## The shim (JavaScript, built into the core)

A small JS file (`crates/core/src/recipes/script/shim.js`, `include_str!`) evaluated in the context
before the recipe's source. It defines `recipe()` (records the definition once), and for each
instance an API object whose functions close over the instance's handle:

- `on`, `onZone`: push handlers into the instance's table and return; the listen set is the union of
  the instances' tables, read by Rust after each call (R2.5 of the contract);
- `get`, `order`: check the slot name against the instance's resolved slots (ids from the validated
  params, held in Rust: the allow-list), then call the host functions `__get(handle, slot, alias)`,
  `__order(handle, slot, alias, value)` registered by Rust;
- timers: a per-instance list of `{id, due, period, fn}`; Rust gives `now` at each call and asks
  `__next_deadline()` after it, so the engine calls `tick` only when one is due;
- `state`: a per-instance ordered object, serialized after each call (512-byte check in Rust);
- `log`: lines buffered per call, at most 20, handed to Rust after the call.

No call takes an id: `__get` and `__order` take a slot name, and Rust maps it to the ids of that
instance. The shim captures the engine's functions in closures and deletes them from `globalThis`;
Rust refuses a handle that is not one of this recipe's instances.

## Memory and stack on the box

- **Memory limit without `unsafe` in the core** (D11 kept): the core calls `Runtime::set_memory_limit`
  and reads `memory_usage()`, both safe; QuickJS counts with the allocator's usable size. The core
  asks a `ScriptRuntimes` adapter for each new runtime (`fn new_runtime(&self) ->
  Result<rquickjs::Runtime, String>`): the host returns `Runtime::new()` (the system allocator, whose
  `malloc_size` QuickJS reads), the firmware `Runtime::new_with_alloc(PsramAllocator)`, an allocator
  over `heap_caps_malloc(MALLOC_CAP_SPIRAM)` with `heap_caps_get_allocated_size` as its usable size
  (its `unsafe` under D11, each with a `SAFETY` comment). An allocation refused during a call sets a
  flag the engine reads after it (R3.5), through the adapter (`refused_since_last_call`).
- **Threads**: `rquickjs`'s `parallel` feature makes the runtime `Send` (the engine's `Box<dyn Recipe
  + Send>`) and resets QuickJS's stack top at each `Context::with`, so the stack limit holds on
  whichever thread calls. The home lock already serialises every call.
- **The arena**: `JS_ARENA_LARGE_BLOCKS_ONLY` is an unconditional `#define` in `quickjs.c`, so
  `rquickjs-sys` is vendored under `vendor/rquickjs-sys` with the two-line `#ifndef` patch of
  `research.md` and built with the define set; `[patch.crates-io]` in the root `Cargo.toml` and in
  `firmware/Cargo.toml` (its own workspace).
- **Stacks on the box**: the engine thread's stack grows by the measured need. The radio image's HTTP
  server runs the handler of every non-`GET` `/api/v1/*` request but the account's (`/api/v1/auth/*`, which never reach a script and stay on the worker) on a `script` job thread whose stack
  is in internal RAM (`job_thread.rs`, D59 and D60's pattern), the worker waiting for it; `GET`s never
  reach a script. The core needs no adapter for it (`&Home` is not `Send`: the request, not the home,
  goes to the thread, which takes the lock itself).
- **Time**: the interrupt handler reads a monotonic clock closure the engine is given (the host's and
  the firmware's real one; a fake in tests). `Date.now` and `new Date()` are pinned by the shim to the
  call's wall time.

## Data

| Name | Type | Where |
| ---- | ---- | ----- |
| Script source | text, at most 16 KB | host: the file read at start; box: `include_str!` of `recipes/scripts/*.js`; never written to `/data` |
| Loaded script recipe | id, version, `RecipeInfo` JSON, the runtime and context, state `Ready` or `Failed { since, restarts_today }` | RAM (PSRAM on the box) |
| Instance slots | the ids of each equipment slot, resolved from the validated params | the engine, per instance |
| `scripts:` line | counters per script recipe (calls, average and max µs, heap, peak) | radio image's log |

No file of the data folder changes: `recipes.json` keeps `recipeId`.

## Files changed

| File | Change |
| ---- | ------ |
| `Cargo.toml` (root) | remove the member `crates/recipes/motion-light-wasm` and `[profile.recipe]`; `rquickjs` version pinned in the workspace dependencies; `[patch.crates-io]` if vendored |
| `rust-toolchain.toml` | remove the `wasm32-unknown-unknown` target |
| `crates/recipe-sdk/` | remove `guest.rs`, the `guest` feature and the export macro; `abi.rs` keeps only what the contract still uses (bounds), renamed if it reads as WebAssembly |
| `crates/recipes/motion-light-wasm/`, `scripts/build-recipes.sh` | removed |
| `crates/core/Cargo.toml` | `rquickjs` (optional, feature `script-recipes`, features `parallel`), not a default feature; the tests of scripts with `required-features` |
| `vendor/rquickjs-sys/` (new) | the crate with the arena patch, licence kept |
| `crates/core/src/recipes/script/` (new) | `mod.rs`, `load.rs`, `runtime.rs`, `limits.rs`, `shim.js`, `slots.rs` (the host's generic validation, R1.3) |
| `crates/recipe-sdk/src/recipe.rs` | an error kind (instance error or recipe failure) and `next_deadline` (default: every turn) |
| `crates/core/src/recipes/mod.rs` | provider of scripts; recipe-level failure and restarts; `next_deadline`; the turn's 300 ms bound; refuses a script with a built-in's or loaded id |
| `crates/core/src/recipes/native.rs` | the host functions a script reaches, by slot |
| `recipes/scripts/motion-light-script.js` (new) | the port, R5 |
| `crates/core/tests/script_recipes.rs` (new) | load, API, slots, limits, failures, restarts (test plan) |
| `crates/core/tests/script_parity.rs` (new) | the shared behaviour scenarios, both ways |
| `crates/core/tests/script_memory.rs` (new) | R6.1 |
| `crates/core/tests/lock_time.rs`, `memory_worst_case.rs` | R6.1, R6.5 |
| `crates/core/tests/fixtures/scripts/*.js` (new) | the bad scripts (loop, allocation bomb, deep recursion, throws, unknown slot, raw id, 17 timers, big state, 21 lines, `mode()`...) |
| `crates/host/src/main.rs`, `serve.rs` | `--recipe-script`, the lines of R4.1 |
| `firmware/Cargo.toml` | feature `scripts = ["solite-core/script-recipes", "dep:rquickjs"]` (rquickjs with `bindgen`, `allocator`, `parallel`); `radio = [..., "scripts"]`; `sim` and `flash-saves` without it; `[patch.crates-io]` for the vendored crate |
| `scripts/qemu.sh`, `scripts/validate.sh`, `scripts/release.sh`, `scripts/box.sh` | QEMU builds `--features sim,scripts`; a symbol search (`JS_NewRuntime`) proves the measuring and box images have no QuickJS; the release and box builds take the radio image's features as they are |
| `firmware/.cargo/config.toml` | `[env]`: `CC_xtensa_esp32_espidf`, `CFLAGS_xtensa_esp32_espidf`, `BINDGEN_EXTRA_CLANG_ARGS_xtensa_esp32_espidf` (relative to `.embuild`, no path of one computer) |
| `firmware/src/script_alloc.rs` (new) | `PsramAllocator` |
| `firmware/src/radio_engine.rs`, `simulated.rs`, `measure.rs`, `http_server.rs` | built-in scripts loaded; engine stack; the `scripts:` line; the QEMU phase with the deep-recursion fixture on both threads; non-`GET` API handlers on the `script` job thread |
| `scripts/qemu.sh`, `scripts/lib/firmware.sh` | the `recipes: script` line and its gates |
| `scripts/box-session.sh` | a step: create a `motion-light-script` instance on the garage, the person's wave, read the `scripts:` line |
| `docs/` | living spec, architecture, memory budget, decisions D73 to D76, sowel-subset (script API deviations), roadmap (F1 to F3; box check), CLAUDE.md commands |

## Reused

- Pass A's whole contract: `Recipe`, `Host`, `NativeHost`, `provider_of`, the state and listen
  caches, settle, the depth check, the pinned JSON.
- The engine's stop path for a failed recipe (log line, `recipe.instance.stopped`,
  `recipe.instance.error`).
- `job_thread.rs` (a thread with an internal-RAM stack) for API calls into scripts on the box.
- `lock_time.rs`, the counting allocator of the memory tests, `measure.rs`, `qemu.sh`'s gates,
  `box-session.sh`'s steps.

## Decisions

- **D73**: recipes outside the firmware are JavaScript scripts in Sowel's script-recipe format, run
  by QuickJS through `rquickjs` (replaces D1's `wasmi`; the measures in `research.md`).
- **D74**: one runtime per script recipe, its bounds (224 KB through QuickJS's own limit, 2 recipes, 8
  instances each, 1000 ms to load, 100 ms per call, 300 ms per turn, 16 KB of stack, the arena off by a vendored patch), PSRAM on the box through
  the firmware's allocator (D11 kept), calls on internal-RAM stacks (the engine thread, a `script`
  job thread for API requests); a recipe fails as a whole and restarts after 1 minute, 3 times in
  24 h.
- **D75**: a script reaches only its slots (the allow-list in the host); what Solite adds to and
  leaves out of Sowel's draft API.
- **D76**: where scripts come from in this spec: `--recipe-script` on the Mac, built into the radio
  image (so into releases) on the box; download and signing are F2, with a key of their own.
