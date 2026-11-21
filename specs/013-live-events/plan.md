# Plan: Spec 013

## Steps

- [x] 1. Living spec updated
- [x] 2. Core types: `EngineEvent` (R4), topics, dedup keys
- [x] 3. Core logic: `LiveHub` (subscribe, queues, bounds, batches); events emitted by the home
- [x] 4. Adapters: WebSocket in the host (`tungstenite`)
- [x] 5. API: `GET /ws`
- [x] 6. Interface: not applicable (B8)
- [x] 7. Host: client threads, 200 ms flush
- [x] 8. Simulator: nothing new
- [x] 9. Tests; each module seen failing once
- [x] 10. Docs, decision D18

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| live | each event of R4 serialized | equals a JSON example written from Sowel's `types.ts` |
| live | topic of each event type (incl. `equipment.order.*`, `recipe.instance.*`) | as the reference §3 |
| live | subscribe `["devices","zones"]`; then `["zones"]` | devices+zones+system; zones+system |
| live | subscribe with unknown names, `[]`, not JSON, `{"type":"other"}` | names ignored; system only; ignored; ignored |
| live | before any subscribe | system only: a device event not queued |
| live | 10 `device.data.updated` of one key, then one of another key | 2 events, the deduped one at its last position |
| live | `equipment.created` twice | both kept (never deduped) |
| live | no client | `emit` builds nothing (a counter stays 0) |
| live | no event waiting | `take_batch` gives nothing |
| live | 257th event; 8 KB passed (overhead counted) | the client marked to close with 1013; another client unaffected |
| live | 5th client | refused |
| live | a client removed | its slot reused |
| events | deleting a device | `device.removed` |
| events | a report | `device.data.updated` and `equipment.data.changed` per binding |
| events | status change of a device and of its equipment | `device.status_changed`; `equipment.status.changed` |
| events | zone create, update, delete; aggregate change | the three zone events; `zone.data.changed` |
| events | equipment create, update, delete, binding added | `equipment.created`, `updated`, `removed`, `updated` |
| events | order from the API acknowledged; failed | `equipment.order.executed` without `source`; `equipment.order.failed` with `error` |
| events | order from a recipe | `source` `{"kind":"recipe",...}` |
| events | recipe instance created, PUT, enable/disable of a stopped one, state key written, removed, start failing right after create, restore failing | `created`+`started`; `stopped`+`started`; `state.changed`; `state.changed`; `removed`; `error`; no `error` |
| events | equipment status: first seen; derived status changes after a recompute; goes stale with time only | silent; `equipment.status.changed`; sent at the 60 s check |
| events | order dropped at its deadline | `equipment.order.failed` `device did not answer` |
| live | no client listening to `devices` (one listening to `zones`) | no device event built |
| memory | realistic home (spec 012's) + 1 client with a full queue, a batch being sent | peak below 200 KB |
| host | connect with `bearer.x` | the subprotocol echoed; the welcome object first |
| host | sim garage with spec 012's instance, subscribed to devices/equipments/zones, silent after subscribing | arrays with temperature, occupancy, `zone.data.changed`; light `state` true at about 6 s, false at about 44 s |
| host | Origin absent; same host:port; `http://localhost:5173`; `http://evil.example`; `http://localhost:3000`; malformed | accepted ×3; closed 4003 ×3 |
| host | `Host: evil.example` | 403 before the upgrade |
| host | 5 clients | the 5th gets 503; one closes, a new one connects |
| host | a 2 KB frame | closed 1009 |
| host | a client that never reads | dropped within 10 s, slot freed; another client still receives; HTTP still answers |
| host | a client silent for 60 s (no pong) | dropped, slot freed |
| host | `GET /ws` without upgrade; version 8; `POST /ws` | 426; 400; 405 |
| host | no subprotocol; `foo, bearer.x` | none echoed; `bearer.x` |
| host | a binary frame with a subscribe | applied |
| host | `topics` a string; `[1,"zones"]` | ignored; zones |
| host | Ctrl-C with a client | close 1001 received |
| host | ping | pong |

## Validation

- [x] `sh scripts/validate.sh` green
- [x] Every row above has its test
- [x] Code review done, findings fixed
