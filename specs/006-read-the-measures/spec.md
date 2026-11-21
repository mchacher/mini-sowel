# Spec 006: read-the-measures

## Context

Spec 005 pairs a device and identifies it. To automate, the host must receive what devices
measure. Zigbee devices send their measures as ZCL attribute reports, but only to a node that
asked for them: the coordinator binds the device's clusters to itself and configures the
reporting (how often, from what change). This is increment A5, the last of milestone A: the
SNZB-02's temperature, humidity and battery shown live, still after a restart of the host.

The measures are read **generically**, from the standard Zigbee clusters, without a device
definition: the A1 report (spec 002) shows this covers 50 % of Zigbee2MQTT's definitions fully
and 21 % more for their main function, the SNZB-02 included.

Specified in autonomy (the user approved, 2026-11-21, building milestones A and B as far as
possible without hardware); choices listed under "Decided in autonomy".

## Goals

1. On pairing, the host sets up the reporting of the standard clusters the device has.
2. The host shows every measure it receives, in units, from any paired device, run after run.
3. All of it tested with the fake radio.

## Out of scope

- Storing measures or devices (B2); history (milestone H).
- IAS zone enrollment (motion and door sensors that need it): B2.
- Sending orders (B4).
- Device definitions beyond the standard clusters (milestone I).

## Requirements

### R1: Generic reading (core, `radio/zcl.rs` and `radio/measures.rs`)

ZCL Report Attributes (command 0x0A) and Read Attributes Response (0x01) records are decoded
for these attributes; any other is ignored:

Measures take the names of Sowel's `DataCategory` (`../sowel/src/shared/types.ts`), so B2 maps
them without translation.

| Cluster | Attribute | Type | Measure (Sowel category) | Unit, scale, shown as |
| ------- | --------- | ---- | ----------------------- | --------------------- |
| 0x0402 temperature | 0x0000 | int16 (0x29) | `temperature` | °C, value / 100, 2 decimals at most; 0x8000 invalid; outside −50..150 °C dropped |
| 0x0405 humidity | 0x0000 | uint16 (0x21) | `humidity` | %, value / 100, 2 decimals at most; 0xFFFF invalid; outside 0..100 % dropped |
| 0x0403 pressure | 0x0000 | int16 (0x29) | `pressure` | hPa, value (kPa × 10), whole; 0x8000 invalid |
| 0x0400 illuminance | 0x0000 | uint16 (0x21) | `luminosity` | lx, 10^((v − 1) / 10000) rounded to a whole lux; 0 is 0 lx; 0xFFFF invalid |
| 0x0406 occupancy | 0x0000 | bitmap8 (0x18) | `motion` | bit 0: true or false |
| 0x0006 on/off | 0x0000 | bool (0x10) | `light_state` | on or off |
| 0x0008 level | 0x0000 | uint8 (0x20) | `light_brightness` | 0..254 → 0..100 %, rounded to a whole % |
| 0x0001 power | 0x0021 | uint8 (0x20) | `battery` | %, value / 2, whole; 0xFF invalid |
| 0x0001 power | 0x0020 | uint8 (0x20) | `voltage` | V, value / 10, 1 decimal; 0xFF invalid |

Shown values drop trailing zeros (19.50 → 19.5).

The IAS zone status change notification (cluster 0x0500, command 0x00 from the device) gives
**alarm** (bit 0 of the zone status) and **tamper** (bit 2).

Records: a Report Attributes record is attribute id, type, value; a Read Attributes Response
record is attribute id, status, then type and value only when the status is 0 (success). A record
of a type other than the table's is skipped using the type's length (every fixed-size ZCL type;
strings 0x41/0x42 with a 1-byte length, 0x43/0x44 with a 2-byte length); a type of unknown
length ends the reading of that frame (the records before it count). An "invalid" or dropped
value gives no measure. A frame gives at most 16 measures (a bound written next to it).

The IAS zone notification is decoded (names `alarm` and `tamper`, mapped to Sowel's categories by
zone type in B2), but devices only send it after enrollment (B2): in this spec only tests use it.

### R2: Setting up the reporting on pairing (core, coordinator)

After a device is identified (spec 005), the coordinator asks for its Simple Descriptor
(ZDO 0x0004) on its first endpoint, to know its input clusters, then, for each of the clusters
of R1 the device has (except IAS zone):

1. a ZDO Bind_req (0x0021): device's EUI64 and endpoint, cluster, to the coordinator's EUI64,
   endpoint 1;
2. a ZCL Configure Reporting (0x06) of the attribute(s) of R1. Each record: direction 0,
   attribute id, type, min and max interval (uint16, seconds), then the reportable change in the
   attribute's own type and size, left out entirely for discrete types (bool, bitmap8):

   | Attribute | Type | Min | Max | Raw change |
   | --------- | ---- | --- | --- | ---------- |
   | temperature | int16 | 30 | 3600 | 20 (0.2 °C) |
   | humidity | uint16 | 30 | 3600 | 100 (1 %) |
   | pressure | int16 | 30 | 3600 | 1 (0.1 kPa) |
   | illuminance | uint16 | 30 | 3600 | 10 (raw units of the log scale) |
   | occupancy | bitmap8 | 0 | 3600 | none |
   | on/off | bool | 0 | 3600 | none |
   | level | uint8 | 1 | 3600 | 1 |
   | battery % | uint8 | 3600 | 62000 | 2 (1 %) |
   | battery voltage | uint8 | 3600 | 62000 | 1 (0.1 V) |

   The answer is a Configure Reporting Response (0x07): either a single status byte 0x00 (every
   attribute accepted), a single failure status byte (the whole command failed: the cluster
   failed), or records of status, direction, attribute for the refused ones; or a
   Default Response (0x0B) whose non-success status means the cluster failed.

