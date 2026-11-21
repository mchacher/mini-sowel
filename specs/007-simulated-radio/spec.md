# Spec 007: simulated-radio

## Context

Specs 004 to 006 talk to the Dongle Max's radio; their tests use a fake radio written inside the
test folder (`crates/core/tests/common/mod.rs`): a scripted EmberZNet NCP speaking ASH and EZSP,
with one fake Sonoff SNZB-02. Increment B1 of the roadmap turns it into a **simulated radio** the
host can run: the user, and every later increment of milestone B, can then work without the
dongle, with scenarios written as files: devices that join, measures over time, a relay that
obeys.

Because the simulation speaks the real protocols, it exercises the same code as the real radio
(ASH, EZSP, the coordinator): only the transport changes.

Specified in autonomy (the user approved, 2026-11-21, building milestone B as far as possible
without hardware); choices listed under "Decided in autonomy".

## Goals

1. `--sim <scenario>` replaces `--radio tcp://...` in every host command (identify, `--pair`,
   `--listen`).
2. A scenario file describes the radio, the devices and what happens when, in plain JSON.
3. The tests of specs 004 to 006 keep passing and keep their meaning, on the shared simulator.

## Out of scope

- IAS zone enrollment (B2): until then, the simulated motion sensor sends no motion, like the real
  one.
- Sending On/Off commands from the host (B4): the relay's answers are tested inside the crate.
- Recording a real session into a scenario.
- A `leave` event (nothing in B2 to B6 needs it).

## Requirements

### R1: The simulator crate

`crates/sim`, a member of the Mac workspace, never built for the ESP32. It holds the simulated
NCP moved from `crates/core/tests/common/mod.rs` (moved, not duplicated), generalised from one
device to a list of devices (at most 64, unique network addresses assigned on join), with the
fault switches the tests use. It implements the core's `RadioTransport` (`SimTransport`).

- **Errors**: `crates/sim` follows the project's rule: no `unwrap`, `expect` or `panic!` outside
  its tests; a malformed frame from the host is answered as the real NCP would (ignored or an
  ASH error), never a crash.
- **Records**: the records the tests inspect (bytes received, commands, messages sent) are off by
  default and switched on by the tests; when on, each is capped at 10 000 entries (bound written
  next to it).
- **Time**: the simulator takes its clock from its caller: the host gives real time
  (`Instant`), the tests their fake clock (the one that advances with each read's timeout, as
  today). `SimTransport::read` waits for the shorter of the read's timeout and the time to the next
  due event, without busy looping.
- The core's own unit tests (`#[cfg(test)]` modules) do not use the crate; its integration tests
  (`crates/core/tests/`) do, as a dev-dependency. The host's tests of specs 004 to 006 keep the TCP
  transport (they test it), with the simulator behind the TCP listener.

### R2: The simulated radio's memory

The simulated radio keeps its flash in the host's data folder, `<data>/sim-<scenario file
name>.json`: the network (PAN id, extended PAN id, channel, keys) and every joined device (EUI64,
network address, bindings and reporting set up, relay state, enrolled or not). Under `--sim` the
default data folder is `mini-sowel-data/sim/`, so the real dongle's backup in `mini-sowel-data/`
is never touched. The file is read at start and written when it changes (temporary file, then
rename). A file that does not parse is an error naming it; the host does not start.

The scenario's `"network"` applies only when there is no such file: `"none"` (no network) or
`"formed"` (a coordinator network with fixed identifiers: PAN 0x5A5A, extended PAN
0x5A5A5A5A5A5A5A5A, channel 15, fixed keys; the host then prints `network: resumed (made by
another program), ...`, having no backup of it).

### R3: Simulated devices

Each kind answers the interview, binding and reporting setup of specs 005 and 006 as the real
device does; a device can override `manufacturer` and `model`:

| Kind | Default identity | Endpoint 1 in clusters | Behaviour |
| ---- | ---------------- | ---------------------- | --------- |
| `temperature-humidity` | eWeLink TH01 (Sonoff SNZB-02) | 0x0000 0x0001 0x0003 0x0402 0x0405 | sleepy: answers only for 10 s after joining or after one of its reports |
| `motion` | eWeLink MS01 (Sonoff SNZB-03) | 0x0000 0x0001 0x0003 0x0500 | sleepy; sends IAS zone notifications only once enrolled (CIE address written, Zone Enroll Response received: B2); a `motion: true` report is followed by `motion: false` 60 s later, as the real sensor clears itself |
| `relay` | SONOFF ZBMINIR2 | 0x0000 0x0003 0x0004 0x0005 0x0006 | mains powered, always answers; On (0x01), Off (0x00), Toggle (0x02) change its state, answered by a Default Response (success) unless disabled, then a Report Attributes of 0x0006/0x0000 (boolean) if its reporting is configured |

A report is sent only by a device in the network, and only for attributes whose reporting was
configured (as a real device after spec 006's setup); `battery` is sent by battery devices only.

### R4: Scenario files

JSON, in `scenarios/` at the repository's root, at most 1 MB (checked before reading); unknown
fields are errors:

```json
{
  "radio": { "ezsp": 13, "stack": "7.4.4.0", "eui64": "8C6FB9FFFE123456", "network": "none" },
  "devices": [
    { "id": "th", "kind": "temperature-humidity", "eui64": "00124B0012345678" }
  ],
  "events": [
    { "at": "2s", "device": "th", "do": "join" },
    { "at": "40s", "device": "th", "do": "report", "temperature": 19.52, "humidity": 54.1 },
    { "at": "1m30s", "device": "th", "do": "report", "battery": 100 }
  ]
}
```

- `ezsp`: 13 to 19. `eui64`: 16 hexadecimal digits, most significant first. `id` and `eui64`
  unique.
- `at`: time since the host connected, in scenario time: whole numbers with `h`, `m`, `s`, in
  that order, combinable (`1m30s`), at most 24 h. Events may be written in any order: they run
  sorted by time, ties in file order.
- `do`: `join` (the device joins as soon as pairing is open at or after `at`, like a button
  pressed once `pairing: open` shows; dropped if pairing never opens before the run ends; a join
  whose time falls after a pairing window closed waits for the next one) or `report` (measures by
  Sowel category, each allowed only for its kind: `temperature` and `humidity` numbers and
  `battery` 0..100 for temperature-humidity; `motion` boolean and `battery` for motion;
  `light_state` `"on"` or `"off"` for relay).
- After the last event the simulated radio stays up, answering, until the host stops.
- At most 64 devices and 10 000 events (bounds written next to them).
- Errors, one line, the host exits 1: `scenario: <file>: line <l>, column <c>: <reason>` for JSON
  syntax; `scenario: <file>: devices[<i>].<field>: <reason>` and `scenario: <file>:
  events[<i>].<field>: <reason>` for the rest (unknown device, unknown kind, bad `at`, field not
  allowed for the kind, duplicate id or EUI64, out-of-range value, too many devices or events,
  file over 1 MB).

`--speed <factor>` (a whole number, 1 to 100, default 1) runs the scenario's clock faster: it
scales the `at` times only, not the host's own timers (the pairing time, the closing grace) nor
the simulated devices' answer delays.

### R5: Shipped scenarios

- `scenarios/snzb02.json`: one SNZB-02 joins at 2 s, then reports temperature and humidity every
  30 s for 10 minutes, battery once.
- `scenarios/garage.json`: a SNZB-02, a motion sensor and a relay join; temperatures every minute;
  motion at 1 min and 3 min (sent only once B2 enrolls the sensor). Used by B2 to B6.

### R6: The host

`--sim <scenario>` in place of `--radio`: the same commands and lines, `radio: sim <scenario>`
replacing `radio: tcp://...`. Usage errors (exit 2): `--sim` and `--radio` together, `--speed`
without `--sim`, `--speed` outside 1..100 or not a whole number. The usage text names both
`--radio` and `--sim`. A missing scenario file: `scenario: <file>: <reason>`, exit 1.

