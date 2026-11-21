# Living spec

What mini-Sowel does **today**, in business words. The source of truth: the code follows it, and
every change updates it in the same pull request (the "living spec delta" of its spec). Present
tense only: what it does not do is not written here, but in the roadmap.

## Purpose

mini-Sowel is a minimal Sowel in a Zigbee coordinator sold off the shelf, the Sonoff Dongle Max.
It automates an annex of a house (garage, pool house, workshop, outbuilding) or a small home on
its own, without a server, Docker or the cloud, by proposing the recipes itself; when a Sowel
exists, it becomes its extension. It keeps Sowel's concepts (devices, equipments, zones, recipes,
modes) with the same names, types and JSON as Sowel's API.

## Users

- **Its author**, for their own annexes and their family's homes.
- **Sowel users** who flash their own Dongle Max from a public release.

They pair Zigbee devices, choose the room, accept the proposed recipes, and look at the box from
its web interface or from Sowel.

## What the tool does

### Starts and shows its name

Each of the three programs starts and says which tool it is, with its version:

- **On the Mac**: `cargo run -p mini-sowel-host`, without `--radio` or `--sim`, prints `mini-sowel 0.1.0`
  and exits.
- **On the ESP32**: the firmware starts ESP-IDF and writes `mini-sowel 0.1.0` to the serial log,
  then its free memory: internal RAM (free, and largest free block) and PSRAM (free, of total),
  for example `memory: internal 245 KB free (largest block 110 KB), psram 2039 KB free of
  2048 KB`, or `psram none` without PSRAM. It runs on chips of revision 3 and later, like the
  Dongle Max's.
- **On the Mac, in an emulated ESP32**: `sh scripts/qemu.sh` boots the same firmware in
  Espressif's QEMU, with 2 MB of PSRAM, and prints its banner and memory line; it fails, with the
  last lines of the log, when the firmware panics, reboots, or does not show its memory within
  60 seconds. `sh scripts/validate.sh` runs it.
- **In the browser**: the interface opens on Sowel's layout: the sidebar with the Sowel logo and
  the name MINI-SOWEL, and a page titled `mini-sowel`. It follows Sowel's light theme (Hybrid).

### Talks to the radio

`cargo run -p mini-sowel-host -- --radio tcp://<address>:6638` connects to the radio of a Dongle
Max that runs its original firmware (its ESP32 relays the radio over TCP, port 6638, one client at
a time). It resets the radio, speaks Silicon Labs' ASH and EZSP protocols with it, and shows:

```
radio: tcp://192.168.1.50:6638
ezsp: protocol 13, stack type 2, EmberZNet 7.4.4.0
radio address: 0x8C6FB9FFFE123456
network: no network
```

The network state is one of: no network, joining, joined, joined with no parent, leaving, or
`unknown (<n>)`. It accepts radios speaking EZSP 13 to 19 (EmberZNet 7.2 and later). On a
failure it prints one line on the error output and exits 1: `radio: cannot connect to <address>:
<reason>`, `radio: no answer to reset`, `radio: no answer to command`, `radio: link lost`,
`radio: connection closed by the radio`,
`radio: error 0x<code>`, or the EZSP error (a version outside 13 to 19 is named); a malformed
`--radio` or `--sim` prints the expected form and exits 2.

### Pairs a device

`cargo run -p mini-sowel-host -- --radio tcp://<address>:6638 --pair [seconds]` (180 by default,
1 to 254) starts the radio's Zigbee network and opens pairing:

- the first time, the radio **forms** a network on channel 15, with random keys and identifiers,
  and saves it in its own flash; the host writes a backup of it (identifiers and keys) to
  `mini-sowel-data/network.json` (`mini-sowel-data/sim/network.json` under `--sim`; `--data
  <dir>` for another folder), readable by its owner only,
  never printed;
- after that, the radio **resumes** its network; a coordinator network another program made is
  used as it is, never destroyed, and named so;
- a device that joins is shown with its address, then with the manufacturer and model it sends
  (an original Sonoff SNZB-02 sends `eWeLink TH01`); `device: <address> unknown unknown` when it
  does not answer, `device: <address> not identified (radio busy)` when the radio stays busy;
  beyond 32 devices in one session, `pairing: too many devices, <address> ignored`; a device
  already in the network that announces itself again is shown again;
