# Spec 011: orders

## Context

Increment B5 of the roadmap. Specs 008 and 010 list device orders (`state` for an on/off device,
`brightness` for a dimmable one) and bind them to equipments under aliases. This increment executes
them: `POST /api/v1/equipments/:id/orders/:alias {"value": ...}`, as in Sowel, sends the matching
Zigbee command to each bound device, so the user's Sonoff ZBMINIR2 relay switches. It is what B6's
`motion-light` uses.

Sowel code mirrored: `../sowel/src/api/routes/equipments.ts` (the route: `200 {"success":true}`,
`502 {"error": <reason>}`), `../sowel/src/equipments/equipment-manager.ts` (`runOrder`: 404
`Equipment not found`, 400 `Equipment is disabled`, 404 `Order alias not found: <alias>`;
`resolveOrderValue`: a boolean order with an empty value means `true`; enum values matched without
case; `resolveWireValue`: `true` / `false` mapped to the order's `valueOn` / `valueOff`;
`dispatchWithRetry`: one retry after 2 s; success when at least one bound device took the order),
`DeviceOrder` (`types.ts:236`).

Specified in autonomy (the user approved building milestone B as far as possible without
hardware, without questions); choices under "Decided in autonomy".

## Goals

1. An equipment's order alias switches or dims the bound Zigbee devices.
2. The answer says whether a device took it.
3. The radio is used only by the radio loop: HTTP never waits on a lock while the radio works.

## Out of scope

- Zone orders (`allLightsOff`), timed actions, order outcome tracking and its events (B7 brings
  events), VMC and shutter commands.
- Optimistic values: the data changes when the device reports its new state.

## Requirements

### R1: The route (core, `api/`)

`POST /api/v1/equipments/:id/orders/:alias`, body `{"value": <v>}` (a missing body or value is an
empty value):

| Case | Answer |
| ---- | ------ |
| Unknown equipment | 404 `{"error":"Equipment not found"}` |
| Equipment disabled | 400 `{"error":"Equipment is disabled"}` |
| No order binding with that alias | 404 `{"error":"Order alias not found: <alias>"}` |
| Value not accepted for the order (R2) | 400 `{"error":"Invalid value for <alias>"}` |
| At least one bound device acknowledged | 200 `{"success":true}` |
| No device acknowledged | 502 `{"error":"<reason>"}`: `device did not answer`, `device refused the command (status 0x<s>)`, `radio busy` |
| Order queue full (R3) | 503 `{"error":"Busy"}` |

### R2: Values and Zigbee commands (core, ZCL)

- `state` (boolean order, `light_toggle`): `true`, `"ON"` or `"on"` → On/Off cluster (0x0006)
  command On (0x01); `false`, `"OFF"` or `"off"` → Off (0x00); `"TOGGLE"` or `"toggle"` → Toggle
  (0x02); an empty value → On (Sowel: an empty boolean value means `true`). The boolean is mapped
  through the order's `valueOn` / `valueOff` (`"ON"` / `"OFF"`) as Sowel's `resolveWireValue`,
  then to the command. Anything else: 400.
- `brightness` (number 0..254, `set_brightness`): Level Control cluster (0x0008) command Move to
  Level with On/Off (0x04), level, transition time 0 (immediate). Out of range or not a
  number: 400.
- Each command is a cluster-specific ZCL frame from the coordinator's endpoint 1 to the device's
  endpoint (the first endpoint of its descriptor), with "disable default response" clear: the
  Default Response (status success) is the acknowledgement. A Default Response with an error
  status is a refusal.
