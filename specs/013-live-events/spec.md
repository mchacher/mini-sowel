# Spec 013: live-events

## Context

Increment B7 of the roadmap (split on 2026-11-21: B7 is the live events, B8 the interface served
by the host). Sowel's interface keeps itself up to date through `GET /ws`, a WebSocket that sends,
every 200 ms, the engine events of the topics the page subscribed to. This increment serves the
same WebSocket, so Sowel's interface, and B8's, see a temperature change, a motion or a light
switch as it happens.

The reference protocol, with a source line for every rule, is
[websocket-reference.md](websocket-reference.md) (a summary of `../sowel/src/api/websocket.ts`,
`src/api/server.ts`, the `EngineEvent` union of `src/shared/types.ts` and the UI's
`ui/src/store/useWebSocket.ts`, read on 2026-11-21). **Where this spec says "as the reference",
that file is the requirement.**

Specified in autonomy (no questions); choices under "Decided in autonomy".

## Goals

1. A client connected to `ws://127.0.0.1:<port>/ws` that subscribes to topics receives, as
   Sowel's, arrays of engine events every 200 ms.
2. The events of devices, zones, equipments, orders and recipes are produced with Sowel's types
   and fields.
3. Bounded: clients, their queues and the cost when no one listens.

## Out of scope

- The interface (B8). Authentication and roles (D13: no authentication on 127.0.0.1); redaction.
- Topics with no feature behind them: `modes`, `calendar`, `energy`, `activity`, `logs`,
  `mqtt-publishers` (accepted in a subscribe, never fed).
- The events and fields of R8 (the UI degrades to its REST refetch, reference §6).

## Requirements

### R1: The endpoint (host)

- `GET /ws` with a WebSocket upgrade on the API's port. A `Sec-WebSocket-Protocol` entry starting
  with `bearer.` is echoed back (the browser drops the socket otherwise); the token is not checked
  (D13). The `Host` check of the API applies before the upgrade (403 as today).
- `Origin`: absent, or whose host is `127.0.0.1`, `localhost` or the `Host` header's host:
  accepted; otherwise the socket is closed after the upgrade with code 4003 `Origin not allowed`,
  as the reference §1.
- First message: `{"type":"connected","message":"Connected to Sowel engine","version":"0.1.0"}`
  (a single object, as Sowel).
- At most **4 clients**; a 5th upgrade request: 503 `{"error":"Busy"}` before the upgrade. A
  client holds no HTTP worker: it runs on its own thread, so the 4 HTTP workers stay free.
- Incoming frames larger than 1 KB close the socket (1009). Ping frames are answered (pong);
  a closed or failed socket frees its slot.

### R2: Subscribe (core)

`{"type":"subscribe","topics":[...]}` replaces the client's topic set; `system` is always in it;
unknown names are ignored; any other message, or text that is not JSON, is ignored. The 11 names
of the reference §2 are accepted. Before any subscribe, the set is `system` only.

### R3: Batches (core)

Every 200 ms, a client with events waiting receives one text frame: a JSON array of the events,
flat, in emission order, after the reference's dedup (last event per key `d:<deviceId>:<key>`,
`ds:<deviceId>`, `e:<equipmentId>:<alias>`, `es:<equipmentId>`, `z:<zoneId>`, at its last
position; other events never deduped). No event, no frame.

### R4: Events (core), Sowel's fields (`../sowel/src/shared/types.ts` T:1819-1974)

