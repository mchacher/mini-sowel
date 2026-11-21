# Sowel's live WebSocket: the reference protocol (summary of Sowel's code, with sources)

Paths relative to the Sowel repository (`../sowel`), read on 2026-11-21. WS = src/api/websocket.ts,
TYPES = src/shared/types.ts, UI = ui/src/store/useWebSocket.ts, DOC = docs/technical/api-reference.md.

## 1) Endpoint, upgrade, auth

- Path `GET /ws`, Fastify `@fastify/websocket` (`ws` library). WS:366. Same port as the HTTP API (DOC:3, DOC:807 says 3000).
- Server registers the plugin with `handleProtocols`: if a requested subprotocol starts with `bearer.`,
  that exact string is echoed back as the accepted subprotocol; otherwise `false` (no subprotocol).
  src/api/server.ts:315-322. A browser drops the connection if the server does not echo the
  subprotocol it asked for, so the Rust server MUST echo `Sec-WebSocket-Protocol: bearer.<token>`.
- Token extraction (WS:24-37), in this order:
  1. `Authorization: Bearer <token>` header (trimmed);
  2. `Sec-WebSocket-Protocol`, comma-separated list, first entry starting `bearer.`, token = rest.
- The `?token=` query documented in DOC:807-809 is NOT read by the server code (extractWsToken has no
  query path; WS:376-379). DOC is stale on this point. The only client that uses `?token=` is
  ui/src/pages/LogsPage.tsx:87, which therefore gets close 4001 (inferred: an existing bug; the Logs
  live view is admin-only and can be ignored).
- The UI always uses the subprotocol: `new WebSocket(url, "bearer."+localStorage.sowel_access_token)`
  (UI:313-316, 524-525). URL is `ws(s)://<window.location.host>/ws`, no query (UI:306-311).
- Upgrade is accepted first; auth failures are reported AFTER the handshake by closing the socket:
  - Origin check first (WS:368-372): missing Origin allowed; `*` in CORS list allowed; Origin in list
    allowed; or Origin host == Host header (WS:48-68). Else `close(4003, "Origin not allowed")`.
  - No token: `close(4001, "Authentication required")` (WS:381-385).
  - Token invalid: `close(4001, "Invalid token")` (WS:390-393). Verification: prefix `swl_`/`wch_`/`cbl_`
    = API token, anything else = JWT access token (src/auth/auth-middleware.ts:60-72).
- HTTP onRequest auth hook skips `/ws` (auth-middleware.ts:201); everything else auth-wise is in WS.
- 127.0.0.1 without auth: NO loopback exemption anywhere (grep of src/auth and server.ts finds none).
  A local client with no token is closed with 4001. A minimal device that has no auth must decide
  itself (inferred): accept the upgrade and ignore the token, but still echo `bearer.*` subprotocol
  if offered, else browsers will drop the socket.
- Role: from token payload, `"admin" | "standard"` (TYPES:1482). Used only for filtering (section 3).

## 2) Messages

### Client -> server (WS:403-425)
Only one message type, text frame JSON:
`{"type":"subscribe","topics":["devices","equipments",...]}`
- Replaces the whole subscription set (not additive): `state.topics = resolveSubscribedTopics(...)`
  (WS:408,419). `system` is always in the set (WS:171-173). Unknown topic names silently ignored
  (WS:174). `logs` and `mqtt-publishers` silently dropped for non-admin (WS:104,175).
- No `unsubscribe` message (subscribing to a smaller list is how to unsubscribe).
- No ping/pong application messages. Anything unparseable or of another `type` is ignored silently
  (WS:422-424). Binary frames: `String(raw)` then JSON.parse (WS:405).
- Valid topics (WS:70-95; exact strings): `devices`, `equipments`, `zones`, `modes`, `recipes`,
  `calendar`, `mqtt-publishers`, `system`, `logs`, `activity`, `energy`. The UI type lists only
  devices, equipments, zones, modes, recipes, calendar, system, activity, energy (UI:18-27).
- Initial state before any subscribe: only `system`.

### Server -> client
1. Welcome, sent once right after accept, single object NOT in an array (WS:439-446):
   `{"type":"connected","message":"Connected to Sowel engine","version":"0.1.0"}`
   The UI ignores it (it has no case for it; handleEvent switch has no `connected`, UI:330+).
2. Event batches: a JSON ARRAY of EngineEvent objects, flat (no envelope, no topic field, no seq/ts
   wrapper), sent every 200 ms per client iff the client's pending queue is non-empty
   (WS:96,297-314). Array may contain one element. Order = emission order after dedup.
