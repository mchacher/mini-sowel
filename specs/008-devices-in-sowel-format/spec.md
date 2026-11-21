# Spec 008: devices-in-sowel-format

## Context

Specs 004 to 007 pair devices and show their measures as lines. Increment B2 makes them Sowel
**devices**: each paired device becomes a Sowel `Device` with its `DeviceData` (one per measure)
and its `DeviceOrder`s, kept on disk, and readable through the same REST API as Sowel's. This is
the contract of decision D3: what mini-Sowel exposes takes Sowel's names, types and JSON, so
`sowel-plugin-mini` (milestone G) and Sowel's interface read it unchanged. IAS zone devices
(motion sensors) are enrolled in the next increment (the user split it out, 2026-11-21).

Sowel files mirrored (summary with sources: `sowel-contract.md` in this folder):
`../sowel/src/shared/types.ts` (`Device` T:185, `DeviceData` T:225, `DeviceOrder` T:236,
`DeviceWithDetails` T:259, `DataCategory` T:7, `OrderCategory` T:106, `DeviceStatus` T:176,
`PowerSource` T:183), `../sowel/src/api/routes/devices.ts` (routes), and the Zigbee2MQTT plugin's
key and category mapping (`../sowel-plugin-zigbee2mqtt/src/z2m-parser.ts`, `PROPERTY_TO_CATEGORY`).

Specified in autonomy (the user approved, 2026-11-21, building milestone B as far as possible
without hardware); choices listed under "Decided in autonomy".

## Goals

1. A paired device is a Sowel device, with data and orders, kept across runs.
2. The host serves Sowel's device routes in Sowel's JSON.
3. Memory ceilings are tested from here.

## Out of scope

- IAS zone enrollment (motion, contact, leak, smoke sensors): the next increment. Until then an
  IAS device gets only its `battery` and `voltage` data.
- Zones and equipments: `zoneId` is `null`; `PUT` accepts `zoneId` only as `null`.
- Executing orders: orders are listed, not sent.
- WebSocket and events; the extra routes (`battery-alerts`, `suggest`, `raw`).
- Authentication (see "Decided in autonomy").

## Requirements

### R1: The device model (core, `crates/core/src/devices/`)

Rust types mirroring Sowel's, serialised with `serde` and Sowel's field names (camelCase; optional
fields omitted when absent; nullable ones `null`):

- `Device`: `id` (UUID v4), `integrationId` `"mini-sowel"`, `sourceDeviceId` and `ieeeAddress`
  the EUI64 as `0x` + 16 lowercase hexadecimal digits (Zigbee2MQTT's form), `name` (default
  `<model> <last 4 hex digits of the EUI64>`, for example `TH01 5678`; `Zigbee <last 4>` when the
  model is unknown), `manufacturer` and `model`
  as the device sent them (omitted when unknown), `zoneId` `null`, `source` `"mini-sowel"`,
  `status`, `powerSource`, `lastSeen`, `createdAt`, `updatedAt`.
- `powerSource`: `"mains"` or `"battery"` from the device announce's capability byte (bit 2,
  "mains powered"); `"unknown"` when no announce was received.
- `status`: `"online"` when `lastSeen` is within 10 minutes (mains) or 25 hours (battery),
  `"offline"` past it (25 hours when the power source is unknown), `"unknown"` before any message
  (Zigbee2MQTT's availability defaults).
- `updatedAt` changes with the name, the data and orders added at pairing, and the status; not
  with `lastSeen` or values (as Sowel).
- `DeviceData`: `id`, `deviceId`, `key`, `type`, `category`, `value`, `unit` (omitted when none),
  `value` and `lastUpdated` (`null` until the first value).
- `DeviceOrder`: `id`, `deviceId`, `key`, `type`, `category`, and `min`, `max`, `valueOn`,
  `valueOff` where they apply.
- `DeviceWithDetails` = `Device` + `data` + `orders`.
- Dates are ISO 8601 UTC with milliseconds (`2026-11-21T09:12:03.120Z`); the clock and the random
  source (for the UUIDs) are given by the caller.

### R2: Data and orders of a device

Created at pairing from the clusters whose reporting was set up (spec 006), plus the IAS zone of
R3; keys are Zigbee2MQTT's property names, as Sowel's Zigbee2MQTT plugin uses them as aliases:

| Cluster | Data key | type | category | unit |
| ------- | -------- | ---- | -------- | ---- |
| 0x0402 | `temperature` | number | temperature | °C |
| 0x0405 | `humidity` | number | humidity | % |
| 0x0403 | `pressure` | number | pressure | hPa |
| 0x0400 | `illuminance` | number | luminosity | lx |
| 0x0406 | `occupancy` | boolean | motion | |
| 0x0006 | `state` | boolean | light_state | |
| 0x0008 | `brightness` | number | light_brightness | |
| 0x0001 (0x0021) | `battery` | number | battery | % |
| 0x0001 (0x0020) | `voltage` | number | voltage | mV (attribute × 100, Zigbee2MQTT's `battery_voltage`) |

`brightness` keeps the device's 0..254 scale (Zigbee2MQTT's), not the percentage shown by
`--listen`. A key reached from two clusters (later: `occupancy` from 0x0406 and from an IAS motion
zone) is one data. Each measure received updates the matching data's `value` and `lastUpdated`, and the
device's `lastSeen`; any message from the device updates `lastSeen`.

Orders: cluster 0x0006 → `{key: "state", type: "boolean", category: "light_toggle", valueOn: "ON",
valueOff: "OFF"}`; cluster 0x0008 → `{key: "brightness", type: "number", category:
"set_brightness", min: 0, max: 254}`.

A device paired again (same EUI64) keeps its id, name and data ids; its data and orders are
completed, never duplicated. A measure from a device that is not in the store (paired before
this spec, or deleted) is ignored: such a device must be paired again.

### R3: Storage

The core defines a `DeviceStore` adapter trait (load all, save all); the host implements it with
`devices.json` in the data folder (temporary file then rename, mode 0600). Writes are limited so
the box's flash lasts (the user's choice, 2026-11-21): a change of definitions (a device paired,
renamed or deleted) is written at once; values and `lastSeen` are written every 10 minutes when
they changed, and on stop: at most 6 value writes an hour, plus one per definition change. Only
the radio loop writes: the HTTP thread changes the registry under its lock and marks it changed;
radio I/O never holds the lock; a poisoned lock is recovered, not unwrapped. At most 64 devices,
16 data and 4 orders each (bounds written next to them); one past the device bound, the device is
not added and `pairing: device store full (64), <eui64> ignored` is shown (distinct from the
32-per-session `pairing: too many devices`). A `devices.json` that does not parse is the error
`devices: <file>: cannot read: <reason>`; the host does not start and never overwrites it. One
process at a time per data folder (`--pair` and `--serve` together on one folder are not
supported).

