# Spec 017: add a device from the interface

> **Superseded in part by spec 019** (the text below stays as the record of what was built):
> Goal 3 "place them in a room (an existing one, or a new one made on the spot)"; Out of scope
> "Editing rooms beyond creating one under the home's root"; R5's room select, the whole "Room
> select" bullet ("No room", "New room…", the zone made under the root, the rollback) and "Empty
> home text"; the living spec delta's "the list with rename, room (including 'New room…')" and
> History "named and placed in a room"; the acceptance "renaming, choosing a room, 'New room…'";
> the edge cases "'New room…' refused (32 zones)", "A rename or zone refused", "'New room…' with an
> empty name", "Only the root zone: the select offers 'No room' and 'New room…'"; Decided in
> autonomy "'New room…' in the room select"; "How the user will check it" ("choose 'New room…'
> 'Garage'", "is renamed and placed in the garage"). A device has no room: the room is its
> equipment's, chosen in the equipment form.

## Context

Increment J1 of the roadmap (milestone J, the autonomous box): add a device from the interface.
Today a device is paired only by `--pair`, a mode of its own that cannot run with `--serve`
(spec 005); the host must be stopped, run with `--pair` in a terminal, then started again with
`--serve`. While serving, the coordinator only listens (`Radio::listen`): a device that joins is
not followed, and `--serve` never forms a network (`radio: no network, pair a device first`).
Spec 013 left out Sowel's `device.discovered` event for that reason (decision D18: "no pairing
while serving").

This spec opens pairing **while serving**, from the API and from the interface: the user presses
"Add a device", the new device appears live in a devices page, and the user names it and places
it in a room, with no terminal. Done when the ZBMINIR2 is added this way (roadmap, J1).

The logic goes in the core (the pairing window, the API route, the engine loop), so the box
(milestone E3) gets it with the API it serves. Specified in autonomy (no questions); choices under
"Decided in autonomy".

## Goals

1. While serving (`--serve`, with `--sim` or `--radio`), pairing opens and closes through the API;
   the engine follows the devices that join, identifies them, sets up their reporting, and adds
   them to the home, as `--pair` does.
2. A new device is announced live (Sowel's `device.discovered` on the `devices` topic).
3. The interface has a devices page: start and stop pairing with a countdown, see the devices
   appear, rename them, place them in a room (an existing one, or a new one made on the spot),
   delete one.
4. `--serve` starts the network like `--pair` does: on a radio with no network it forms one, so a
   fresh box or a fresh simulator is set up from the browser alone.

## Out of scope

- Editing rooms beyond creating one under the home's root (rename, move, delete, the tree): J2.
- Equipments made from the new device (a light from the ZBMINIR2): J2. Recipes: J3.
- Showing a device's data or orders in the devices page (Sowel's `DeviceDataTable`), the device
  detail page, grouping and filters of Sowel's `DeviceList`.
- Authentication of the pairing route (D13; J4 brings the password).
- A live event for the pairing window itself (Sowel has none; see R6).
- `--pair` and `--listen`: unchanged.

## Requirements

### R1: The pairing route

Sowel has no pairing route (in Sowel, Zigbee2MQTT pairs). Solite's sits under Sowel's integration
namespace (`/api/v1/integrations/:id/...`, Sowel's `src/api/routes/integrations.ts`), with
Solite's integration id `solite` (the devices' `integrationId`), and takes the names of
Zigbee2MQTT's bridge request `permit_join` (`time` in seconds, 0 to close):

| Route | Answer |
| ----- | ------ |
| `GET /api/v1/integrations/solite/permit-join` | 200 `{"permitJoin": <bool>, "remaining": <seconds>}`, plus `"error": "<text>"` when the last opening failed |
| `POST /api/v1/integrations/solite/permit-join` `{"time"?: 0..254}` | 202 `{"permitJoin": <bool>, "remaining": <seconds>}`: the state asked for |

- `time` missing (`{}`): 180 s, the default of `--pair`. `time` 1 to 254 opens joining for that
  long; a window already open restarts with the new time. `time` 0 closes it (202
  `{"permitJoin": false, "remaining": 0}`, also when it was closed).
- `time` not an integer from 0 to 254 (negative, 255, `1.5`, a string, `null`): 400
  `{"error":"Invalid field: time"}`; another field: 400 `Unknown field: <field>`; a body that is
  not a JSON object: 400 `Invalid JSON` (the texts of the other routes, `api/input.rs`). Another integration id: 404
  `{"error":"Integration not found"}`. Another method: 405. The API's checks apply: `Host` (403),
  `application/json` on POST (415), body size (413), workers (503).
- `remaining` is the whole seconds left, rounded up, from the engine's monotonic clock; 0 when
  closed. A request the engine has not applied yet is reported as asked (`GET` right after a
  `POST` agrees with it).
- `error`: the radio's refusal of the last opening (`the radio refused permitJoining (status
  0x<code>)`), at most 128 characters, cleared by the next `POST`.

### R2: The engine follows the devices while the window is open

- The engine (the core's loop, `engine::run`) takes the request at its next turn, outside the
  home's lock: it opens joining on the radio (`permit_join`), the window is open until its time
  ends; it closes joining (`close_joining`) at the end, on `time` 0, and on stop (Ctrl-C, best
  effort, as `--pair`).
- From the opening until the end of the window, and then while identifications, enrollments and
  reporting setups started in it are still running (at most 70 s after the end, as `--pair`), the
  coordinator follows the devices: a device that joins or announces itself is identified, its
  reporting set up, an alarm device enrolled, exactly as with `--pair` (spec 005, 006, 009).
  After that it only listens again, as today. A setup still running at the cap prints
  `reporting: <address> cut short`; the setup is then abandoned.
- Each pairing event is applied to the home (`Devices::apply`): a new device is added with its
  default name (`ZBMINIR2 567A`), its data and orders; one already known keeps its id, name and
  zone. The lines `--pair` prints (`joined:`, `device:`, `reporting:`, notes such as `pairing:
  device store full (64), <address> ignored`) go to the engine's sink, always (not only with
  `--verbose`); the host prints them on its output; also `pairing: open for <n> s`, `pairing:
  closed`, and `pairing: failed: <reason>` when the radio refuses the opening.
- A refusal by the radio (`Refused`) does not stop the engine: the window stays closed and
  `error` is set (R1). A transport error ends the loop, as for any radio error today.
- Measures, orders, recipes and saves go on during the window as at any other time.
- When a window opens, the coordinator lets go of the devices whose pairing finished in earlier
  windows (identified, no setup running), so the limit of 32 devices followed applies to one
  window, as to one `--pair` session; a failed reporting setup of such a device is then no longer
  retried at its next message (pair it again).
- **A `POST` while the window is finishing** (closed, setups of its devices still running): a new
  window opens at once; the devices still being set up are kept and go on in the new window
  (`forget_finished` skips them), so they count in its 32.
- **A `POST` before the engine's first turn** (the host still starting the radio): kept as the
  request and applied at the first turn; `GET` in the meantime reports it as asked (R1).
- **Delete during an identification**: the engine copies, each turn, the addresses of the devices
  still being identified or set up (at most 32) into the window; `DELETE /api/v1/devices/:id` of
  such a device answers 409 `{"error":"Device is being paired"}`. Once its setup is done it can be
  deleted.
- **A device deleted from the home but still in the Zigbee network** that announces itself during
  a window is followed like any other and comes back as a new device (new id, default name,
  `device.discovered`); outside a window it is not followed and its messages are ignored, as today.
  A setup still running when it is deleted never brings it back: only a new identification does.

### R3: `--serve` starts the network

`--serve` starts the network like `--pair`: it resumes the radio's network, or forms one when the
radio has none (channel 15, random keys, the backup written to the data folder before any later
step), and prints the same `network:` line (`network: formed, ...`, `network: resumed, ...`,
`network: resumed (made by another program), ...`). Joining stays closed until a `POST` opens it.
The errors of `--pair`'s start apply, with the same texts on the error output and exit 1, before
`serving:` is printed and before the port is answered: `radio: network did not start`, `radio: the
radio is not a coordinator`, `cannot write <path>` (the backup). A coordinator network the host has
no backup of is resumed and named `network: resumed (made by another program), ...`, never
destroyed, and serving goes on.

### R4: `device.discovered`

When the home adds a device it did not have (R2), Sowel's event `{"type":"device.discovered",
"device": <Device>}` is sent on the `devices` topic (Sowel's `types.ts`, `EngineEvent`): the
`Device` as `GET /api/v1/devices/:id` gives it, without `data` and `orders`. Not sent for a device
already known (Sowel's `device-manager.ts` emits it on first discovery only), nor when no client
listens to `devices`. Built once, shared by the clients, under the queue bounds of spec 013.

### R5: The devices page

- **Reached** from a "Devices" entry in the sidebar under the rooms (and in the phone drawer), at
  `#/devices`. The room pages are unchanged.