- an alarm device (IAS zone) is enrolled right after its identification, while it is awake: the
  host reads its zone, writes its own address into it and answers its enrollment, as Zigbee2MQTT
  does (`, enrolled (motion)`, `, enrollment unconfirmed (motion)` or `, enrollment failed` ends
  its `reporting:` line); a failed or unconfirmed enrollment is tried once more when the device
  next speaks; enrollment requests are answered in every mode;
- after identification, the host sets up the device's reporting: it binds each standard cluster
  the device has (temperature, humidity, pressure, illuminance, occupancy, on/off, level, power)
  to itself and asks for reports (`reporting: <address> temperature, humidity, battery`, or
  `reporting: <address> failed: <clusters>`, both parts when some succeed and some fail,
  `reporting: <address> none` when the device has none of them); a failed setup is tried once more when the device
  next speaks, except when the device gave no description of its clusters (`reporting: <address>
  failed: no descriptor`: pair it again); then the measures it sends are shown;
- pairing closes at the end of the time (identifications, enrollments and reporting setups in
  progress finish first, at most 70 s; a setup still running then shows `reporting: <address> cut
  short`), or on Ctrl-C (exit 130).

With `--pair`, the identification's `network: <state>` line is replaced by the `network:` line
below; `network: resumed (made by another program), ...` for a network the host has no backup
of. When a network is formed, the backup is written as soon as the radio accepts it, before the
steps after it; a backup of another network already in the folder is kept, renamed
`network-<PAN id>.json`. Warnings (a refused setting, a dropped callback) go to the error output.

```
network: formed, channel 15, PAN 0x1A2B, extended PAN 0xDDDDDDDDDDDDDDDD
pairing: open for 180 s
joined: 0x00124B0012345678 (0x5C3D)
device: 0x00124B0012345678 eWeLink TH01
pairing: closed
```

New errors: `radio: network did not start`, `radio: the radio is not a coordinator`,
`radio: the radio refused <command> (status 0x<code>)`, `no random source: <reason>`,
`cannot write <path>`. Devices are kept as described below.

### Shows the measures

`cargo run -p mini-sowel-host -- --radio tcp://<address>:6638 --listen` resumes the radio's
network (never forms one: `radio: no network, pair a device first`) and shows every measure the
paired devices send, until Ctrl-C (exit 130):

```
network: resumed, channel 15, PAN 0x1A2B, extended PAN 0xDDDDDDDDDDDDDDDD
listening
0x00124B0012345678 temperature 19.52 °C
0x00124B0012345678 humidity 54.1 %
0x00124B0012345678 battery 100 %
```

Measures are read from the standard Zigbee clusters, without a device definition, and named as
in Sowel: `temperature` (°C), `humidity` (%), `pressure` (hPa), `luminosity` (lx), `motion`,
`light_state`, `light_brightness` (%), `battery` (%), `voltage` (V), and `alarm`, `tamper`
(IAS zone notifications, sent by enrolled alarm devices); booleans are shown
`motion true`, `light_state on`. The banner and the radio's lines come first, as with `--pair`.
Invalid values and
implausible ones (temperature outside −50 to 150 °C, humidity outside 0 to 100 %) are not shown.
A device is shown by its 64-bit address, or by its network address when the radio does not know
it. The host answers each report as Zigbee requires (a default response). A sensor reports at most
every 30 s and at least every hour; its battery every few hours.

### Keeps devices, in Sowel's format

