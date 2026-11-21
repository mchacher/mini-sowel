# Plan: Spec 017

## Steps

- [x] 1. Living spec updated (spec, "Living spec delta", History row 017); D28 written with the
      box constraint, D18 amended, the E3 constraint in the roadmap; the
      sentences of "No longer true" corrected.
- [x] 2. Core, window: `home/pairing.rs` with its unit tests; `Home` owns it.
- [x] 3. Core, API: `api/pairing.rs`, the `integrations` segment in `api/mod.rs`; tests in
      `crates/core/tests/api.rs`.
- [x] 4. Core, coordinator: `set_following`, `forget_finished`; tests in
      `crates/core/tests/coordinator.rs`.
- [x] 5. Core, live: `device.discovered` from `Devices::ensure`; tests in
      `crates/core/tests/live.rs`.
- [x] 6. Core, engine: the window in `engine::run`, `poll_pairing`, the lines to the sink, the
      grace, the close on stop; `CLOSE_GRACE_MS` in the core; tests in
      `crates/core/tests/engine.rs` with the simulator.
- [x] 7. Host: `--serve` starts the network (shared with `pair`), `set_following(false)`;
      `main.rs` uses the core's grace; tests in `crates/host/tests/serve.rs` and `ws.rs`.
- [ ] 8. Interface: types, `api/devices.ts`, `api/pairing.ts`, `createZone`, `useDevices`
      (ported), `usePairing`, `useLive`, `DeviceNameEditor` (ported), `DevicesPage`,
      `DeviceRow`, `ZoneSelect`, sidebar entry, `#/devices`, messages; Vitest tests beside them.