- **Header**: Sowel's title "Devices" ("Appareils") and the count; a button "Add a device"
  ("Ajouter un appareil"). Pressed, it sends `POST {"time": 180}`; while open the header shows
  "Pairing open: put the device in pairing mode" with the seconds left, counting down each second
  from the answer's `remaining`, and a "Stop" button (`POST {"time": 0}`). At 0 it reads the state
  again (`GET`). One second after a `POST`, it reads the state once to show a refusal (`error`).
- **List**: every device, newest first (pairing order reversed), each row: Sowel's
  `DeviceNameEditor` (the pencil, Enter to save, Escape to cancel; 1 to 64 characters, the API's
  refusal shown under it), the manufacturer and model, the status (online, offline, unknown,
  Sowel's colours), a room select, a delete button. A device discovered since the page opened is
  marked "New" ("Nouveau") until the page is left.
- **Room select**: "No room" (`zoneId` null) and every zone except the home's root, indented by
  depth as in the sidebar; changing it sends `PUT /api/v1/devices/:id {"zoneId": ...}`; the last
  option "New room…" ("Nouvelle pièce…") shows a name field (1 to 64 characters) that creates the
  zone under the root (`POST /api/v1/zones {"name"}`, no parent: under the root) and then places the
  device in it. A refusal shows its text for 5 s, and the select shows the device's zone again. The
  zone limit (32, spec 010) refused by `POST /zones`: its text shown, nothing created. When the
  `PUT` fails after the zone was created, the interface deletes that new zone (best effort) and
  shows the `PUT`'s text.
- **Delete**: asks Sowel's confirmation ("Delete this device?..." in Solite's words: it stays in
  the Zigbee network, pairing it again brings it back), then `DELETE /api/v1/devices/:id`. A refusal (409 while it is
  being paired, 404 already gone) shows its text for 5 s; on 404 the row is removed.