| Event | When | Topic |
| ----- | ---- | ----- |
| `device.removed` `{deviceId, deviceName}` | `DELETE /devices/:id` | devices |
| `device.status_changed` `{deviceId, deviceName, status}` | a status changes | devices |
| `device.data.updated` `{deviceId, deviceName, dataId, key, value, previous, timestamp}` | every update of a device data | devices |
| `zone.created`, `zone.updated` `{zone}`; `zone.removed` `{zoneId, zoneName}` | zone routes | zones |
| `zone.data.changed` `{zoneId, aggregatedData}` | the aggregate changed (spec 012 R8.2's rule) | zones |
| `equipment.created`, `equipment.updated` `{equipment}`; `equipment.removed` `{equipmentId, equipmentName, zoneId}` | equipment and binding routes | equipments |
| `equipment.data.changed` `{equipmentId, alias, value, previous}` | each binding of an updated device data | equipments |
| `equipment.status.changed` `{equipmentId, equipmentName, oldStatus, newStatus}` | a derived status changes | equipments |
| `equipment.order.executed` `{equipmentId, orderAlias, value, source?}`; `equipment.order.failed` `{equipmentId, orderAlias, value, error, source?}` | an order acknowledged / failed (spec 011), from the API (no `source`) or a recipe (`{"kind":"recipe","instanceId","recipeName","recipeId"}`) | equipments |
| `recipe.instance.created|removed|started|stopped|state.changed` `{instanceId, recipeId}`; `recipe.instance.error` `{instanceId, recipeId, error}` | the recipe engine (spec 012) | recipes |

Each event is serialized once when emitted, and shared by the clients that receive it. With no
client connected, no event is built.

### R5: Bounds and slow clients (core)

Each client's waiting events: at most **256 events and 8 KB** of JSON (R7.3) (after dedup at insertion
for the dedup keys). One past it: the client is closed with code 1013 (try again later); Sowel's
interface reconnects and refetches everything over REST, so it loses nothing. See R7.3 for the memory test.

### R6: Where it runs

Core (topics, dedup, queues, event building) and host (upgrade, frames, threads; a small crate
for the WebSocket frames, `tungstenite`, under D12). The firmware does not serve the API, not
affected. Nothing on the command line.

### R7: Precisions after the spec review (they override R1 to R6 where they differ)