- [ ] 9. `docs/architecture.md`, `scripts/demo-garage.sh` comment; `npm run check:bundle`.
- [ ] 10. `sh scripts/validate.sh` green (QEMU's run and memory gates included).
- [ ] 11. Code review (`code-reviewer`), demo on the simulator, then with the ZBMINIR2.

## Test plan

### Rust, core (`crates/core/tests/`, and `#[cfg(test)]` in `home/pairing.rs`)

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| `home/pairing.rs` | New window | closed, remaining 0, no error |
| `home/pairing.rs` | `request(60)` before the engine took it | state reports open, remaining 60 |
| `home/pairing.rs` | `opened(ends_at)`, clock 10.2 s later | open, remaining 50 (rounded up) |
| `home/pairing.rs` | Clock past `ends_at` | `state` reports closed, remaining 0 |
| `home/pairing.rs` | `refused("...")` | closed, `error` set; next `request` clears it |
| `home/pairing.rs` | Error of 300 characters, multibyte | kept at 128 characters, valid UTF-8 |
| `home/pairing.rs` | Two requests before a turn | the last one wins |
| `api.rs` | `GET .../solite/permit-join` | 200 `{"permitJoin":false,"remaining":0}` |
| `api.rs` | `POST {"time":60}` | 202 `{"permitJoin":true,"remaining":60}`; request 60 recorded |
| `api.rs` | `POST {}` | 202, remaining 180 |
| `api.rs` | `POST {"time":0}` while closed and while open | 202 `{"permitJoin":false,"remaining":0}`; request 0 |
| `api.rs` | `POST {"time":254}` and `{"time":1}` | 202 with 254, 1 |
| `api.rs` | `time` 255, -1, 1.5, `"60"`, `null` | 400 `Invalid field: time` |
| `api.rs` | `{"time":60,"x":1}`; body `[]` | 400 `Unknown field: x`; 400 `Invalid JSON` |
| `api.rs` | `.../integrations/zigbee2mqtt/permit-join`; `.../solite/other` | 404 `Integration not found`; 404 `Not found` |
| `api.rs` | `PUT` and `DELETE` on the route | 405 |
| `api.rs` | `POST` with `text/plain`; wrong `Host` | 415; 403 |
| `api.rs` | `GET` after a refusal recorded | 200 with `"error"` |
| `api.rs` | Refusal recorded, then `POST {"time":30}`, then `GET` | no `"error"` field |
| `api.rs` | `POST` with no engine turn yet, then `GET` | open, remaining as asked |
| `api.rs` | `DELETE /devices/:id` of a busy device; after it is done | 409 `Device is being paired`; 204 |
| `coordinator.rs` | Listening (after `listen`), a device joins | no `Joined` event (unchanged) |
| `coordinator.rs` | `set_following(true)`, `permit_join`, a device joins | `Joined`, `Identified`, `ReportingConfigured`, as with `--pair` |
| `coordinator.rs` | `set_following(false)` after the device is done | a later announce is not followed |
| `coordinator.rs` | 32 devices finished, `forget_finished`, a 33rd joins | followed, no `TooManyDevices` |
| `coordinator.rs` | 32 devices still being identified, a 33rd joins | `TooManyDevices` (unchanged) |
| `coordinator.rs` | `forget_finished` with a device mid-setup | that device kept |
| `live.rs` | A new device identified, a client on `devices` | one `device.discovered` with Sowel's `Device` fields, no `data`/`orders` |
| `live.rs` | A known device identified again | no `device.discovered` |
| `live.rs` | No client on `devices` | no event built |
| `live.rs` | 64 devices discovered in one batch, one client | each kept (no dedup); past 256 events or 8 KB the client closed 1013, the others unaffected |
| `live.rs` | Serialization | `{"type":"device.discovered","device":{"id",...,"integrationId":"solite",...}}`, camelCase as `GET /devices/:id` |
| `engine.rs` | Sim `garage.json`, no request | no device joins; measures of known devices as before |
| `engine.rs` | Request 30, sim devices join | three devices in `Home`, lines `pairing: open for 30 s`, `joined:`, `device:`, `reporting:` to the sink |
| `engine.rs` | Window time ends | `close_joining` sent, `pairing: closed`; a scenario join after the end does not join |
| `engine.rs` | Identification in progress at the end | it finishes; then the coordinator listens only |
| `engine.rs` | Setup still running at the 70 s cap | `reporting: <address> cut short`, listening |
| `engine.rs` | Request 0 while open | closed at the next turn |
| `engine.rs` | New request while open | window restarts with the new time |
| `engine.rs` | The radio refuses `permitJoining` (sim's refusal knob, added to `crates/sim` if missing) | window closed, `error` set, `pairing: failed: ...` line, the loop goes on |
| `engine.rs` | Request while `Finishing`, one device still mid-setup | new window opened; that device kept and its setup finishes; finished ones let go |
| `engine.rs` | Request recorded before the first turn | applied at the first turn: `pairing: open for <n> s` |
| `engine.rs` | Each turn during a window | the busy addresses in the window match `setups_in_progress` and identifications |
| `engine.rs` | A device deleted from the home announces itself during a window | added again with a new id and default name, `device.discovered` |
| `engine.rs` | Stop while open | `close_joining` sent, `Ok(true)` |
| `engine.rs` | Device store full (64) during a window | note line, no device, no event |
| `engine.rs` | Orders and measures during a window | executed and applied as outside it |
| `memory_worst_case.rs` | Unchanged bounds, the window in `Home` | the test passes as it is |

### Rust, host (`crates/host/tests/`)

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| `serve.rs` | `--sim garage.json --serve` on an empty folder | `network: formed, ...` printed, `network.json` written, `GET /devices` empty |
| `serve.rs` | `POST permit-join {"time":30}` then wait | the three devices in `GET /devices` with data; `joined:`/`device:` lines on stdout without `--verbose` |
| `serve.rs` | `GET permit-join` during and after the window | remaining decreases, then `{"permitJoin":false,"remaining":0}` |
| `serve.rs` | Restart `--serve` on that folder | `network: resumed, ...`, devices kept (unchanged behaviour) |
| `serve.rs` | Data folder not writable | `cannot write <path>` (or the existing writable-check text), exit 1, no `serving:` line |
| `serve.rs` | Scenario radio that is not a coordinator (sim knob) | `radio: the radio is not a coordinator`, exit 1 |
| `serve.rs` | Scenario whose network does not come up (sim knob) | `radio: network did not start`, exit 1 |
| `serve.rs` | Scenario `"network": "formed"` with no backup in the folder | `network: resumed (made by another program), ...`, serving goes on, nothing formed |
| `serve.rs` | Ctrl-C while the window is open | exit 130, store written |
| `serve.rs` | Existing tests (`--pair` then `--serve`, usage errors, ports, devices file) | unchanged and green |
| `ws.rs` | Client subscribed to `devices`, window opened | one `device.discovered` per new device |
| `listen.rs`, `pair.rs` | Existing tests | unchanged (`--listen` still refuses a radio with no network) |

### Interface (Vitest, beside the files)

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| `api/pairing.test.ts` | `setPermitJoin(180)`; `getPermitJoin()` | `POST` JSON `{"time":180}` to the route; parsed answer |
| `store/usePairing.test.ts` | `start()` | open, remaining from the answer; ticks down each second (fake timers) |
| `store/usePairing.test.ts` | Countdown reaches 0 | `GET` once, closed |
| `store/usePairing.test.ts` | 1 s after `start()`, `GET` gives an `error` | closed, error shown |
| `store/usePairing.test.ts` | `stop()` | `POST {"time":0}`, closed |
| `store/stores.test.ts` | `useDevices.addDevice`, `removeDevice`, `updateDeviceStatus` | the map follows |
| `store/stores.test.ts` | `updateDeviceZone` refused (404) | the old zone back, the error thrown |
| `store/useLive.test.ts` | Open | subscribes `zones`, `equipments`, `devices` |
| `store/useLive.test.ts` | `device.discovered` / `device.removed` / `device.status_changed` | in `useDevices`; the id marked new in `usePairing` |
| `store/useLive.test.ts` | Reconnection | devices and pairing state read again |
| `components/devices/DeviceNameEditor.test.tsx` | Sowel's test ported | as in Sowel |
| `components/devices/DevicesPage.test.tsx` | "Add a device" | `POST`, countdown and "Stop" shown |
| `components/devices/DevicesPage.test.tsx` | A device discovered | its row appears first, marked "New" |
| `components/devices/DevicesPage.test.tsx` | Rename | `PUT {"name"}`; a 400 shows its text |
| `components/devices/DevicesPage.test.tsx` | Choose a room; "No room" | `PUT {"zoneId": id}`; `PUT {"zoneId": null}` |
| `components/devices/DevicesPage.test.tsx` | "New room…" with a name; with an empty name | `POST /zones {"name"}` then `PUT`; nothing sent |
| `components/devices/DevicesPage.test.tsx` | Only the root zone | options "No room" and "New room…" |
| `components/devices/DevicesPage.test.tsx` | Delete, confirmed / cancelled | `DELETE`, row gone / nothing sent |
| `components/devices/DevicesPage.test.tsx` | Delete refused 409; 404 | text shown 5 s, row kept; row removed |
| `components/devices/DevicesPage.test.tsx` | Room change refused (404) | text shown 5 s, the select back to the old zone |
| `components/devices/DevicesPage.test.tsx` | "New room…" refused (zone limit) | text shown, no `PUT` |
| `components/devices/DevicesPage.test.tsx` | "New room…" made, `PUT` refused | `DELETE /zones/<new id>` sent, the `PUT`'s text shown |
| `components/devices/DevicesPage.test.tsx` | Connection lost during the window | the countdown goes on; on reconnection `GET` permit-join and devices |
| `components/devices/DevicesPage.test.tsx` | A known device's `status_changed` during the window | no "New" mark |
| `components/devices/DevicesPage.test.tsx` | No device | the empty title and message |
| `App.test.tsx` | `#/devices`; the sidebar; the phone drawer | the devices page; the "Devices" entry in both; room pages unchanged |
| `App.test.tsx` (`:84`, `:90`), `i18n/i18n.test.ts` (`:39`) | Only the root; no zone; French | "No room yet: create one from the devices page"; "Aucune pièce : créez-en une depuis la page des appareils" |

## Validation

- [ ] `npm run check:bundle`: about 40 KB gzip
- [ ] Validate command green (bundle budget, QEMU run and memory gates included)
- [ ] Agent review done, blocking findings fixed
- [ ] Demo seen and approved by the user (simulator, then the ZBMINIR2)