- **Empty**: Sowel's `devices.empty.title` ("No devices discovered") with Solite's message "Press
  Add a device, then put the device in pairing mode."
- **Live**: the page subscribes the `devices` topic: `device.discovered` adds a row,
  `device.removed` removes it, `device.status_changed` updates the status. On every reconnection
  the devices and the pairing state are read again, as the other stores are.
- **Language and look**: French and English, Sowel's keys where Sowel has them; Sowel's tokens
  and classes; the first load stays under the 170 KB gzip budget, about 40 KB expected (34 KB in
  spec 014), confirmed by `npm run check:bundle`.
- **Empty home text**: `app.noRoom` becomes "No room yet: create one from the devices page" /
  "Aucune pièce : créez-en une depuis la page des appareils".

### R6: No event for the window

The window's state is read with `GET` (on opening the page, after a `POST`, at the countdown's
end, on reconnection). Two browsers: the second sees the window when it reads the state; a `Stop`
from one is seen by the other at its countdown's end. Decided so that Solite sends only Sowel's
events (D18).

## Living spec delta

### docs/living-spec/README.md

- **Pairs a device**: New: "While serving, pairing also opens from the API or the interface (see
  'Serves Sowel's device API' and 'Shows the home in the browser'); the devices that join are
  followed, identified and kept exactly as with `--pair`, with the same lines."
- **Serves Sowel's device API**: Changed: "`--serve` ... resumes the network, listens" → "starts
  the network as `--pair` does (forms one on a radio that has none, with its backup), listens";
  New rows in the table: `GET /api/v1/integrations/solite/permit-join` and `POST ... {"time"?}`
  with R1's answers and errors; New paragraph: the window (R2): the lines printed, the 70 s for
  the setups in progress, a refusal kept in `error`, joining closed on Ctrl-C; the 32 devices
  followed per window.
- **Live events**: Changed: the `devices` topic: "(data updated, status changed, removed)" →
  "(discovered, data updated, status changed, removed)".
- **Shows the home in the browser**: New bullet "**Devices**": the sidebar's entry, `#/devices`,
  the button, the countdown and Stop, the list with rename, room (including "New room…"), delete,
  the "New" mark, live. Changed: "No room yet: create one with the API" → "No room yet: create one
  from the devices page".
- **History**: New row `| 017 | Pairing while serving, from the API and the interface's devices
  page: new devices appear live, named and placed in a room |`.

### docs/decisions.md

- New decision D28 "Pairing while serving": the route under `/integrations/solite/permit-join`
  with Zigbee2MQTT's `time` (Sowel has no pairing route), 202 and the state asked for, no event
  for the window, `--serve` forms a network, one window at a time. The route has no
  authentication (D13): on the box (E3) it stays disabled (404) until J4's password exists.
