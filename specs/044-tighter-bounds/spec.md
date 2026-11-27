# Spec 044: Tighter bounds for the engine's worst case

## Context

The engine's worst case (`crates/core/tests/memory_worst_case.rs`: every memory bound at its
maximum at once) measures 1,042,032 bytes on the Mac against its 1 MiB ceiling (1,048,576 bytes,
the engine's share of the PSRAM in `docs/memory-budget.md`): 6.5 KB of room. On 2026-11-27 it went
over once (1,051,635 bytes) when the pre-commit check ran on a loaded Mac, and every coming feature
(I1, the modes, the history) adds to it. The user (2026-11-27): "il est trop gros le pire cas pour
Solite. Il faut se calmer [...] il faut qu'on borne mieux les limites".

Measured that day, phase by phase (Mac, 64-bit; the box's 32-bit figures are lower):

| Part | Bound today | Worst case |
| ---- | ----------- | ---------- |
| Devices | 64 devices, 16 data and 4 orders each | 284 KB |
| Recipe logs | 16 instances, 50 entries of 200 bytes each | 205 KB (86 KB measured, 200-byte entries added) |
| Equipments | 64 equipments, 16 data and 16 order bindings each | 183 KB |
| HTTP bodies and the radio's estimate | 4 bodies of 16 KB, the radio's queues | 115 KB |
| The largest answer and the save path (peak) | | about 100 KB |
| Recipe instances | 16 instances, params of 2 KB each | 66 KB |
| Zones, event queues, live clients, orders | | about 65 KB |

Solite's target is an annex or a small home (the true north). Three bounds are far above what a
real device or equipment uses: a device has 3 to 8 data (a temperature and humidity sensor 4, a
colour bulb 4, a plug with metering 5 once I1 reads it, an IAS multi-sensor 8), a light 1 to 4
bindings of each kind, and the last 20 lines of a recipe's log say what it did.

## Goals

1. The engine's worst case falls to about 715 KB on the Mac, about 30 % under the 1 MiB ceiling,
   by the bounds the user approved on 2026-11-27 (R1 to R3; R4 keeps the params).
2. A home stored with the old bounds still loads: what is over a new bound is dropped and said once
   in the log, never a failed start.

## Out of scope

- The 1 MiB ceiling itself, the number of devices (64), equipments (64), zones (32), recipe
  instances (16), log message length (200 bytes), and every other bound: unchanged.
- Why the test's peak once counted 9.6 KB more on a loaded Mac (the roadmap's inbox line of
  2026-11-27): this spec gives the room, it does not explain the reading.
- The script recipes' internal RAM (the inbox line "Before F2"): another change.

## Requirements

### R1: Data of one device: 16 → 8

1. `devices::MAX_DATA` is 8. A device that would get a 9th data does not get it, as today past 16:
   the line `<device>: more than 8 data, <key> not added` (`registry.rs:493`), **once per device and
   key**, for the last 16 pairs remembered, (today it repeats at every frame that carries it, such as an IAS notification).
2. Its orders stay at 4 (`MAX_ORDERS`, unchanged).
3. Data are added in the order they arrive (the clusters, then the IAS zone's type at enrolment,
   then the zone's status bits the first time they are set, `registry.rs:420-471, 694-699`). The
   richest device of today's catalog has exactly 8: an IAS motion multi-sensor with occupancy,
   temperature, humidity, illuminance, battery, voltage, then `tamper` and `battery_low`. Its margin
   is zero, written in D80. A test pairs it and checks its 8 data.

### R2: Bindings of one equipment: 16 + 16 → 8 + 8

1. `equipments::MAX_BINDINGS` is 8: 8 data bindings and 8 order bindings. One past it through the
   API is refused, 400 `Too many bindings (8)`, as today with 16.
2. An equipment created from several devices binds at most 8 values of each kind (for example 2
   colour bulbs, 4 dimmable bulbs, 8 relays); the others are skipped, logged as today
   (`home/equipment_ops.rs:225-246`). A single device never gives more (8 data, 4 orders).
3. The interface's own bound follows (`ui/src/components/equipments/bindingUtils.ts:129`,
   `MAX_BINDINGS` 16 → 8): its form plans at most 8 bindings of each kind, and its warning reads
   "more than 8 values, of which only 8 are bound" (`ui/src/i18n/messages.ts:167, 550`). Left at 16,
   the form would plan a 9th binding, the API would refuse it, and the creation would fail whole.

### R3: Log entries of one recipe instance: 50 → 20

1. `recipes::MAX_LOG_ENTRIES` is 20; the oldest goes first, as today. A message stays at most 200
   bytes (`MAX_LOG_MESSAGE_BYTES`, unchanged).
2. `GET /api/v1/recipe-instances/:id/log?limit=` gives at most 20 lines: a larger `limit` gives the
   20 kept, as today a `limit` over 50 gives 50 (`api/recipes.rs:24-32`). The interface keeps asking
   for 50, Sowel's default, and gets the 20 kept.
3. The logs are in RAM only: nothing stored changes.

### R4: Params of one recipe instance: unchanged at 2 KB (decided)

The user approved 2 KB → 1 KB before it was known that `motion-light`'s 16 lights and 16 buttons
take about 1.4 KB of ids, and that a script recipe's slots at their bounds take far more than 1 KB.
For a gain of about 16 KB, 1 KB would have cut `motion-light` to 8 lights and 8 buttons and the
script recipes' list slots to 8. **Decided by the user on 2026-11-27: params stay at 2 KB**
(`MAX_PARAMS_BYTES` unchanged); the worst case lands at about 715 KB in place of about 700. Also
decided that day, on the review's questions: an equipment made of several devices binds at most 8
values of each kind (R2.2), and 8 data a device stands, the IAS multi-sensor's zero margin accepted
(R1.3).

### R5: A home stored with the old bounds

1. Devices: a device read from `devices.json` with more than 8 data keeps its first 8, in the file's
   order; the load logs once per such device `devices: <name>: <n> data, <n − 8> not kept (at most
   8)`.
2. Equipments: the home's load first drops the bindings whose data or order is gone and those whose
   alias is taken (`Home::from_parts`, `home/mod.rs:207-223`), then keeps the first 8 bindings of
   each kind, in the file's order (today `Equipments::from_list` cuts first, `home/mod.rs:187`, so a
   valid binding past the cut could be lost while a dropped one took its place). It logs once per
   equipment cut `equipments: <name>: <n> data bindings, <n − 8> not kept (at most 8)`, the same
   for order bindings (today the cut at 16 is silent, `equipments/mod.rs:363-364`).