The device is sleepy and awake only for a few seconds after joining: these requests start right
after identification, one at a time, with spec 005's timeouts (ZDO 15 s, ZCL 10 s) and one retry.
The first request still unanswered after its retry stops the setup: the clusters not done yet are
marked failed (the device has gone back to sleep). The result is an event **ReportingConfigured**
(EUI64, clusters set up, clusters failed). A failure does not stop the pairing session. While
the session runs, the failed clusters are tried again once, as soon as the device sends any
message (it is awake then). A session closes at most 70 s after the end of the pairing time
(identification and setup in progress).

Answers are matched to requests by TSN and cluster and command (ZDO: TSN and response cluster);
frames that are not the awaited answer, such as the first reports that often arrive between the
Bind and the Configure Reporting Response, are handled as usual (measures shown), not dropped.

### R3: Showing the measures (host)

- During `--pair`, after `device:`, the host shows `reporting: <eui64> temperature, humidity,
  battery` (or `reporting: <eui64> failed: <clusters>`), then every measure received.
- `--listen` (with `--radio`) resumes the network (spec 005, without forming one: a radio with no
  network is the error `radio: no network, pair a device first`) and shows every measure
  received until Ctrl-C:

```
network: resumed, channel 15, PAN 0x1A2B, extended PAN 0xDDDDDDDDDDDDDDDD
listening
0x00124B0012345678 temperature 19.52 °C
0x00124B0012345678 humidity 54.1 %
0x00124B0012345678 battery 100 %
```

The 64-bit address comes from the radio (`lookupEui64ByNodeId`, its address table; on EZSP 14+
the incoming message also carries the sender's EUI64 when the radio knows it; all zeros means
unknown); when it is unknown, the network address is shown (`0x5C3D`): after a restart, a device
whose parent is a router shows its network address until B2 stores devices.

Manufacturer-specific frames (with a manufacturer code) give no measure: their attributes are not
the standard ones.

The coordinator sends a ZCL Default Response to a received unicast ZCL frame that asks for one
(the "disable default response" bit clear), except to a Default Response and to a command that
has its own response: status success for reports, notifications and responses; "unsupported
general command" (0x82) for a global request it does not serve (such as Read Attributes),
"unsupported cluster command" (0x81) for a cluster-specific request; and
once per TSN and sender (frames can arrive twice): direction reversed, TSN echoed, manufacturer
code echoed when present, from the endpoint the frame was addressed to, to the endpoint that sent
it. It is sent once, without waiting for an answer, and does not take the request slot nor count
as busy. Broadcast and group frames get none. Values are shown with the precision the device sends,
trailing zeros removed. Ctrl-C exits 130.

## Living spec delta

### docs/living-spec/README.md

- "Pairs a device": after identification, the reporting of its standard clusters is set up.
- New section "Shows the measures": `--listen`, the measures and units of R1, the lines.
- History row 006.

### No longer true

- `docs/living-spec/README.md`: "identifications in progress finish first, at most 40 s" →
  "identifications and reporting setups in progress finish first, at most 70 s".
- `docs/living-spec/README.md`: "It talks to the radio to identify it, start its network and pair
  devices" → "..., pair devices and receive their measures".
- `README.md`, `CLAUDE.md`: commands → `--listen` added.
- `docs/architecture.md`: the radio block → also the measures.
- Searched: `--pair`, `--listen`, `measure`, `temperature`, `A5`.

## Acceptance criteria

- [ ] Fake radio: a paired fake SNZB-02 has its reporting set up; its reports are shown with
      `--pair` and with `--listen` in a second run.
- [ ] Real Dongle Max and SNZB-02 (pending): temperature, humidity and battery shown after
      pairing; after the host restarts with `--listen`, without pairing again, humidity and
      temperature shown after breathing on the sensor (battery at its next report, within
      62000 s).

## Edge cases

- The device does not answer the Simple Descriptor (after one retry): nothing is set up,
  `reporting: <eui64> failed: no descriptor`; pairing it again starts over (trying every cluster
  would outlast the few seconds a sleepy device stays awake).
- Bind or Configure Reporting refused (status not success) for one cluster: listed as failed,
  the others go on.
- A report from a device never paired in this run: shown with its address.
- An attribute of an unknown type, an invalid value, a truncated record: ignored, no panic.
- A report of several attributes in one frame: one line per measure.
- `--listen` on a radio with no network: error.

## Sowel contract

None yet: B2 maps these measures to Sowel's device data (`../sowel/src/shared/types.ts`).

## Decided in autonomy

- Reporting intervals from Zigbee2MQTT's defaults for the SNZB-02 and the generic extensions.
- No descriptor, no setup: pairing again is the remedy; failed clusters are retried once at the
  device's next message in the same run (spec review question).
- Added while building: `--listen` prints the banner and the radio lines before `network:`, as
  `--pair`; a mixed setup shows `reporting: <eui64> temperature, humidity failed: battery`, none
  qualifying shows `reporting: <eui64> none`; a cluster is set up when at least one of its
  attributes is accepted; no setup when the Basic read got no answer; `--listen` does not follow
  joins or announces; booleans are shown `motion true`, `light_state on`; the 70 s after the
  pairing time is a hard limit.
- Implausible temperatures and humidities dropped, as Zigbee2MQTT does for the SNZB-02 (spec
  review question): an automation never acts on them.
- Measures named after Sowel's `DataCategory`.
- Only the first endpoint is set up (the SNZB-02 has one); devices with measures on endpoint 2 or
  more: out of scope.

## How the user will check it

Pair the SNZB-02 (`--pair`), watch its first measures, stop the host, run `--listen`, breathe on
the sensor: the humidity and temperature rise within a minute (the sensor reports at most every
30 s; battery only every few hours).
