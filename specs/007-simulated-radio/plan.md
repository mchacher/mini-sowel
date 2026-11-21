# Plan: Spec 007

## Steps

- [x] 1. Living spec updated
- [x] 2. Core types: not applicable (the core does not change)
- [x] 3. Simulator: crate, NCP moved, devices, scenario, clock, transport
- [x] 4. Adapters: `SimTransport` in the host
- [x] 5. API: not applicable
- [x] 6. Interface: not applicable
- [x] 7. Host: `--sim`, `--speed`
- [x] 8. Tests; the tests of specs 004 to 006 on the crate, all passing
- [x] 9. Scenarios `snzb02.json`, `garage.json`
- [x] 10. Docs

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| scenario | The example of R4 | parsed: radio, one device, three events |
| scenario | Not JSON | `scenario: <file>: line <l>, column <c>: ...` |
| scenario | Unknown field / unknown kind / unknown device / duplicate id / duplicate EUI64 | `scenario: <file>: devices[i].<field>` or `events[i].<field>`: reason |
| scenario | `at` "1m30s" / "2x" / "25h" / "30s1m" | 90 s / error / error / error |
| scenario | `temperature` on a relay; `battery` 101; `ezsp` 12; `eui64` of 15 digits | error naming the field |
| scenario | 65 devices / 10 001 events / file of 1 MB + 1 byte | error naming the bound |
| scenario | Events out of order, two at the same time | sorted, ties in file order |
| clock | `at` 40 s at speed 10, with the fake clock | due at 4 s of clock time |
| devices | Relay receives On, Off, Toggle (reporting configured) | Default Response; `light_state` reported on, off, on |
| devices | Relay command with "disable default response" | no Default Response |
| devices | Motion device `motion: true`, not enrolled | nothing sent |
| devices | Motion device enrolled (CIE written, enroll response) then `motion: true` | zone notification alarm set, then cleared 60 s later |
| devices | Report from a device not joined, or for an attribute not configured | not sent |
| devices | Temperature-humidity device asked 15 s after its last report | no answer (asleep) |
| devices | Join before pairing opens | joins when it opens |
| devices | Join after the window closed, a second window opens | joins in the second |
| devices | Three devices join at once | three distinct network addresses, three interviews |
| memory | Network and devices written, second run | resumed; devices known; reports sent |
| memory | `"network": "formed"`, no file | `network: resumed (made by another program)` with PAN 0x5A5A |
| memory | File not parsable / not writable | error, no start |
| records | Records off by default; on and past 10 000 | none kept / capped |
| robustness | Malformed frames from the host (random bytes, truncated EZSP) | no panic, answered as an NCP would |
| existing | All tests of specs 004 to 006 | pass; `git diff` of `crates/*/tests/*.rs` changes only `mod`/`use` and device setup lines, no assertion; `cargo test --workspace` counts the same tests before and after, plus the new ones |
| host | `--sim scenarios/snzb02.json --pair 10 --speed 10 --data <tmp>` | `radio: sim scenarios/snzb02.json`, joined, device, reporting, measures |
| host | Then `--sim ... --listen --data <tmp>` | `network: resumed`, measures |
| host | `--sim` and `--radio`; `--speed` without `--sim`; `--speed 0`, `101`, `1.5` | usage, exit 2 |
| host | Missing scenario file | `scenario: <file>: ...`, exit 1 |
| host | `--sim` without `--data` | writes under `mini-sowel-data/sim/`, never `mini-sowel-data/network.json` |
| scenarios | Every file in `scenarios/` | parses |

## Validation

- [x] Validate command green
- [x] Agent review done, blocking findings fixed
- [ ] Demo seen and approved by the user