### R4: The API

`--serve [port]` (default 8080), with `--radio` or `--sim`: resumes the network (never forms one,
as `--listen`), listens, keeps the devices up to date, and serves HTTP on 127.0.0.1 only:

| Route | Answer |
| ----- | ------ |
| `GET /api/v1/devices` | 200, `DeviceWithDetails[]` sorted by `createdAt` |
| `GET /api/v1/devices/:id` | 200 `DeviceWithDetails`; 404 `{"error": "Device not found"}` |
| `PUT /api/v1/devices/:id` body `{"name"?: string, "zoneId"?: null}`, at least one of them | 200 `Device`; 400 `{"error":"Invalid JSON"}`, `{"error":"Unknown field: <f>"}`, `{"error":"Nothing to update"}` (`{}`), `{"error":"Invalid name"}` (empty or over 64 characters), `{"error":"zoneId must be null"}`; 404 |
| `DELETE /api/v1/devices/:id` | 204; 404. The device is forgotten by the host (not removed from the Zigbee network: B-later) |
| Any other path under `/api/v1` | 404 `{"error": "Not found"}` |
| Another method on a known path | 405 `{"error": "Method not allowed"}` |

Bodies over 16 KB: 413 `{"error": "Body too large"}`. Responses with a body are
`application/json; charset=utf-8` (204 has none). Requests whose `Host` header is not
`127.0.0.1:<port>` or `localhost:<port>` get 403 `{"error": "Forbidden host"}` (a browser page
cannot reach the API through DNS rebinding). Ctrl-C stops the HTTP thread (`unblock`), writes the
store, then exits 130. The routing and the JSON live in the core (a pure function from method, path and
body to status and body), so the box reuses them; the host only moves bytes, with the `tiny_http`
crate. `--pair` also updates `devices.json`, so a device paired in one run is served by the next.
The host prints `serving: http://127.0.0.1:8080` after `network:`; the measure lines of `--listen`
are not printed in `--serve`; `--verbose` prints them too.

### R5: Memory ceiling