- One retry after 2 s when the device did not answer (Sowel's `dispatchWithRetry`); a sleepy
  device is not a target of orders (no order on a battery device's catalog).

### R3: The flow (host and core)

The HTTP worker never touches the radio. It puts the order (equipment, alias, resolved commands per
device) in a bounded queue (at most 8 orders waiting) with a reply slot, releases the lock, and
waits for the reply, at most 12 s (two attempts with their timeouts). The radio loop takes the
orders one at a time between radio polls, sends the commands, waits for the Default Responses
without blocking the reception of other frames (the measures keep flowing), and answers. Queue
full: 503. A reply that does not come in time: 502 `device did not answer`.

### R4: Values afterwards

The relay reports its new `state` (its reporting was configured at pairing, spec 006): the data,
the equipment binding and the zone aggregation (`lightsOn`) change through the usual path. No value
is set from the order itself.

### R5: The simulator

The simulated relay already answers On, Off and Toggle with a Default Response and a report. A
`dimmer` kind is added only for the tests of `brightness` (Level Control: Move to Level with
On/Off, then reports of level and on/off), not in the shipped scenarios.

### R6: The host

Nothing new on the command line. `--serve` executes orders.

### R7: Precisions (after the spec review; they override R1 to R6 where they differ)

1. **Reaching the device.** The core's `Home` keeps, for each device, its last network address and
   the endpoint its 0x0006 / 0x0008 frames come from (endpoint 1 when unknown), persisted in
   `devices.json` outside Sowel's JSON (`#[serde(default)]`), updated by every message. To send,
   the address comes from that record, else from the coordinator's cache, else from EZSP
   `lookupNodeIdByEui64` (0x0060); still unknown: 502 `device address unknown`.
2. **Timing.** Each attempt waits 5 s for its Default Response; one retry 2 s later (5 + 2 + 5 =
   12 s). The order carries a deadline 12 s after it was queued: an order whose deadline passed, or
   whose waiter is gone, is dropped without being sent (the client is never told "failed" for a
   light that then switches). The devices under one alias are sent in parallel within the same
   12 s; commands in flight at most 8.
3. **Orders waiting**: at most 2 (bound written next to the queue); a 3rd gets 503 `Busy`, so at
   least 2 of the 4 HTTP workers stay free for reads.
4. **Values as Sowel's `resolveWireValue`** (`../sowel/src/shared/order-wire-value.ts`,
   `coerceOnOff`): strings trimmed, case ignored; `true`, `"on"`, `"true"`, `"1"`, `1` → On;
   `false`, `"off"`, `"false"`, `"0"`, `0` → Off; `"toggle"` in any case → Toggle; empty → On;
   anything else 400. Brightness: a number or a numeric string; 255 → 254 (as Zigbee2MQTT);
   not an integer, below 0 or above 255: 400.
5. **State after an order**: Zigbee2MQTT does not configure the ZBMINIR2's reporting
   (`configureReporting: false`) and publishes the state from the command. Here, after a Default
   Response success, the coordinator reads the device's `onOff` (Read Attributes 0x0006/0x0000)
   once (and `currentLevel` for a brightness order); the answer goes through the usual measures
   path. No value is set from the order itself.
6. **Move to Level frame**, as Zigbee2MQTT sends it: `01 <tsn> 04 <level> 00 00 00 00` (level,
   transition 0 as two bytes, options mask 0, options override 0).
7. **Retry scope**: a missing answer or `radio busy` is retried once; an error status is not.
   With several devices failing for different reasons, the 502 gives the last one's (Sowel's
   `lastError`).
8. **Acknowledgement match**: a Default Response matches on sender network address, cluster, TSN
   and the command id it carries (payload byte 0). The TSN counter is the coordinator's
   (shared with pairing).
9. **Orders from the core**: the order queue entry (an `OrderRequest` and an optional reply slot)
   can be pushed by the core too, so B6's `motion-light` uses the same path as HTTP.
10. **Where it runs**: core and host; the firmware does not serve the API, not affected. The real
    demo uses the Dongle Max over TCP with its original firmware, nothing flashed (D6). The
    simulator's "silenced relay" switch and `dimmer` kind are test-only, not in scenario files.
11. Two orders on the same device back to back are served in order.

## Living spec delta

### docs/living-spec/README.md

- "Organises the home in zones and equipments": new bullet "Orders": the route, the values, the
  answers, the retry, the value that follows the device's report.
- "Runs without the dongle": the relay row stays true; the `dimmer` kind is test-only, not
  described.
- History row 011.

### No longer true

- `docs/living-spec/README.md`: "Its orders (`DeviceOrder`)": they are now executable through
  equipments; "resumes the network, listens" (it also sends commands); "503 `Busy` beyond 4
  requests at once" (an order waiting holds one; at most 2 orders wait); the routes table gains
  `POST /api/v1/equipments/:id/orders/:alias`; the closing sentence (orders added to what the
  host does).