A paired device becomes a Sowel device (`Device` of Sowel's `types.ts`): `id` (UUID), `name`
(`TH01 5678` by default: the model and the end of the address; `Zigbee 5678` without a model),
`manufacturer` and `model` as sent, `ieeeAddress` and `sourceDeviceId` (`0x` + 16 lowercase hex
digits), `source` and `integrationId` `mini-sowel`, `zoneId` (its zone, `null` until placed), `powerSource` (`mains`,
`battery` or `unknown`, from the device's announce), `status` (`online`; `offline` after 10
minutes of silence for a mains device, 25 hours otherwise; `unknown` before any message),
`lastSeen`, `createdAt`, `updatedAt` (ISO 8601 with milliseconds).

Its data (`DeviceData`), one per measure, are named as Sowel's Zigbee2MQTT plugin names them:
`temperature` (°C), `humidity` (%), `pressure` (hPa), `illuminance` (luminosity, lx), `occupancy`
(motion), `state` (light_state), `brightness` (0 to 254), `battery` (%), `voltage` (mV); each with
its last `value` and `lastUpdated`. Its orders (`DeviceOrder`): `state` (light_toggle, `ON` /
`OFF`) for an on/off device, `brightness` (set_brightness, 0 to 254) for a dimmable one. A device
paired again keeps its id and name. Alarm devices (IAS zone: motion, contact, water leak, smoke sensors) also get the data of their
zone: `occupancy` (motion), `contact` (contact_door, true when closed), `water_leak`, `smoke`, and
`tamper` and `battery_low` (generic) once the device first reports them. Alarm devices paired
before this was possible keep only their battery data until paired again.

The host keeps them in `devices.json` in its data folder (owner-only): a device paired, renamed
or deleted is written at once, the latest values every 10 minutes and when the host stops, so a
restart keeps them. At most 64 devices, 16 data and 4 orders each; beyond: `pairing: device store
full (64), <address> ignored`. A `devices.json` that cannot be read stops the host
(`devices: <file>: cannot read: <reason>`), never overwritten. One host at a time per data folder.

### Serves Sowel's device API

`--serve [port]` (8080 by default), with `--radio` or `--sim`, resumes the network, listens, keeps
the devices up to date, sends the orders it receives, and serves on `127.0.0.1` only, without authentication (a browser page
from another site is refused by a `Host` check):

| Route | Answer |
| ----- | ------ |
| `GET /api/v1/devices` | the devices with their data and orders, in pairing order |
| `GET /api/v1/devices/:id` | one device; 404 `{"error":"Device not found"}` |
| `PUT /api/v1/devices/:id` `{"name"?, "zoneId"?}` | renames it (1 to 64 characters) and/or places it in a zone (an id, or `null`; 404 `Zone not found`) |
| `DELETE /api/v1/devices/:id` | 204; the host forgets it (it stays in the Zigbee network) |
| `POST /api/v1/equipments/:id/orders/:alias` `{"value": ...}` | sends the order to the bound devices (see "Organises the home") |
| `/api/v1/recipes`, `/api/v1/recipe-instances` | the recipe and its instances (see "Runs the motion-light recipe") |

Errors are `{"error": "<message>"}` (400, 403, 404, 405, 413, 415; 502 for an order no device took; 503 `Busy` beyond 4 requests at
once, or beyond 2 orders from the API at once). A request with a body is refused with 415 unless it is sent as
`application/json`, so a web page cannot send one from the browser. The lists of devices and equipments are sent in chunks (HTTP chunked transfer), so
their size never weighs on memory; the files are written the same way. It prints `serving:
http://127.0.0.1:<port>`; `--verbose` also prints the measure lines. `--pair` adds the devices to
the same file. Ctrl-C writes the store and exits 130.

### Organises the home in zones and equipments

As in Sowel, nothing acts on a device directly: a **zone** holds **equipments**, and an equipment
binds device data and orders under **aliases**.

- **Zones**: a tree under the root zone `Maison` (id `00000000-0000-0000-0000-000000000001`,
  always present). `GET/POST /api/v1/zones`, `GET/PUT/DELETE /api/v1/zones/:id` with Sowel's
  rules and messages: the root cannot be deleted, nor a zone with child zones or equipments; a
  zone cannot move under itself or a descendant. Deleting a zone returns its devices to no zone.
  At most 32 zones (the root included). A device is placed with `PUT /api/v1/devices/:id
  {"zoneId": ...}`.
- **Equipments**: types `light_onoff`, `light_dimmable`, `shutter`, `sensor`, `switch`,
  `thermostat`, `button`. `POST /api/v1/equipments {name, type, zoneId, deviceIds}` binds every
  data and order of the given devices under their key as alias (`temperature`, `state`...);
  bindings are added or removed one by one under `.../data-bindings` and `.../order-bindings`.
  Each equipment shows its bindings with their values, `lastChanged`, `stale` (a temperature,
  humidity, pressure or luminosity silent for 65 minutes), and its `status`: `online`,
  `degraded` (a device offline among several, or a binding stale) or `offline` (all its devices
  offline, or no binding), with the reason. At most 64 equipments, 16 bindings of each kind.
- **Orders**: `POST /api/v1/equipments/:id/orders/:alias {"value": ...}` sends the order bound
  under that alias to its devices, as Sowel: `state` takes `true`/`false`, `"on"`/`"off"`,
  `"true"`/`"false"`, `1`/`0` (any case, spaces ignored; empty means on) or `"toggle"`;
  `brightness` takes 0 to 254 (255 counts as 254). Answers: `{"success":true}` when a device
  acknowledged; 502 with the reason (`device did not answer`, `device refused the command
  (status 0x..)`, `radio busy`, `device address unknown`) otherwise; 404 `Equipment not found`,
  400 `Equipment is disabled`, 404 `Order alias not found: <alias>`, 400 `Invalid value for
  <alias>`. A device that does not answer is tried once more 2 s later; the answer comes within
  12 s; no attempt is sent with less than 4.5 s left, so an order that could not start in time
  is dropped, never sent late, and a late answer to the first attempt is never followed by a
  second one. The new state is
  read back from the device, then shows in its data, its equipment and the zone aggregation.
  Each device's last network address and endpoint are kept, so a relay is reachable after a
  restart before it speaks. Orders also come from recipes (below): those are never waited for,
  have their own bound (32), and an order from the API is sent before them.
- **Zone aggregation** (`GET /api/v1/zones/aggregation`), as Sowel computes it, over each zone and
  its sub-zones: average `temperature` (1 decimal), `humidity`, `luminosity`; `motion`, the
  number of motion sensors and `motionSince`; open doors and windows; water leak, smoke; lights
  on and in total; shutters; equipments unavailable by category. It is recomputed after every batch of measures, status change and edit.

Zones and equipments are kept in `zones.json` and `equipments.json` beside `devices.json`, written
at once when they change; on load, references to things that no longer exist are dropped or moved
under the root. A `zones.json` or `equipments.json` that cannot be read stops the host, like
`devices.json`.

### Runs the motion-light recipe

The host runs Sowel's `motion-light` recipe (the same id, settings, messages and behaviour as
Sowel's 1.1.2), through Sowel's routes:

- **What it does**: when motion is seen in its zone, it switches its lights on (`light_onoff`
  equipments of that zone); after the timeout (`10m` by default) with no motion, it switches them
  off. With a lux threshold, it does not switch on above it, and switches off above it plus 10 %.
  A light switched off by hand while motion lasts, or on by hand above the threshold, puts it in
  override until the zone has been still for the timeout; a light switched on by hand with no
  motion goes off after the timeout. The safety auto-off forces the lights off after its duration
  without fresh motion. A button press switches the lights off (override) or on. "Inactive during
  day" has no effect: the tool does not know the sun's times (the root zone's `isDaylight` is
  `null`, night). At every start it sets the lights to what the zone calls for (off with no
  motion).