- D18 amended: `device.discovered` is sent (its "(no pairing while serving)" removed).

### docs/roadmap.md

- Row E3: a constraint added: "the pairing route stays disabled in the box until J4 (D28)".

### No longer true

- `docs/living-spec/README.md` "Serves Sowel's device API": "resumes the network, listens" →
  "starts the network as `--pair` does, listens" (R3).
- `docs/living-spec/README.md` "Shows the home in the browser": "No room yet: create one with the
  API" → "No room yet: create one from the devices page".
- `ui/src/i18n/messages.ts:81` (en `app.noRoom`) "No room yet: create one with the API" → "No room
  yet: create one from the devices page"; `ui/src/i18n/messages.ts:162` (fr) "Aucune pièce :
  créez-en une avec l'API" → "Aucune pièce : créez-en une depuis la page des appareils".
- `ui/src/App.test.tsx:84` and `:90`: the expected English text, as above.
- `ui/src/i18n/i18n.test.ts:39`: the expected French text, as above.
- `docs/decisions.md` D18: "not sent: `device.discovered` (no pairing while serving)" → removed
  from the list of events not sent.
- `docs/architecture.md`, block "Devices and API": add the pairing window in `home/` and the
  route in `api/`; block "Interface": the devices page.
- `crates/host/src/serve.rs` module comment: "`--serve`: resume the network" → "start the
  network".
- `crates/core/src/live/events.rs` module comment: "(`device.discovered`, modes, ...) are not
  here" → `device.discovered` removed from that list.
- `crates/core/src/radio/coordinator.rs` `listen`: "Joining stays closed; the devices are not
  followed" stays true for `listen`; the new functions say when they are followed.
- `scripts/demo-garage.sh` comment "(`--sim scenarios/garage.json --serve`, after `--pair`)":
  "after `--pair`, or after adding the devices from the interface".