- `docs/architecture.md`: "`api/` (... a pure function from request to response)" → it returns an
  order request the host completes through the radio loop.
- `docs/architecture.md`: the order queue between HTTP and the radio loop.
- `docs/roadmap.md`: B5's status.
- Searched: `order`, `light_toggle`, `B5`.

## Acceptance criteria

- [x] With `--sim scenarios/garage.json --serve --speed 10` and the garage of spec 010's demo:
      `POST .../orders/state {"value":true}` → `{"success":true}`; `GET` the equipment: `state`
      true; the aggregation: `lightsOn` 1; `{"value":false}` back to 0.
- [x] Real Dongle Max and ZBMINIR2 (pending): the relay clicks on and off.

## Edge cases

- Equipment disabled; unknown alias; a value of the wrong type or out of range.
- Two devices bound under the same alias, one answers: 200.
- No device answers (sim relay silenced): one retry after 2 s, then 502.
- A Default Response with an error status: 502 with the status.
- A 3rd order waiting: 503; reads still served.
- A relay not heard since the start: its address from `devices.json`, else looked up; unknown: 502
  `device address unknown`.
- An order whose deadline passed in the queue: dropped, not sent.
- Measures keep arriving while an order waits for its answer.
- A device bound to the order was deleted: its binding is gone (spec 010), nothing sent to it.

## Sowel contract

Mirrors Sowel's order route, texts and value rules (files above). Divergences (decision D16): no
order outcome events yet (B7); a 400 for a value Sowel would pass through to its integration; the
12 s deadline; the retry only on a missing answer or a busy radio; brightness 255 → 254; the state
read back after the order instead of an optimistic value.

## Decided in autonomy

- Transition time 0 for brightness (immediate), as Zigbee2MQTT's default without `transition`.
- The acknowledgement is the ZCL Default Response, not the next report.
- 2 orders at most in the system (R7.3), 8 commands in flight at most (R7.2).

## Added while building

1. **The queue lives in `Home`.** `api::handle_stream` pushes the order itself and returns
   `Reply::Order` with a wait handle; the host waits for it without holding the `Home` lock
   (`api::order_response`). The core pushes through `home.orders().push_unattended` (R7.9). The
   plain `api::handle`, which cannot wait, answers 501 `Orders need the streaming handler` to an
   order and queues nothing.
2. **"At most 2" counts the order being sent**: one sending plus one waiting, so a 3rd concurrent
   order gets 503 (R7.3 read as "2 orders in the system").
3. **Two devices under one alias** cannot be built over HTTP (spec 010 allows one order binding
   per alias per equipment): that edge case, and "queued past its deadline", are core tests on the
   runner and the queue, not HTTP tests.
4. **The HTTP waiter waits 12.5 s**, so the radio loop's own answer at the 12 s deadline wins.
5. **Test switches in the simulator**: the `dimmer` kind is accepted in a scenario file (only
   test-written files use it); `silent` and `refuse_commands` are test-only device fields,
   `silent` also kept in the simulator's memory file so host tests can set it between two runs.
6. After the code review: an acknowledgement to any attempt of a command counts, so a late answer
   is never followed by a second send; an attempt (first or retry) is not sent with less than
   4.5 s left before the deadline (`ANSWER_TIMEOUT_MS - SEND_SLACK_MS`), the order then fails with
   its last reason; deadlines are counted on a monotonic clock; a radio error while sending fails
   that order (`radio error: <e>`) instead of stopping `--serve`; POST and PUT need
   `Content-Type: application/json`, else 415 `Unsupported Media Type` (a web page cannot send
   one from the browser).
7. `GET` on `.../orders/:alias` answers 405; address updates from the radio come through a
   separate `take_routes()` stream.

## How the user will check it

After spec 010's demo steps: `curl -X POST http://127.0.0.1:8080/api/v1/equipments/<light>/orders/state
-H 'Content-Type: application/json' -d '{"value":true}'`, then `GET /api/v1/zones/aggregation`:
`lightsOn` 1.

Run on the real hardware (2026-11-21): `POST /api/v1/equipments/:id/orders/state {"value":"ON"}` answered success; the ZBMINIR2 clicked and reported `light_state on`.