3. `activity.added` (DOC:858-880): the DOC says single event, not batched, but the code routes
   `activity.*` through the same getEventTopic/batch path (WS:146-147, 319-326), so it arrives as a
   one-or-more element array like the rest (inferred; the UI handles both because
   `Array.isArray(data) ? data : [data]`, UI:605).
4. `log.entry` objects, single (not array), only for `logs` topic, admin (WS:351): skip.
- Client tolerance: UI accepts a single object or an array (UI:603-607). Safest: always arrays.
- JSON is `JSON.stringify` output: camelCase keys, `undefined` fields omitted (so optional fields
  are absent, not null).

### Batching, dedup (WS:249-289)
At flush, per client, walk the pending list backwards keeping only the LAST event per key, then restore
order. Keys:
- `device.data.updated` -> `d:<deviceId>:<key>`
- `device.status_changed` -> `ds:<deviceId>`
- `device.heartbeat` -> `dh:<deviceId>`
- `equipment.data.changed` -> `e:<equipmentId>:<alias>`
- `equipment.status.changed` -> `es:<equipmentId>`
- `zone.data.changed` -> `z:<zoneId>`
- `energy.arbiter.status` -> `ea:status`; `energy.capacity.budget` -> `eb:<equipmentId>`;
  `energy.ev_session.updated` -> `evs:<session.id>`
- Everything else (created/updated/removed, order.*, recipe.*, mode.*, system.*, activity.*): never
  deduped. Note: dedup keeps the last event's position, so a deduped key moves to its last position.
- No initial snapshot, no replay: a client only sees events after it subscribed. The UI refetches
  everything over REST on every (re)connect (UI:527-545).
- Heartbeats: no WS ping frames or app heartbeat in code (WS has no ping/terminate logic). The
  `device.heartbeat` event is a device-level event, not a keepalive. Library defaults only.
- Close codes used by the server: 4001 (auth), 4003 (origin). Nothing else custom; socket
  `error` handler just drops the client (WS:433-437). No server-initiated close on idle.

## 3) EngineEvent -> topic, filters, fields

Topic = prefix of `type` before first dot (WS:128-153): device->devices, equipment->equipments,
zone->zones, mode->modes, recipe->recipes, calendar->calendar, mqtt-broker|mqtt-publisher->
mqtt-publishers, activity->activity, energy->energy, anything else (system, settings, sunlight,
notification-publisher, shared_access, ...)->system.
So `equipment.order.*`, `equipment.timed_action.*`, `equipment.status.changed` are all topic
`equipments`; `recipe.instance.*` is `recipes`. `settings.changed`, `sunlight.changed`,
`shared_access.*`, `notification-publisher.*`, `system.*` go on `system` (always delivered).

Delivery filter (WS:241-247): an event goes to a client iff topic is in client.topics AND NOT
(admin-only event AND role != admin). Admin-only = topics mqtt-publishers and logs, plus type
prefixes `notification-publisher` and `shared_access` (WS:104,118,161-164).
Redaction for non-admin on top-level string fields (WS:215-238): `system.error.error`,
`system.update.progress.message`, `system.update.error.error` become `"[redacted]"`.
Events are pushed to the queue as the same object (no deep copy); redaction shallow-copies.

