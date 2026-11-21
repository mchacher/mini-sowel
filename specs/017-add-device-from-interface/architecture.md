# Architecture: Spec 017

## Overview

```
browser #/devices ──POST/GET /api/v1/integrations/solite/permit-join──► core api/pairing.rs
                                                                          │ (home lock)
                                                                          ▼
                                                     Home.pairing: PairingWindow (request, state)
                                                                          │ taken each turn
core engine::run (radio loop, one thread) ◄───────────────────────────────┘
   permit_join / close_joining (outside the home lock)
   radio.poll_pairing() ──► PairingEvents ──► Home: Devices::apply (new device ──► live
                                     │                                  device.discovered)
                                     └──► measures (as today), lines ──► sink (host: stdout)
browser ◄── /ws topic devices: device.discovered, device.removed, device.status_changed
browser ──PUT/DELETE /api/v1/devices/:id, POST /api/v1/zones──► existing routes (specs 008, 010)
```

The window is in the core, so the box (E3) gets the route and the behaviour with the API it
serves; the host only changes how it starts the network.

## Data

| Name | Type | Where | Bound |
| ---- | ---- | ----- | ----- |
| `PairingWindow.request` | `Option<u8>` (seconds, 0 closes) | `Home`, behind the home's lock | one; a new request replaces the old |
| `PairingWindow.state` | `Closed` / `Open { ends_at: u64 }` / `Finishing { until: u64 }` (monotonic ms) | `Home` | constant size |
| `PairingWindow.error` | `Option<Box<str>>` | `Home` | 128 characters, cut on a character boundary |
| `PairingWindow.busy` | addresses (`[u8; 8]`) of the devices still being identified or set up, copied by the engine each turn | `Home` | 32 (`MAX_DEVICES`) |
| `PairingWindow.clock` | the orders' monotonic `Clock` | `Home`, set by `engine::run` as the orders' queue's | — |
| Coordinator's followed devices | existing `Pairing.devices` | `radio/coordinator.rs` | 32 (`MAX_DEVICES`), finished ones let go at each opening |
| Interface: `useDevices` | devices by id, data by id | browser | at most 64 devices (the host's bound) |
| Interface: `usePairing` | `permitJoin`, `remaining`, `error`, ids discovered this visit | browser | ids at most 64 |

Nothing new is written to disk: devices and zones are kept by the existing files (`devices.json`,
`zones.json`); `network.json` is written by `--serve` when it forms a network, as by `--pair`.
The interface's first load: about 40 KB gzip expected (34 KB after spec 014), checked by
`npm run check:bundle`. The memory budget (`docs/memory-budget.md`) gains no line: the window is a few dozen bytes in
`Home`, and the coordinator's bound is unchanged; the worst-case test runs as it is.

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/src/home/pairing.rs` (new) | `PairingWindow`: `request(time)`, `state(now)` → `(open, remaining, error)`, `take_request()`, `opened(ends_at)`, `refused(reason)`, `ended()`, `finished()`, `set_busy(addresses)`, `is_busy(eui64)`; pure, tested on the Mac |
| `crates/core/src/home/mod.rs` | `Home` owns a `PairingWindow`; `pairing()` / `pairing_mut()` |
| `crates/core/src/api/devices.rs` | `DELETE` of a device whose address `is_busy`: 409 `Device is being paired` |
| `crates/core/src/api/pairing.rs` (new), `api/mod.rs` | the `integrations` segment: `["integrations", "solite", "permit-join"]`, GET and POST, R1's answers and errors (reading `time` with `api/input.rs`) |
| `crates/core/src/engine/mod.rs` | each turn: take the request (lock), call `permit_join` / `close_joining` (no lock), record the result (lock); end of window → close, then `Finishing` until `!radio.pairing_busy()` or 70 s, then `set_following(false)` and the `cut short` lines; a request while `Finishing` opens a new window with `forget_finished` (which keeps devices mid-setup); the busy addresses copied into the window each turn; `radio.poll_pairing()` in place of `radio.poll()`: measures as today, other events through `Devices::apply`, their `line()` and notes to the sink; recompute also after pairing events; on stop, `close_joining` if open (best effort). `CLOSE_GRACE_MS` (70 000) moves here from the host's `main.rs` (the host's `--pair` uses it, with its test variable) |
| `crates/core/src/radio/coordinator.rs` | `set_following(bool)` (the inverse of `Pairing.listening`), `forget_finished()` (drop devices with `step == Done` and no setup); `permit_join` unchanged |
| `crates/core/src/devices/registry.rs` | `ensure` emits `device.discovered` through the live handle when it creates a device |
| `crates/core/src/live/events.rs`, `live/topics.rs` | `EngineEvent::DeviceDiscovered { device: Device }`, topic `devices`; module comment updated |
| `crates/host/src/serve.rs` | `radio.start_network(...)` with the backup (as `pair` in `main.rs`) in place of `radio.listen()`, then `set_following(false)`; the `network:` line with `foreign` as `pair` computes it; module comment |
| `crates/host/src/main.rs` | `pair` uses the core's grace constant; the start of the network shared with `serve.rs` (a function `start(radio, data, fill)` in `backup.rs` or `main.rs`) |
| `crates/sim/src/` (scenario, NCP) | knobs for the tests, if missing: refuse `permitJoining`, a radio that is not a coordinator, a network that does not come up |
| `firmware/src/simulated.rs` | unchanged (`listen()` then `engine::run`: no request ever comes, the window stays closed); compiled and run in QEMU by the validate command |
| `ui/src/types.ts` | Sowel's `Device`, `DeviceData`, `DeviceStatus`, `PowerSource` (copied from `../sowel/ui/src/types.ts`) |
| `ui/src/api/devices.ts` (new) | `getDevices`, `updateDevice(id, {name?, zoneId?})`, `deleteDevice` (Sowel's `api/devices.ts` names) |
| `ui/src/api/pairing.ts` (new) | `getPermitJoin`, `setPermitJoin(time)` |
| `ui/src/api/zones.ts` | `createZone({name})` (Sowel's name) |
| `ui/src/store/useDevices.ts` (ported) | Sowel's store unchanged, plus `updateDeviceZone` and `deleteDevice` (marked as Solite's additions) |
| `ui/src/store/usePairing.ts` (new) | the window's state, `start()`, `stop()`, `fetch()`, the 1 s countdown, the read 1 s after a `POST` and at 0, the "New" ids |
| `ui/src/store/useLive.ts` | topic `devices`; `device.discovered`, `device.removed`, `device.status_changed` to `useDevices` (and the id to `usePairing`'s "New"); `refetchAll` also reads devices and the pairing state |
| `ui/src/components/devices/DeviceNameEditor.tsx` (ported) | Sowel's, unchanged |
| `ui/src/components/devices/DevicesPage.tsx`, `DeviceRow.tsx`, `ZoneSelect.tsx` (new) | R5's page, row and room select with "New room…" |
| `ui/src/components/layout/Sidebar.tsx`, `ui/src/zones.ts`, `ui/src/App.tsx` | the "Devices" entry; `#/devices` recognised; the page shown |
| `ui/src/i18n/messages.ts:81`, `:162` | `app.noRoom` en "No room yet: create one from the devices page", fr "Aucune pièce : créez-en une depuis la page des appareils" |
| `ui/src/App.test.tsx:84`, `:90`; `ui/src/i18n/i18n.test.ts:39` | the new `app.noRoom` texts expected |
| `ui/src/i18n/messages.ts` | Sowel's `devices.title`, `devices.empty.title`, `devices.saveFailed`; Solite's `devices.add`, `devices.pairingOpen`, `devices.stop`, `devices.new`, `devices.noRoom`, `devices.newRoom`, `devices.deleteConfirm`, `devices.emptyMessage`, `devices.status.*` (fr, en) |
| `docs/living-spec/README.md` (sections and History row 017), `docs/decisions.md` (D28 with the box constraint, D18), `docs/roadmap.md` (E3 constraint line), `docs/architecture.md`, `scripts/demo-garage.sh` (comment) | the living spec delta and the sentences made false (spec, "No longer true") |

## Reused (Phase 1.3)

- `main.rs::pair` (`crates/host/src/main.rs:202-304`): the order of events, routes and activity,
  the closing and the grace, the `cut short` lines; R2 moves this logic into the core's loop.
- The orders' queue (`orders/`, `Home::orders`, `engine::run`): a request recorded under the home's
  lock by the API, executed by the radio loop between two polls, on the monotonic clock set by
  `engine::run`.
- `api/input.rs`: reading a JSON object with Sowel's error texts.
- `live/`: the events built only when a client listens, the `devices` topic, the dedup (a
  `device.discovered` has no dedup key: each is kept).
- Sowel's `useDevices.ts` and `DeviceNameEditor.tsx`, ported as the stores of spec 014.
- Spec 014's `flattenZones` (`ui/src/zones.ts`) for the room select.
- The sim's `waiting_joins` (`crates/sim/src/transport.rs:116-128`): a scenario's `join` waits
  until joining is permitted, so `garage.json` joins at the first `POST`.
