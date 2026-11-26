# Research: Spec 041, how a recipe runs outside the firmware

Four measured spikes, 2026-11-26, done in copies of the repository (nothing of them is in the
code). They decided the runtime: QuickJS, not WebAssembly (decision D73).

## 1. WebAssembly with `wasmi` 2.0.3

`motion-light` written once in Rust and compiled to `wasm32-unknown-unknown` (JSON texts across
the boundary, `serde_json` on both sides). RAM counted on the Mac (64-bit) with a counting
allocator, through a realistic call sequence (validate, start, 5 events, 4 ticks, refresh, prune).

| Module | File | RAM (lazy translation: module + instance + calls) |
| ------ | ---- | ------------------------------------------------- |
| As built | 228.6 KB | 1107 KB (eager translation: 938 KB for the module alone) |
| `info()` constant, no `core::fmt`, immediate abort (nightly flags), own allocator, hand-written JSON, own float parser | 52 KB | 376 KB |

`wasmi` turns about 47 KB of code into about 155 KB of its own bytecode (24-byte instructions); one
module fits a 512 KB ceiling, not two. With a linear memory of 2 pages, `dlmalloc` cannot allocate at
all (it only grows the memory).

## 2. WebAssembly with WAMR 2.4.6 (classic interpreter)

Same modules, C harness, fixed pool. On the 32-bit target in QEMU: 34 KB of runtime per module,
plus its linear memory (64 or 128 KB) and its file (52 KB) kept resident: about 150 to 215 KB per
module. The fast interpreter translates and costs as much as `wasmi`. A C runtime of 90 to 125 KB of
flash, 3 to 5 days of integration, plus the 2 to 3 days of module shrinking above.

## 3. Other runtimes (published figures and Mac measures)

The same motion-light-like script (2 KB) on the Mac: QuickJS-ng about 85 KB per runtime and context
(before the arena was found, below); Berry 14 KB; Rhai 118 KB. Elk is too limited (no arrays, no
closures); wasm3 about 64 KB of code, 10 KB of RAM at least; ESP-IDF's ELF loader has no sandbox;
a declarative JSON format cannot hold motion-light's state machine (and Sowel's own spec rejects it).
QuickJS ranked first: Sowel's planned script recipes (`roadmap/ai-script-recipes/spec.md` in
Sowel) run in QuickJS, so one script can run in both.

## 4. QuickJS-ng 0.16.2 through `rquickjs` 0.14.0, in the firmware, in QEMU

Built inside the firmware with `rquickjs` (features `bindgen`, `allocator`) once three build
variables are set (`BINDGEN_EXTRA_CLANG_ARGS_xtensa_esp32_espidf` to ESP-IDF's libc headers,
`CC_xtensa_esp32_espidf` to ESP-IDF's `xtensa-esp32-elf-gcc`, `CFLAGS_xtensa_esp32_espidf="-mlongcalls
-ffunction-sections -fdata-sections"`); espup's generic `xtensa-esp-elf-gcc` makes big-endian objects
that do not link.

Free-heap deltas on the 32-bit target, KB (the script's bytecode 4040 bytes, 1880 stripped):

| Step | Stock (4 KB arena) | Arena 1 KB | No arena |
| ---- | ------------------ | ---------- | -------- |
| Runtime + context (base objects, JSON, eval, Promise) + 5 host functions | 135.7 | 61.7 | 49.3 |
| + the script loaded | 144.8 | 73.4 | 59.5 |
| + start, 5 events, 4 ticks | 144.8 | 73.5 | 60.5 |
| Three runtimes | 433 | 221 | 181 |
| One runtime, three contexts | 179 | 127 | 118 |

- quickjs-ng serves every allocation up to 512 bytes from 4 KB pages, one per size class, per
  runtime: about 85 KB more per runtime, not settable from outside (a two-line `#ifndef` patch makes
  `JS_ARENA_LARGE_BLOCKS_ONLY` settable).
- `JS_SetMemoryLimit` does nothing on ESP-IDF with the default allocator (no `malloc_usable_size`:
  every block counts 0); a counting allocator over `heap_caps_get_allocated_size`, with the limit
  inside it, works, and can place the heap in PSRAM.
- Placement decides: in the default heap every QuickJS block (all under 4 KB) lands in internal RAM,
  and three runtimes exhaust it; with the allocator forcing PSRAM, a recipe costs about 0.5 KB of
  internal RAM, and the QEMU gate (64 KB internal free) passes with three stock runtimes held.
- Limits checked in QEMU, the firmware running on: an infinite loop stopped by the interrupt
  handler after 153 ms (deadline 150 ms); an allocation bomb stopped at the 128 KB limit; a stack
  limit of 16 KB stops at JS depth 40 (about 420 bytes of C stack per JS call). The runtime is
  created and called on the same thread.
- Flash: +422 KB for the application (+28 %), in a 4 MB slot.
- Time on the Mac: 3.0 µs per call on average; on the ESP32 an estimate of 0.1 to 0.3 ms per event
  (not measured).
- Sowel's script API (`recipe({api, name, slots, start({on, onZone, get, order, after, every, at,
  now, state, log})})`) maps onto a small shim; gaps: modes, tariff, energy, history, observation
  mode, a per-instance order rate.
- GC: QuickJS's automatic threshold is 256 KB, above a 192 KB limit: it must be lowered or run
  after the calls.