No transformation of events otherwise: the bus event is serialised as is. There is no mapping from
`device.data.updated` to `equipment.data.changed`: both exist independently on the bus (the
equipment-manager derives the second from bindings; Sowel's device->equipment logic is not in WS),
and each goes to its own topic. `zone.data.changed` carries the whole aggregate (not a diff).

Exact shapes (TYPES:1819 onward; line numbers = TYPES start 1819 + offset):
- device.discovered `{type, device: Device}` (TYPES:1821)
- device.removed `{type, deviceId, deviceName}`
- device.status_changed `{type, deviceId, deviceName, status: "online"|"offline"|"unknown"}` (TYPES:176)
- device.data.updated `{type, deviceId, deviceName, dataId, key, value: unknown, previous: unknown,
  timestamp: ISO string, sourceTimestamp?: epoch seconds number}` (TYPES:1837-1847). UI reads
  deviceId,key,value,timestamp only (UI:343-345; useDevices.ts:85-100: updates value+lastUpdated of
  the matching key and device.lastSeen; ignored if device not in store).
- device.heartbeat `{type, deviceId, timestamp: ISO}`; UI uses it for lastSeen (UI:346).
- zone.created / zone.updated `{type, zone: Zone}`; zone.removed `{type, zoneId, zoneName}`
- zone.data.changed `{type, zoneId, aggregatedData: ZoneAggregatedData}` (TYPES:283-322+: temperature,
  humidity, luminosity: number|null; motion: bool; motionSensors, motionSince: ISO|null; openDoors,
  openWindows, waterLeak, smoke, lightsOn, lightsTotal, shuttersOpen/Total, averageShutterPosition,
  doorsOpen/Total, waterValvesOpen/Total, waterFlowTotal, powerTotal, sunrise, sunset, isDaylight,
  displaysOnline, ... read the interface to the end for the complete field list). UI stores it
  verbatim per zoneId (useZoneAggregation.ts:33-37).
- equipment.created / equipment.updated `{type, equipment: Equipment}` (UI ignores payload and
  refetches REST, coalesced 1 s: useEquipments.ts:58-61,123-131); equipment.removed
  `{type, equipmentId, equipmentName, zoneId}`
- equipment.data.changed `{type, equipmentId, alias, value, previous, sourceTimestamp?}`; UI patches
  the binding whose `alias` matches (useEquipments.ts:132-146) and marks order timing received.
- equipment.order.executed `{type, equipmentId, orderAlias, value, source?: OrderSource,
  startedAt?: ms, partOf?: string, composite?: true}`; UI uses equipmentId, orderAlias, value
  (UI:389-393).
- equipment.order.failed `{type, equipmentId, orderAlias, value, error: string, source?}`
- equipment.order.unconfirmed `{type, equipmentId, orderAlias, value, reason:
  "timeout"|"device_offline"|"integration_disconnected"|"device_fault", source?}` (+ possibly more
  fields at TYPES ~1900-1910, not shown in my read; check lines 1900-1915)
- equipment.order.outcome `{type, equipmentId, orderAlias, value, status:
  "pending"|"moving"|"confirmed"|"fault"|"unconfirmed"|"stopped", expected?: "open"|"close",
  detail?, source?}` (TYPES:1575-1585). UI: useOrderOutcomes.handleOutcome (UI:385-387).
- equipment.timed_action.armed/reverted/disarmed/failed: fields at TYPES ~1925-1960; UI only
  triggers an equipment refetch (UI:378-383).
- equipment.status.changed `{type, equipmentId, equipmentName, oldStatus, newStatus}`, statuses
  "online"|"degraded"|"offline" (TYPES:527). UI: refetch + optimistic status (UI:394-396;
  useEquipments.ts:177-189).
- recipe.instance.created|removed|started|stopped|state.changed `{type, instanceId, recipeId}`;
  recipe.instance.error adds `error: string` (TYPES:1969-1974). UI ignores payload and refetches
  instance list, coalesced 1 s (UI:397-404; useRecipes.ts:39-42). `recipe.instance.state.changed`
  can fire dozens of times per minute (useRecipes.ts:39).
- OrderSource (TYPES:1550-1560): `{kind:"recipe",instanceId,recipeName,recipeId?}` |
  `{kind:"mode",modeId,modeName}` | `{kind:"manual",userId,userName?}` |
  `{kind:"button",buttonId,buttonLabel?}` | `{kind:"external",channel}` |
  `{kind:"shared_access",accessId,label,pressId?}`.
- mode.created|updated `{type, mode}`, mode.removed|activated|deactivated `{type, modeId, modeName}`;
  calendar.profile.changed `{type, profileId, profileName}`.
- system.* : started `{}`, integration.connected|disconnected `{integrationId}`, error `{error}`,
  alarm.raised `{alarmId, level:"warning"|"error", source, message, messageKey?, messageParams?,
  zoneId?}`, alarm.resolved `{alarmId, source, message, messageKey?, messageParams?, zoneId?}`,
  update.available/progress/error, restart_required `{reason}` (TYPES:2022-2060).
- activity.added `{type, item: {id, timestamp: epoch ms, category:
  "recipe"|"mode"|"motion"|"order"|"sunlight"|"alarm", zoneId: string|null, message: {template,
  params}, source?: OrderSource}}` (TYPES:1806-1813,1768). Server broadcasts all items; client
  filters by zone (DOC:880).

## 4) Back-pressure, limits

- Per client unbounded `pending: EngineEvent[]` between 200 ms flushes (WS:120-126); no cap, no
  slow-consumer detection, no `bufferedAmount` check. If `readyState !== 1` the queue is cleared
  (WS:300-303). Send errors swallowed (WS:310-312).
- Events are queued only if `clients.size > 0` (WS:320).
- No max client count in code (grep for maxClients/maxPayload finds nothing in src). `ws` library
  defaults then apply (inferred: maxPayload 100 MiB, no ping). Fastify global rate limit
  300 req/min applies to HTTP routes, not to WS messages (server.ts:305-308, inferred).
- Role-wise cost: dedup keeps batch size bounded by number of distinct keys per 200 ms.
- A Rust device must add its own caps (inferred): bounded queue per client (e.g. drop oldest
  data events, dedup first), a client cap (2-4), small max inbound frame (e.g. 1 KiB).

## 5) How the UI uses it

- Single global socket, store `useWebSocket`; `connect()` is a no-op if already open/connecting
  (UI:522-523 region). Opened after login with the access token from localStorage.
- Subscription per page via `useWsSubscription(topics)` (ui/src/hooks/useWsSubscription.ts:11-24):
  waits for status "connected", re-sends `subscribe` only when the topic list string changes. The
  list REPLACES the previous one, so each page sends its own full list. System is implicit.
  Pages (grep `useWsSubscription(`): Home `[zones,equipments,modes,recipes,activity]`;
  Dashboard `[equipments,zones,recipes]`; Zones `[zones]`; ZoneDetail `[zones,equipments,recipes]`;
  Equipments `[equipments]`; EquipmentDetail `[equipments,devices,energy]`; Devices
  `[devices,equipments,zones]`; DeviceDetail `[devices]`; Modes/ModeDetail `[modes]`; Calendar
  `[calendar,modes]`; LiveEnergy `[equipments,energy]`.
- On open (UI:527-545): status connected, reset backoff, re-send `currentTopics`, then REST refetch
  of devices, equipments, zones, zone aggregation, recipes, recipe instances, modes, arbiter,
  activity, health (alarm banner), battery alerts, PV health alerts. It expects NO snapshot over WS.
  The welcome message is ignored.
- Reconnect (UI:614-623, 316-321): on close, status disconnected; retry after
  `min(1000*2^attempts, 30000) + random(0..500)` ms; attempts reset on open. Also reconnects on
  `visibilitychange` visible and `online` (UI:287-304).
- Parsing errors ignored (UI:610-612). Unknown event types fall through the switch silently.

## 6) What a minimal implementation can leave out

Needed for the UI to work: `/ws` upgrade with `bearer.*` subprotocol echo; `{"type":"subscribe"}`
handling; arrays of events at <= 200 ms cadence; topics devices, equipments, zones, recipes
(and ideally modes, activity, system).
Can be omitted safely (reasons):
- `connected` welcome: ignored by UI (inferred from absence of case in handleEvent); cheap, keep it.
- `logs`, `mqtt-publishers`, `calendar`, `energy` topics: admin/optional pages; accept the names in
  subscribe and just never send events (unknown/empty topics are harmless).
- Roles, redaction, Origin check (4003): only if the device has no multi-user auth; keep a token
  check or LAN-only assumption (decision for the product, not for the UI).
- Dedup: not needed for correctness of the UI (it applies events in order), but cheap and
  protects memory; implement for device.data.updated and equipment.data.changed and zone.data.changed.
- `device.heartbeat`, `equipment.status.changed` (UI refetches REST on it), `equipment.order.outcome`,
  `equipment.timed_action.*`, `equipment.order.failed|unconfirmed`, `device.discovered`
  (UI addDevice but a refetch on reconnect also works): optional; the UI degrades to REST refetch.
- `equipment.created|updated|removed` and `recipe.instance.*`: payload ignored by the UI, which only
  refetches REST (1 s coalesce), so a minimal `{type,...}` with the documented ids suffices
  but the event must be sent for the UI to notice changes.
- `mode.*`, `settings.changed`, `sunlight.changed`, `shared_access.*`, `notification-publisher.*`,
  energy.*: skip if the feature does not exist.
- Nothing in the UI depends on ping/pong, close codes other than reconnecting on any close, or on
  `version` of the welcome.
- Do keep: per-client topic set replaced by each subscribe; `system` always on; ignoring malformed
  frames instead of closing.

## Doc/code discrepancies found
- DOC:807-809 says auth via `?token=`; code only accepts Authorization header or `bearer.` subprotocol.
- DOC:858 says `activity.added` is pushed unbatched; code batches it with all other events.