A core test, alone in its file (`crates/core/tests/memory_ceiling.rs`, one `#[test]`, so the
counting allocator sees only it), builds a realistic home, 50 devices with 6 data and 1 order each
(the framing note's figure), and serialises the device list; the peak memory stays under 200 KB
(the user's choice, 2026-11-21). The measured peak is written in "How the user will check it". The
Mac's 64-bit figure is an upper bound of the ESP32's 32-bit one. The storage bound (64 × 16 × 4)
is tested for behaviour, not for memory.

## Living spec delta

### docs/living-spec/README.md

- New section "Keeps devices, in Sowel's format": what a device is (fields), its data and orders
  per cluster, online/offline, the storage (`devices.json`), the limits (64, 16, 4).
- New section "Serves Sowel's device API": `--serve`, the routes and answers, 127.0.0.1 only, no
  authentication.
- Changed: "it stores no device and has no API; the host keeps one file, the network backup" →
  "the host keeps the devices and the network backup, and serves Sowel's device API".
- History row 008.

### No longer true

- `docs/living-spec/README.md`: the sentence above; "The host keeps nothing about devices between
  runs" → it keeps them in `devices.json`.
- `docs/decisions.md` D12 "it uses `log` only" → a new decision (D13) records the core's `serde`
  and `serde_json` and the host's `tiny_http` (D12 is not edited).
- `docs/architecture.md`: "The box stores no data" stays true (the box does not run this yet);
  Data (`devices.json`), blocks (devices, API).
- The `--listen` lines are unchanged.
- `README.md`, `CLAUDE.md`: `--serve` command (CLAUDE.md is near its limit: one line).
- Searched: `stores no device`, `no API`, `enroll`, `--listen`, `B2`.

## Acceptance criteria

- [ ] `--sim scenarios/garage.json --pair 30 --speed 10`, then `--sim scenarios/garage.json
      --serve --speed 10`: `GET /api/v1/devices` lists three devices in Sowel's JSON (the relay
      `mains` with its `state` data and order, the motion sensor with `battery` and `voltage`);
      the SNZB-02's temperature changes between two calls.
- [ ] A test compares the JSON of a device with an example written from Sowel's `types.ts`
      (field names, casing, enum values).
- [ ] The memory ceiling test passes.
- [ ] Real Dongle Max and SNZB-02 (pending): the same JSON with the real sensor.

## Edge cases

- A measure for a data the device does not have: ignored.
- A device paired twice: one device, same id.
- 65th device: not added. 17th data: not added, logged.
- `devices.json` missing (first run): empty list; not parsable: error, never overwritten.
- `PUT` with an unknown field, an empty name, `zoneId` a string: 400. Body not JSON: 400.
- Body over 16 KB: 413. Port already in use: `serve: cannot listen on 127.0.0.1:<port>: <reason>`,
  exit 1.
- A `Host` header other than 127.0.0.1 or localhost: 403.
- Ctrl-C while serving: the store written, exit 130.
- Status: a battery device silent for 25 h becomes offline; a message brings it back online.

## Sowel contract

Mirrors `Device`, `DeviceData`, `DeviceOrder`, `DeviceWithDetails`, `DeviceStatus`,
`PowerSource`, `DataCategory`, `OrderCategory` (`../sowel/src/shared/types.ts`) and the device
routes (`../sowel/src/api/routes/devices.ts`). Divergences, recorded in decision D13: no
authentication; no `rawExpose`; `zoneId` `null` until zones exist (Sowel puts new devices in its
root zone); `source` and `integrationId` `"mini-sowel"`; the list sorted by `createdAt` (Sowel:
by name; mini-Sowel keeps pairing order, stable when a device is renamed); data and orders in the
order of the R2 table (Sowel: by key); dates ISO 8601 with `T` and milliseconds (Sowel's SQLite
gives `2026-11-21 09:12:03Z`: both parse as ISO); data ids UUID v4 (Sowel: derived from device id
and key); `PUT` stricter (unknown fields, `{}`, empty or long names refused); 405 and 413 with
`{"error"}` bodies (Sowel's Fastify answers 404 and 413 in its own format).

## Decided in autonomy

- No authentication on the Mac: the API listens on 127.0.0.1 only, with a `Host` check; Sowel's
  bearer token comes with the box's commissioning password (milestone E). Decision D13.
- `tiny_http` for the host's HTTP server (the user's choice, 2026-11-21), widening D12 (decision
  D13); the box uses ESP-IDF's server with the same core routing.
- Added while building: the counting allocator is the `peak_alloc` dev-dependency (the workspace
  forbids `unsafe`, which a hand-written allocator needs); a `Measure` carries the device's own
  value (`brightness` 0..254, `voltage` in mV) beside the shown one; the radio reports device
  announces and messages through a bounded activity queue (64) rather than new pairing events; the
  `Host` check is in the core; `PUT` trims the name; `devices.json` is compact JSON, serialised from the registry without a copy (the save path is in the memory test); each HTTP request is handled in its own thread, at most 4 at once (a fifth gets 503 `{"error":"Busy"}`), so a stalled client blocks neither the API nor Ctrl-C; a stop
  writes only if something changed; a failed value write is retried after 10 minutes; a simulated
  scenario replays from its start on each run, so values may go back after a restart under
  `--sim`.
- Memory ceiling on a realistic home, values written every 10 minutes, enrollment split out: the
  user's choices (2026-11-21).
- Data keys as Zigbee2MQTT's properties, so Sowel's aliases match.
- `DELETE` forgets the device on the host only.

## How the user will check it

Run the two `--sim` commands of the first acceptance criterion and open
`http://127.0.0.1:8080/api/v1/devices` in the browser.

Measured memory peak (R5), 2026-11-21 on the Mac (64-bit): 168 707 bytes (164 KB) for 50 devices
× 6 data × 1 order, registry built and list serialised (the JSON body is 88 701 bytes); ceiling
200 KB. A first version measured 384 KB: the model keeps ids as 16 bytes and dates as
milliseconds in memory, written as UUID strings and ISO dates in the JSON, and the API sizes the
JSON buffer exactly.

Run on the real hardware (2026-11-21): `GET /api/v1/devices` listed both devices in Sowel's format, `online`, with the data keys illuminance, occupancy, battery, voltage (sensor) and state (relay). Right after a restart the values are empty until the next report (values are not kept across a restart: milestone H).