## Living spec delta

### docs/living-spec/README.md

- New section "Runs without the dongle": `--sim`, `--speed`, the scenario format and its errors,
  the three simulated devices (identities, clusters, what they send: the motion sensor nothing
  before enrollment, the relay `light_state on/off`), the simulated radio's memory in
  `mini-sowel-data/sim/`, the shipped scenarios.
- Changed: "without `--radio`, prints `mini-sowel 0.1.0`" → "without `--radio` or `--sim`".
- Changed: "a malformed `--radio` prints the expected form" → "a malformed `--radio` or `--sim`
  prints the expected form".
- Changed: the data folder sentence → under `--sim`, `mini-sowel-data/sim/`.
- Changed: the measures paragraph → names `alarm` and `tamper` (IAS zone notifications, sent by
  enrolled alarm devices).
- History row 007.

### No longer true

- `docs/architecture.md`: blocks list → the simulator crate.
- `README.md`, `CLAUDE.md`: commands → `--sim` added.
- `docs/roadmap.md`: milestone A's increments A3 to A5 → "built, demo pending" (specs 004 to 006).
- Searched: `fake radio`, `tests/common`, `--radio`, `mini-sowel-data`, `B1`.

## Acceptance criteria

- [ ] `--sim scenarios/snzb02.json --pair 10 --speed 10` shows the SNZB-02 joining, its
      reporting set up and its measures; `--listen` on the same scenario then resumes the network
      and shows measures.
- [ ] The tests of specs 004 to 006 pass on the crate with the same assertions (see plan).

## Edge cases

- Scenario missing, over 1 MB, not JSON, unknown field, unknown kind, duplicate id, bad `at`,
  field not allowed for the kind, value out of range: the errors of R4.
- A join before pairing opens: waits; pairing never opens: dropped.
- A report from a device not in the network, or for an attribute not configured: not sent.
- The simulated radio's file not parsable: error, no start; not writable: error before running.
- Events written out of order: sorted.

## Sowel contract

None (the simulator is internal); report keys are Sowel's `DataCategory` names.

## Decided in autonomy

- The simulated motion sensor needs enrollment like the real one (spec review question 1): motion
  shows from B2.
- The motion sensor is assumed to be a Sonoff SNZB-03 (IAS zone); its strings are to be confirmed
  at a demo (spec review question 2, asked to the user).
- `serde` and `serde_json` enter the workspace with this spec (CLAUDE.md names them for JSON).
- Default identities follow the user's devices; `eWeLink MS01` and `SONOFF ZBMINIR2` are
  assumptions until the demos.
- `leave` dropped from the scenario format.
- Added while building: the crate's names are `Ncp`, `SimDevice`, `SimTransport`, with a
  `Clock` trait (`RealClock`, `FakeClock`); the memory file is named after the scenario's stem
  (`sim-snzb02.json`) and written at start, so an unwritable folder fails before anything runs;
  more scenario errors (a report without measure, measures on a `join`, `radio.*` fields, the
  first index past a bound); device timers (10 s awake, 60 s motion clearing) run in radio time,
  not scaled by `--speed`; simulated network addresses start at 0x2000; records keep the first
  10 000 entries; the motion sensor does not send a Zone Enroll Request on its own, it answers
  the CIE address write and the Enroll Response.

## How the user will check it

Run `cargo run -p mini-sowel-host -- --sim scenarios/garage.json --pair 30 --speed 10` and watch
three devices join, the sensor's measures arrive, the motion sensor stay silent (not enrolled
before B2).