- **Routes**: `GET /api/v1/recipes`, `GET /api/v1/recipes/:id`; `GET`/`POST
  /api/v1/recipe-instances`, `PUT`/`DELETE /api/v1/recipe-instances/:id`, `POST .../enable`,
  `.../disable`, `.../actions`, `GET .../log?limit=`; texts and statuses as Sowel's (`Invalid
  params: <reason>`, `Recipe not found: <id>`, `Instance not found`, `Instance not running`,
  `Invalid body`).
- **Its log**: the last 50 lines of each instance, newest first, each message at most 200 bytes;
  its state (`overrideMode`, `timerExpiresAt`, `failsafeExpiresAt`) is shown with the instance.
- **What survives a restart**: the instances, in `recipes.json` beside the other files, written at
  once; each enabled one is started again (one that no longer validates logs `Failed to restore:
  <reason>` and stays stopped). Its log and state start empty. A `recipes.json` that cannot be
  read stops the host; an entry beyond 16, or with an id already seen, is dropped.
- **Bounds**: 16 instances, 16 lights and 16 buttons each, settings of at most 2 KB, 128 events
  waiting for the recipes.

### Runs without the dongle

`--sim <scenario>` replaces `--radio tcp://<address>:6638` in every command (identify, `--pair`,
`--listen`, `--serve`): a simulated radio and simulated devices, speaking the same protocols as the real
ones, play a scenario file; the lines are the same, `radio: sim <scenario>` first.
`--speed <1..100>` runs the scenario's clock faster (the pairing time is not affected).