3. A full backup (spec 035) restored from a box with the old bounds goes through the same load at
   the next start: same result.
4. Writes: a load that drops or cuts a binding marks the equipments to be written, as today's
   repairs do (`home/mod.rs:224`): `equipments.json` is written once after the boot. `devices.json`
   is written back cut at its next save; no other write is forced (D39's flash budget).

### R6: The worst case, the realistic home and the API measure

1. `memory_worst_case.rs` reaches every new bound (8 data, 8 + 8 bindings, 20 log entries of 200
   bytes) and keeps its 1 MiB ceiling; `recipe_params_memory.rs` is unchanged.
2. `scripts/measure-api.sh` sends the creation of an equipment at the new bound: **17 requests**
   (one creation, 8 data and 8 order bindings), all 201 within 10 s; the box session's step B6
   (`scripts/box-session.sh:1574-1587`) expects 17.
3. The figures are written in `docs/memory-budget.md`: the worst case and its phase table measured
   again, the realistic home (`memory_ceiling.rs`) and QEMU's `engine:` line.
4. Target: about 715 KB measured on the Mac (estimated gains: about 115 KB for the data, 120 KB for
   the logs, 70 KB for the bindings). A figure over 800 KB is reported to the user before the merge.

## Living spec delta

### docs/living-spec/README.md

- The devices (343): "At most 64 devices, 8 data and 4 orders each"; a stored device over the bound
  keeps its first 8 data, logged.
- The equipments (542): "At most 64 equipments, 8 bindings of each kind"; a stored equipment over
  the bound keeps its first 8 valid ones, logged, written once.
- The proposals (567): "at most 8 of each kind".
- The interface's creation form (985) and the equipment page's Save (1001): 8 values, 8 of each kind.
- The recipe log (767): "the last 20 lines of each instance"; `?limit=` over 20 gives 20.
- `measure-api.sh` (1124-1128): 17 requests, 8 and 8 bindings.
- The worst case (1767-1768): "64 devices of 8 data… 64 equipments of 8 bindings".
- History: spec 044's row.

### Elsewhere

- `CLAUDE.md:59` and `README.md:56`: `measure-api.sh`'s "17 requests of an equipment with 8 + 8
  bindings".
- `docs/memory-budget.md` (R6.3; its line 192 on the realistic home if it changes).
- `docs/sowel-subset.md`: D80 added to the list of decisions that hold differences of value
  (lines 171-173).
- `docs/decisions.md`: D80, the bounds and why (room for the next features; a real device and
  equipment use far less; the IAS multi-sensor's zero margin); a home stored with the old bounds is
  cut at load, logged. D-records already written (for example `decisions.md:480`, "1 + 16 + 16")
  are history and stay as written.

### No longer true

- "16 data", "16 bindings", "16 values", "50 lines", "33 requests" wherever written (above), the
  constants' comments, `memory_worst_case.rs`'s header and its literal `50` (now `MAX_LOG_ENTRIES`),
  `recipes/mod.rs:50-54`'s arithmetic, the interface's comments (`bindingUtils.ts:8-12, 286, 370,
  382, 415`).
- `inputs/` is left as it is (the framing documents are history).

## Acceptance criteria

- [ ] `MAX_DATA` 8, `MAX_BINDINGS` 8 (core and interface), `MAX_LOG_ENTRIES` 20; params unchanged
      at 2 KB.
- [ ] The tests of the plan pass; the old bounds' tests moved to the new ones (`DeviceSelector.test.tsx:166-186`,
      `bindingUtils.test.ts:132-135, 260` included).
- [ ] `memory_worst_case` under 1 MiB at about 715 KB (over 800 KB: told to the user first); the
      figures written in `docs/memory-budget.md`; `measure-api.sh` at 17 requests.
- [ ] Living spec, `CLAUDE.md`, `README.md`, `docs/sowel-subset.md` and D80 written;
      `sh scripts/validate.sh` green.
- [ ] Box (with the user; their flash, `--keep-storage`): the home loads unchanged (2 devices, 1
      zone, 2 equipments, 1 recipe instance), the recipe runs, a log shows at most 20 lines.

## Edge cases

| Case | Behaviour |
| ---- | --------- |
| A device given a 9th data | Not added, `more than 8 data, <key> not added`, once per key |
| The IAS multi-sensor (8 data) | All 8 there; nothing dropped |
| `devices.json` with a device of 12 data | First 8 kept, one log line; bindings to the 4 others dropped at load |
| `equipments.json` with 10 data bindings, one of them to a data gone | The dangling one dropped first, then 8 of the 9 kept, one log line, one write |
| The 9th binding through the API | 400 `Too many bindings (8)` |
| An equipment created from 3 colour bulbs (12 data, 12 orders) | 8 and 8 bound, the others skipped and logged; the interface warns before |
| `?limit=50` on a log | The 20 lines kept |
| A backup from a box with the old bounds | Same as the stored files, at the next start |

## Sowel contract

No field, route or event changes. Sowel sets none of these bounds (`src/shared/types.ts`;
`src/recipes/engine/recipe-manager.ts:555`: `getLog(instanceId, limit = 50)`): they are Solite's
own, in D80. An equipment made in Sowel with more than 8 bindings of a kind cannot be held by the
box as is; the link with Sowel (milestone G) will have to say so.

## How the user will check it

On the Mac: the worst-case figure before and after (`cargo test -p solite-core --test
memory_worst_case -- --nocapture`). On the box (with the user, their flash with `--keep-storage`):
the garage is there as before, the motion light works, a recipe's log shows at most 20 lines.