- Searched, none found: "pairing screens" (spec 014's out of scope, a past spec, left as it is),
  "never forms" (said of `--listen`, still true).

## Acceptance criteria

- [ ] With `--sim scenarios/garage.json --serve` on an empty data folder, the host prints
      `network: formed, ...`; no device joins before pairing is opened.
- [ ] `POST /api/v1/integrations/solite/permit-join {"time": 30}` answers 202; the three
      simulated devices then show in `GET /api/v1/devices` with their data, and their `joined:`,
      `device:` and `reporting:` lines are printed.
- [ ] A WebSocket client subscribed to `devices` receives one `device.discovered` per new device,
      none for a device already known.
- [ ] `GET .../permit-join` counts down, then answers `{"permitJoin": false, "remaining": 0}`;
      `{"time": 0}` closes it at once.
- [ ] Wrong `time`, another integration id, another method: 400, 404, 405 with their texts.
- [ ] In the interface, "Add a device" shows the countdown; the new devices appear marked "New";
      renaming, choosing a room, "New room…" and delete work and survive a reload.
- [ ] `--pair`, `--listen` and a `--serve` on an existing network behave as before; the QEMU run
      and its memory gates stay green.
- [ ] The ZBMINIR2 is added from the interface with the real Dongle Max, no terminal used after
      starting the host (roadmap J1's "done when").

## Edge cases

- `time` missing, 0, 1, 254, 255, negative, decimal, string, `null`; body not an object.
- `POST` while open: the window restarts with the new time. `{"time": 0}` while closed: 202,
  nothing sent to the radio.
- `POST` before the engine's first turn (the host still starting the radio): applied at its first
  turn.
- The radio refuses the opening: window closed, `error` set, the line printed, the engine goes
  on; the next `POST` clears `error`.
- Closing fails at the end: a warning; the radio closes joining by itself at the end of the time
  it was given.
- Identification still running when the window ends: it finishes (at most 70 s), then
  `reporting: <address> cut short` for a setup cut.
- More than 32 devices joining in one window: `pairing: too many devices, <address> ignored`. A
  33rd device in a later window: accepted (R2).
- Device store full (64): the note line, no device, no event.
- A known device pairing again: no `device.discovered`, no "New" mark in the interface; it keeps
  id, name, zone.
- A device deleted from the home, still in the network, announcing itself during a window: back
  as a new device. `DELETE` of a device being paired: 409.
- `POST` while the window is finishing: a new window, the devices still being set up kept.
- `--serve` start errors: the texts of `--pair`, exit 1, nothing served.
- Many devices discovered at once: each `device.discovered` is kept (no dedup), under spec 013's
  queue bounds (256 events, 8 KB): a client past them is closed 1013 and reconnects, then reads
  the devices again.
- "New room…" refused (32 zones): the text shown, nothing created; the `PUT` refused after the
  zone was made: the zone deleted again (best effort).
- A device that joins while the window is closed: not followed (the radio refuses joins anyway).
- Ctrl-C while open: joining closed (best effort), the store written, exit 130.
- No client subscribed to `devices`: no event built.
- Interface: the connection lost during the window: the countdown goes on; on reconnection the
  state and the devices are read again. A rename or zone refused (400, 404): its text shown, the
  old value back. "New room…" with an empty name: nothing sent. Only the root zone: the select
  offers "No room" and "New room…". The devices page with no device: the empty text.

## Sowel contract

- `Device` (`../sowel/src/shared/types.ts`, `interface Device`), unchanged, carried by
  `device.discovered` (`EngineEvent`, same file); emitted once per new device as Sowel's
  `src/devices/device-manager.ts`.
- `PUT /api/v1/devices/:id`, `DELETE /api/v1/devices/:id`, `POST /api/v1/zones` (spec 008, 010),
  unchanged.
- Interface: Sowel's `ui/src/components/devices/DeviceNameEditor.tsx` and
  `ui/src/store/useDevices.ts` ported; Sowel's keys `devices.title`, `devices.empty.title`,
  `devices.saveFailed`.
- Divergence (decision D28): the pairing route, which Sowel does not have; its field `time` is
  Zigbee2MQTT's (`bridge/request/permit_join`), its answer Solite's own.

## Decided in autonomy

- The route `/api/v1/integrations/solite/permit-join` over a top-level `/api/v1/pairing`: Sowel
  groups what an integration does under `/integrations/:id`; Zigbee2MQTT's `time` (0 closes) over
  a pair of open and close routes.
- `POST` answers 202 with the state asked for, rather than waiting for the radio as an order does:
  the opening is rare, a refusal is read with `GET` (`error`); no new wait mechanism.
- No live event for the window (only Sowel's events, D18); the interface counts down from
  `remaining` and reads the state at the ends.
- `--serve` forms a network when the radio has none (R3): without it a fresh box or simulator
  needs a terminal, against J1's goal. A coordinator network of another program is resumed, never
  destroyed, as with `--pair`.
- Default time 180 s, as `--pair`; a new `POST` restarts the window.
- The lines of pairing printed while serving without `--verbose`: they are rare and they are what
  the user looks for when a device does not show.
- Finished devices let go at each opening (R2) rather than a coordinator that follows up to 32
  devices for the host's whole life (a month for the box).
- A devices page of Solite's own, with Sowel's `DeviceNameEditor` and `useDevices` ported, rather
  than Sowel's `DevicesPage` and `DeviceList` (267 and 416 lines, grouping, filters, data table:
  out of scope and of budget).
- "New room…" in the room select (a zone under the root): without it, adding the ZBMINIR2 to a
  room on a fresh box needs `curl`; the rest of room editing stays J2.
- Delete in the devices page: the ZBMINIR2 is already known from E2's demos; deleting it, then
  pairing it again, is how the demo shows it appear.
- The `devices` topic subscribed for the whole session (at most 64 devices; the interface ignores
  `device.data.updated`, which spec 013's dedup and bounds keep small).
- After the spec review: the zone limit and the rollback of "New room…" (R5); no "New" mark for
  a known device pairing again (Sowel sends no event then); a deleted device comes back as new when
  it announces itself in a window, and `DELETE` of a device being paired is refused with 409 (R2);
  a `POST` while finishing keeps the devices still being set up; about 40 KB gzip expected; the
  pairing route disabled on the box until J4 (D28, roadmap E3), since it has no authentication.
- The spec review (`spec-reviewer`, skill step 2.6) is left to the conversation that launched
  this spec.

## How the user will check it

1. Simulator: `cargo run -p solite-host -- --sim scenarios/garage.json --serve --speed 10
   --data <empty folder>`; open `http://127.0.0.1:8080/#/devices`; press "Add a device": the
   countdown runs, TH01, MS01 and ZBMINIR2 appear marked "New"; rename the relay "Lumière
   garage", choose "New room…" "Garage"; reload: the names and the room are kept; "Stop" closes
   the window.
2. Real hardware: `cargo run -p solite-host -- --radio tcp://<address>:6638 --serve`; in the
   devices page delete the ZBMINIR2 known from E2, reset it (button held 5 s), press "Add a
   device": it appears, is renamed and placed in the garage; no terminal after the start. The
   result of the run (date, what was seen) is written here.