A scenario (JSON, at most 1 MB) names the radio (EZSP version 13 to 19, stack version, address,
`"network": "none"` or `"formed"`), up to 64 devices and up to 10 000 events: `join` (as soon as
pairing is open) and `report` (measures by Sowel category, allowed per kind), at times such as
`40s` or `1m30s`. Its errors name the file and the line, or the device or event and its field.

| Kind | Identity sent | Sends |
| ---- | ------------- | ----- |
| `temperature-humidity` | eWeLink TH01 (Sonoff SNZB-02) | temperature, humidity, battery; asleep except 10 s after joining or reporting |
| `motion` | eWeLink MS01 (Sonoff SNZB-03) | enrolled at pairing; then `alarm true` on motion, and `alarm false` 60 s of scenario time later |
| `relay` | SONOFF ZBMINIR2 | `light_state on` / `off` after an On, Off or Toggle command |

The simulated radio keeps its network and its devices in `mini-sowel-data/sim/` (or `--data`),
apart from the real dongle's backup, so a second run resumes. Shipped scenarios:
`scenarios/snzb02.json` (one sensor) and `scenarios/garage.json` (a sensor, a motion sensor, a
relay).

### Measures the coverage of Zigbee2MQTT's definitions

A tool on the Mac, `tools/z2m-coverage` (`npm run report` in its folder), loads the pinned version
of Zigbee2MQTT's device library (`zigbee-herdsman-converters`) and writes
`tools/z2m-coverage/report.md`. For each leading Zigbee brand (Philips Hue, IKEA, Aqara, Tuya and
its white labels, Sonoff, Schneider Electric, Legrand and others), the report gives:

- the share of definitions the box can reuse by export: **declarative** (data and calls to the
  library's shared code) and **Tuya tables** (datapoint tables read by the shared Tuya
  interpreter);
- the share that needs code: **brand code** (a manufacturer's library file) or **device code**
  (written in the device's own file), and among it the share whose code only binds the device and
  sets up its reporting;
- the share a generic reading of the standard Zigbee clusters handles (temperature, humidity,
  on/off, level, colour, occupancy, alarms, battery, covers, thermostat, metering), in full, or
  for its main function with only the manufacturer's extras missed;
- the number of shared functions of the library the box needs to cover the brand.

It also gives totals, the line of a few devices of interest (the Sonoff SNZB-02 among them), and
how the measure is made and what it cannot see. Nothing of the tool runs in the box.

No mode exists in the tool, and `motion-light` is its only recipe; the host keeps the devices,
zones, equipments, recipe instances and the network backup, serves Sowel's device, zone,
equipment and recipe API (above), executes equipment orders, runs the recipe instances,
and talks to the radio to identify it, start its network, pair devices and receive their
measures.

## History

| Spec | Change |
| ---- | ------ |
| 001 | Walking skeleton: the tool starts and shows its name |
| 002 | Measures the coverage of Zigbee2MQTT's definitions for the leading brands |
| 003 | The firmware shows its free memory and boots in QEMU on the Mac |
| 004 | The host identifies the Dongle Max's radio over TCP (ASH, EZSP) |
| 005 | The radio runs a Zigbee network; the host pairs a device and shows what it is |
| 006 | The host sets up reporting and shows the measures of the standard clusters (`--listen`) |
| 007 | A simulated radio and devices play scenario files (`--sim`) |
| 008 | Devices kept in Sowel's format and served by Sowel's device API (`--serve`) |
| 009 | Alarm sensors enrolled; their motion, contact, leak or smoke becomes device data |
| 010 | Zones, equipments and zone aggregation, with Sowel's routes |
| 011 | Equipment orders sent to the Zigbee devices (`POST .../orders/:alias`) |
| 012 | The `motion-light` recipe, with Sowel's recipe routes |