1. **Sockets that wake up.** A client's thread must wake at least every 200 ms to flush, and a
   write must give up after 5 s. `tiny_http`'s upgraded stream cannot set timeouts, so the host
   builds its listener with `socket2` (D12) and gives it a read timeout of 200 ms and a write
   timeout of 5 s, which accepted sockets inherit; a host test proves the inheritance (it fails
   if a client's batch does not arrive). Another mechanism is allowed if the same tests prove it.
   Each client costs two threads (its own and `tiny_http`'s connection thread); the 4 HTTP workers
   are not used.
2. **Dead and slow clients.** A write that times out drops the socket without a close frame and
   frees the slot. The host sends a ping every 30 s; a client from which nothing (pong included)
   arrived for 60 s is dropped. Close 1013 only when the queue overflows while the socket can
   still be written. Tests expect "slot freed within N s".
3. **Memory.** A client's queue is bounded at **256 events and 8 KB**, the per-entry overhead
   (dedup key, slot) counted in the 8 KB. A batch is not built as one string: it is sent as one
   text message in fragments (a frame per queued event, plus the brackets and commas), written
   straight from the queue; if `tungstenite` cannot send fragments, a batch string of at most
   8 KB is allowed and counted. The ceiling test (200 KB, realistic home, spec 012's included)
   adds 1 client with a full queue; spec 012 left it at 186 KB in the worst case, so the margin is
   thin and stated. The worst case of the bounds (4 clients × 8 KB, or 16 KB with batch strings)
   is written next to the bound. An event is built only when a connected client's topics
   include its topic.
4. **`equipment.status.changed`** as Sowel's `src/equipments/equipment-status-tracker.ts`: the
   last status of each equipment (at most 64) is kept from the start, silent the first time an
   equipment is seen; checked after each recompute and every 60 s (staleness changes with time).
5. **`recipe.instance.*`**, as `src/recipes/engine/recipe-manager.ts`: `created` on POST (:345);
   `error` only when the start right after a create fails (:363), not on a failed restore;
   `removed` on DELETE (:390); `started` (:586) and `stopped` (:612) whenever an instance starts or
   stops (a PUT gives `stopped` then `started`); `state.changed` on every write of a state key
   (:718) and on enable or disable of an instance that is not running (:483, :511).
6. **Order events reopen D17's "source not recorded"**: the order queue now carries the source;
   `recipeName` is the recipe's `RecipeInfo.name`, `recipeId` its id; `value` is the value as
   given (the API's raw value, or the recipe's `"ON"`/`"OFF"`); `error` is spec 011's 502 text; an
   order dropped at its deadline gives `equipment.order.failed` with `device did not answer`.
   Decision D18 replaces that point of D17.
7. **No `device.discovered`**: `--pair` and `--serve` do not run together, so no device is
   discovered while clients listen (a divergence; inbox: pairing while serving).
8. **Origin**, as Sowel (`websocket.ts:48-68`): absent, or its `host:port` equals the `Host`
   header, accepted; plus `http://localhost:5173` and `http://127.0.0.1:5173` (the interface's
   development server, B8). Anything else, a malformed one included: closed 4003 `Origin not
   allowed` after the upgrade, as Sowel.
9. **Wrong requests and frames**: `GET /ws` without an upgrade: 426 `{"error":"Upgrade
   Required"}`; `Sec-WebSocket-Version` other than 13: 400 `{"error":"Unsupported WebSocket
   version"}`; another method on `/ws`: 405. No subprotocol offered: none echoed; several: the
   first starting with `bearer.` echoed. A binary frame is read as text (UTF-8, else ignored), as
   Sowel. `topics` not an array: the message ignored; entries that are not strings: ignored.
   Ctrl-C: each client gets close 1001, then the host exits.
10. **Lock order**: home, then hub; client threads take the hub only, never the home (written in
    `serve.rs`'s module comment).
11. **`tungstenite`** with its `handshake` feature (for `derive_accept_key`; it brings `sha1`,
    `data-encoding`, `http`, `httparse`), `max_message_size` and `max_frame_size` 1 KB, small
    read and write buffers.

### R8: Divergences from Sowel (decision D18)

At most 4 clients (503 for a 5th); incoming messages of at most 1 KB (1009); slow clients closed
(1013) or dropped, Sowel's queues being unbounded; pings and the 60 s silence rule; the extra
origins of the development server; the token not checked (no 4001); events not sent:
`device.discovered`, `device.heartbeat`, `equipment.order.outcome|unconfirmed`,
`equipment.timed_action.*`, `system.*`, `mode.*`, `activity.*`; optional fields not sent:
`sourceTimestamp`, `startedAt`.

## Living spec delta

### docs/living-spec/README.md

- "Serves Sowel's device API": a paragraph "Live events": `/ws`, subscribe, the topics fed, the
  200 ms arrays, the dedup, the welcome, the 4-client bound, closes 4003 / 1009 / 1013; the routes
  table gains `GET /ws`.
- History row 013.

### No longer true

- `docs/living-spec/README.md` closing sentence (the host also sends live events); "refused by a
  `Host` check" (an Origin check too, for `/ws`); "503 `Busy` beyond 4 requests" (a 5th live
  client too).
- `docs/decisions.md` D17: "no `recipe.instance.*` events yet" and "the order's source is not
  recorded": D18 replaces both; D16 "no order outcome events yet": D18 says `executed` and
  `failed` are sent.
- `crates/core/src/home/events.rs`: the comment "(and the WebSocket, later)".
- `docs/roadmap.md`: milestone B's paragraph ("live in the interface"), B7 split.
- `docs/architecture.md`: the host's serve block (WebSocket threads), the core (`live/`).
- `docs/decisions.md` D12 (a new small crate): a new decision lists `tungstenite`.
- `docs/roadmap.md`: B7 split; B7's status.
- Searched: `/ws`, `WebSocket`, `B7`, `live`, `tungstenite`, `D16`, `D17`, `Busy`, `Origin`, `later`.

## Acceptance criteria

- [x] `--sim scenarios/garage.json --serve --speed 10`, with spec 010's garage and spec 012's
      instance, and a client (Node 22's built-in `WebSocket`) subscribed to
      `["devices","equipments","zones"]`: the welcome object, then arrays with the SNZB-02's
      `temperature`, the motion sensor's `occupancy`, the garage's `zone.data.changed`, and the
      light's `equipment.data.changed` `state` true at about 6 s, then false at about 44 s after
      the host's start (spec 012's timings at `--speed 10`), with `equipment.order.executed`
      carrying the recipe's `source`.
- Sowel's interface staying live is B8's criterion.

## Edge cases

- Subscribe with unknown topics, an empty list, not JSON, another `type`: as R2.
- Two subscribes: the second replaces the first.
- The wrong requests and frames of R7.9; a peer that vanishes (R7.2); Ctrl-C with clients.
- 5th client: 503. A client gone: its slot freed.
- A slow client past 256 events / 8 KB: closed 1013, the others unaffected.
- An `Origin` from another site: 4003. A frame over 1 KB: 1009.
- 10 temperature reports of one sensor within 200 ms: one event in the array.
- No client: no event built (tested by counting).

## Decided in autonomy

- B7 split into the live events (B7) and the interface (B8): each is a demo of its own.
- The token is echoed, not checked (D13: 127.0.0.1 only, no authentication).
- Slow clients are closed, not fed with gaps: Sowel's UI refetches on reconnect, so a close loses
  nothing, while a gap would leave it wrong.
- `tungstenite` (sync, no async runtime) for the frames, `socket2` for the timeouts: the host has
  no async runtime.
- Dead clients dropped after 60 s of silence, so a laptop going to sleep never holds a slot for
  good (a month without intervention).

## Added while building

1. **A front door instead of listener timeouts** (replaces R7.1's mechanism). On Linux, a read
   timeout on the listener also makes `accept` fail every 200 ms, and `tiny_http` stops on an
   accept error (proved in a Linux container; macOS ignores it). `crates/host/src/front.rs` owns
   the public port and reads each request's head: `/ws` stays in the host, on a socket it owns
   (200 ms read, 5 s write timeouts, a 16 KB send buffer through `socket2`); any other request is
   passed byte for byte to `tiny_http`, listening on a loopback port of its own. Costs: three
   threads per HTTP connection, a request head must arrive within 10 s, a passed-on connection
   closes after 30 s without a byte from the client; `httparse` is a direct dependency.
2. Test-only environment variables `MINI_SOWEL_WS_PING_SECONDS` (30) and
   `MINI_SOWEL_WS_SILENCE_SECONDS` (60). The 5 s write timeout is not proven by a test (loopback
   buffers absorb a trickle); the 1013 overflow path is.
3. `recipe.instance.state.changed` is sent when an engine call changed the state keys (an
   identical rewrite sends nothing, unlike Sowel).
4. A change in a child zone also gives the parent zones' `zone.data.changed` (as Sowel's
   aggregation). `Home::tick` recomputes after 60 s without a recompute (staleness).
5. `equipment.updated` also when a binding is removed; deleting a device sends only
   `device.removed`. Order events also for an order dropped in the queue.
6. Batches are written as fragments of one text message (`Message::Frame`), one queued event at
   a time: no batch string. Close texts: 1001 `Going away`, 1009 `Message too big`, 1013 `Try
   again later`, 4003 `Origin not allowed`.
7. Memory: the ceiling test with 1 client and a full queue: peak 187 KB, worst case 198 KB of
   200 KB (1.1 KB of margin).
8. After the code review: a client's close is answered before the socket is dropped; 1002 on a
   protocol error, 1007 on a text frame that is not UTF-8; an upgrade on any other path than
   `/ws` is a 400 `Upgrade only on /ws`; every answer passed on by the front door carries
   `Connection: close`; events are written in fragments of at most 1 KB straight from the
   queue's text; a client's worst case is about 25 KB (queue 8 KB, one event being written 8 KB,
   buffers), 100 KB for 4 clients, written next to the bounds; host tests kill their hosts on
   failure.
9. Demo on the simulator, 2026-11-21: the welcome at 0.2 s; at 6.0 s the temperature, the
   occupancy and the garage's `zone.data.changed`; at 6.2 s `equipment.order.executed` `ON` with
   the recipe as source and the light's `state` true; at 44.2 s `state` false and `OFF`.

## How the user will check it

```
cargo run -p mini-sowel-host -- --sim scenarios/garage.json --serve --speed 10
node -e "const w=new WebSocket('ws://127.0.0.1:8080/ws','bearer.x');
w.onopen=()=>w.send(JSON.stringify({type:'subscribe',topics:['devices','equipments','zones']}));
w.onmessage=e=>console.log(e.data)"
```

The events scroll as the scenario plays.
