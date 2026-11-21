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
- after identification, the host sets up the device's reporting: it binds each standard cluster
  the device has (temperature, humidity, pressure, illuminance, occupancy, on/off, level, power)
  to itself and asks for reports (`reporting: <address> temperature, humidity, battery`, or
  `reporting: <address> failed: <clusters>`, both parts when some succeed and some fail,
  `reporting: <address> none` when the device has none of them); a failed setup is tried once more when the device
  next speaks, except when the device gave no description of its clusters (`reporting: <address>
  failed: no descriptor`: pair it again); then the measures it sends are shown;
- pairing closes at the end of the time (identifications and reporting setups in progress finish
  first, at most 70 s), or on Ctrl-C (exit 130).

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
`cannot write <path>`. The host keeps nothing about devices between runs.

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

### Runs without the dongle

`--sim <scenario>` replaces `--radio tcp://<address>:6638` in every command (identify, `--pair`,
`--listen`): a simulated radio and simulated devices, speaking the same protocols as the real
ones, play a scenario file; the lines are the same, `radio: sim <scenario>` first.
`--speed <1..100>` runs the scenario's clock faster (the pairing time is not affected).

A scenario (JSON, at most 1 MB) names the radio (EZSP version 13 to 19, stack version, address,
`"network": "none"` or `"formed"`), up to 64 devices and up to 10 000 events: `join` (as soon as
pairing is open) and `report` (measures by Sowel category, allowed per kind), at times such as
`40s` or `1m30s`. Its errors name the file and the line, or the device or event and its field.

| Kind | Identity sent | Sends |
| ---- | ------------- | ----- |
| `temperature-humidity` | eWeLink TH01 (Sonoff SNZB-02) | temperature, humidity, battery; asleep except 10 s after joining or reporting |
| `motion` | eWeLink MS01 (Sonoff SNZB-03) | nothing until enrolled as an alarm device; then `alarm true`, and `alarm false` 60 s later |
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

No device, equipment, zone, mode or recipe exists in the tool: it stores no device and has no
API; the host keeps one file, the network backup. It talks to the radio to identify it, start
its network, pair devices and receive their measures.

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
