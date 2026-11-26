# Plan: Spec 041

## Steps

- [x] 1. Contract and move (pass A, built before the change of runtime): SDK, `motion-light` crate,
      `NativeHost`, registry, state and listen caches, settle through `prune`, depth check, JSON
      pinned
- [x] 2. Remove the WebAssembly parts (pass B's step 5) and rewrite the living spec's WebAssembly
      section as script recipes
- [x] 3. `rquickjs` in the core behind `script-recipes`; the arena setting (flag or vendored patch);
      the `ScriptRuntimes` adapter and the limits; the vendored `rquickjs-sys`; builds on the Mac and for `xtensa-esp32-espidf` (the firmware's
      `[env]` variables)
- [x] 4. The shim, the loader, `ScriptRecipe`, the host's slot validation, the allow-list, timers,
      state, log; failures and restarts in the engine
- [x] 5. `motion-light-script.js`; parity tests; the bad scripts
- [x] 6. Host `--recipe-script`
- [x] 7. Firmware: `PsramAllocator`, scripts built in, API calls on an internal-RAM stack, engine
      stack, the `scripts:` line; QEMU phase and gates
- [x] 8. Measures (Mac, QEMU, flash, worst case); bounds fixed (D74)
- [x] 9. Box session step; docs (architecture, memory budget, decisions D73 to D76, sowel-subset,
      roadmap, CLAUDE.md); the "No longer true" search
- [x] 10. Interface: not applicable (the form is built from the slots, as for any recipe; checked
      once in the browser on the Mac)
- [x] 11. Box run: the user's flash (D6), then the result written in the spec (a second pass after the
      fixes, flashed with the user's leave; a wave on the final image left, roadmap inbox)

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| parity | Every behaviour scenario of `motion_light.rs` the script API can express (motion on, timeout off, lux threshold and hysteresis, override by hand both ways, failsafe, buttons, inactive during day, the start's setting) | Same orders and same state over time, built in and script |
| parity | Validation messages, settle of a deleted light, the legacy `light` key, order by category | The script's documented behaviour (R5.2), each a test |
| API | `GET /recipes` lists the script's `RecipeInfo` (Sowel's shape, slots in order, `zone` added) | As specified |
| API | Built-in recipe's routes and JSON (pinned bytes) | Unchanged |
| load | A valid script | Loaded; the line printed |
| load | Empty; 16 KB + 1; does not compile; no `recipe()`; two calls; bad id; built-in's id; an id already loaded; unknown slot type; no `start` | `Script recipe invalid: <reason>` |
| load | A fifth script; `--recipe-script` without `--serve` | Refused, exit status 2 |
| slots | Required missing; number out of range; bad duration; option not in `select`; unknown equipment; wrong type; equipment outside the zone; outside with `crossZone`; a list of 17 | Sowel's messages; the `crossZone` one accepted; 16 the most |
| slots | The script's own `validate` returns an error | `Invalid params: <its text>` after the host's checks pass |
| API calls | `on` of a slot's alias; `onZone("motion")`, `onZone("luminosity")`, `onZone("temperature")` | Handlers called with the value; the third throws |
| API calls | `get` of one equipment, of a list, of `zone` | Value, array, aggregate |
| API calls | `order` to a list where one lacks the alias; to a disabled equipment | The others get it, one warning; the disabled one gets nothing |
| sandbox | `on`/`get`/`order` with an unknown slot, a raw equipment id, another recipe's slot | `Unknown slot` |
| sandbox | `mode()`, `energy.claim()`, `history` | `Not available in Solite: <name>` |
| timers | `after("2s")`, `every("1s")` (and `every(500)` refused), `at("07:00")`, `cancel` | Fire on time on the monotonic clock (`at` on the home's zone); cancelled one does not; `every` under 1 s throws |
| timers | 17th timer; timers on stop | Throws; all cleared |
| timers | No deadline due | The engine does not call the script on tick |
| timers | `at` before the clock is set; time zone changed | Waits; follows the new zone |
| state | `state.set` in order; 512 bytes; 513 bytes | Served in order; accepted; `state.set` throws (catchable; uncaught: an instance error) |
| log | 21 lines in one call; a line of 300 bytes | 20 kept and one warning; cut at 200 |
| errors | `start` throws | `Failed to start: Script error: <message>` |
| errors | A handler throws | A log line, the instance runs on |
| limits | Infinite loop; allocation bomb; deep recursion | Recipe failed within 200 ms / at 224 KB / at the stack limit; instances stopped with the line and the two events; another recipe runs on; enabled kept, not written |
| restarts | 1 minute after a failure; fourth failure in 24 h; `enable` then; create while failed and fails again at once | Restarted with its line; stays failed; restarted by the person's action; 500 |
| restarts | A settle while failed | Its instances left alone |
| bounds | 8 instances of a script recipe; a 9th; a restore with 9 | Within 224 KB; 400 `Too many instances of <id> (max 8)`; the 9th fails to restore and stays stopped |
| bounds | A third script recipe; a script whose top level loops | `Script recipe invalid: too many script recipes (at most 2)`; refused within 1000 ms |
| bounds | Three instances' start sends 48 orders | Queue bound 32 holds, 16 dropped with the warning |
| bounds | Worst case: 2 scripts at their limit, 8 instances each | Under 512 KB; engine's ceiling unchanged |
| memory | Loaded with 0, 1, 3, 8 instances; peak of a motion event; 10,000 events | Printed; held memory grows by at most 4 KB after 10,000 events |
| time (ignored) | A motion event, script vs built in | Printed |
| stack | A call from another thread than the one that made the runtime (an API thread on the Mac) | Works; the stack limit is measured from that call |
| host | `--recipe-script` valid; missing; refused | Line; exit status 2 |
| sandbox | A forged handle passed to any function the script can reach; the engine's functions looked up on `globalThis` | Recipe failure; not found |
| sandbox | Two instances of one script in two zones | Neither sees the other's events, slots or state |
| API calls | `onMotionReport` with a person staying (repeated active reports, zone aggregate unchanged) | Handler called at each report; `motion-light-script`'s safety auto-off is reset, as built in |
| API calls | `onZone` on an aggregate change that is not motion or luminosity (temperature) | All `onZone` handlers called once, with current values |
| API calls | `get("lights","state")` on a light whose state is a `light_state` data under another alias; `order("lights","state",...)` with a `light_toggle` binding under another alias | Category first, as Sowel's helpers |
| API calls | A 33rd listened pair; `after(10)` | Throws |
| API calls | `console.log`; `Date.now()` | Logged as `log`; the call's wall time |
| API calls | A `then` scheduled in a handler | Runs after the call, within the limits |
| limits | An allocation bomb that catches the out-of-memory error | Recipe failure all the same |
| limits | Garbage above half the limit after a call | Collected; held memory back under half |
| limits | One event to 8 instances of a script that takes 50 ms per call | The turn stops after 300 ms; the rest carried to the next turn; all handled |
| timers | `at("02:30")` on spring and autumn daylight-saving days | Fires once, at the first minute after on the spring day |
| parity | State keys after an override set, cleared and set again | Built-in order every time |
| stack | Deep recursion calling `get` and `order` at its deepest, on the engine thread and on the `script` job thread (QEMU) | Stopped by the 16 KB limit; both threads keep ≥ 2 KB unused |
| firmware | Symbol search for `JS_NewRuntime` in the measuring and box images | Absent |
| box (script) | `box-session.sh`'s step, its cleanup after a failure | The `motion-light` instance enabled again, the script instance deleted |
| firmware (QEMU) | The script phase | The line; gates: present, 3 instances' peak ≤ 75 % of the 224 KB limit, internal RAM ≥ 64 KB, ≥ 2 KB of engine stack unused |
| firmware | `flash-saves` and the box's own image | No QuickJS in them |
| box (person) | The session's step on the garage | ZBMINIR2 on with motion, off after the timeout; `scripts:` and `heap:` lines read |

## Validation

- [x] Validate command green
- [x] Agent review done, blocking findings fixed
- [x] Demo seen and approved by the user (the box run: the user's flash; "garde le script et prépare la PR", 2026-11-26)
