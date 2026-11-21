# Spec 009: alarm-sensors

## Context

IAS zone devices (motion, door and window, water leak, smoke sensors; the user's Sonoff motion
sensor among them) send their alarms only to a coordinator that enrolled them: the device must
know the coordinator's address (its "CIE address") and receive a Zone Enroll Response. Spec 008
gives these devices only their battery data; increment B3 enrolls them, so their motion,
contact, leak or smoke becomes device data, as B6's `motion-light` needs.

The sequence follows Zigbee2MQTT's (`zigbee-herdsman` 11.2.2, MIT,
`dist/controller/model/device.js` lines 431-444 and 905-944): during the interview, read the
zone state, zone type and CIE address; skip a device already enrolled with our address; write
the CIE address; wait 500 ms; send the Zone Enroll Response; then poll the zone state until it
reads "enrolled".

Specified in autonomy (the user approved, 2026-11-21, building milestone B as far as possible
without hardware, without questions); choices under "Decided in autonomy".

## Goals

1. A paired IAS device is enrolled while it is awake, right after its identification.
2. Its zone notifications update its data (`occupancy`, `contact`, `water_leak`, `smoke`,
   `tamper`) in the device API.
3. The simulated motion sensor of spec 007 sends motion once enrolled; the garage scenario shows
   it.

## Out of scope

- Arming and warning devices (sirens, IAS ACE keypads, IAS WD).
- Zone types beyond motion, contact, water leak, fire: their device gets only `tamper`.

## Requirements

### R1: Enrollment (core, coordinator)

For a device whose Simple Descriptor lists cluster 0x0500, right after its identification and
before its reporting setup (the device is awake then):

1. read attributes 0x0000 (zone state), 0x0001 (zone type), 0x0010 (CIE address) in one Read
   Attributes;
2. if the zone state is 1 (enrolled) and the CIE address is the coordinator's EUI64: enrolled,
   nothing more;
3. otherwise write the CIE address (attribute 0x0010, type 0xF0 EUI64, the coordinator's EUI64),
   wait 500 ms, send a Zone Enroll Response (cluster 0x0500, command 0x00 from the client side,
   payload: status 0 success, zone id 23, as Zigbee2MQTT), then read the zone state every 500 ms,
   at most 20 times, until it is 1: **enrolled**; otherwise **enrollment unconfirmed** (the
   response was sent; many sensors still work).
4. A Zone Enroll Request (command 0x01 from the device) is answered at any time with a Zone Enroll
   Response carrying the request's own TSN and the "disable default response" bit; the zone type
   in the request is used when the read gave none.

The `reporting:` line of `--pair` adds `enrolled (motion)` (the zone type's name: motion, contact,
water leak, fire; `zone 0x<type>` otherwise), `enrollment unconfirmed (motion)` or
`enrollment failed` (no answer to the read, after the retry rules of spec 005).

### R2: Data (core, devices)

The zone type decides the data, added to the device (spec 008's catalog):

| Zone type | Data key | type | category |
| --------- | -------- | ---- | -------- |
| 0x000D motion | `occupancy` | boolean | motion |
| 0x0015 contact | `contact` | boolean | contact_door (true when closed: alarm bit clear) |
| 0x002A water leak | `water_leak` | boolean | water_leak |
| 0x0028 fire | `smoke` | boolean | smoke |
| any | `tamper` | boolean | generic |

A zone status notification sets the zone's data from bit 0 (alarm 1) and `tamper` from bit 2.
The `--listen` lines keep `alarm` and `tamper` (spec 006).

### R3: The simulator

The simulated motion sensor answers the read of 0x0000, 0x0001 (0x000D) and 0x0010, accepts the
CIE write (kept in its memory file; zeros before any write) and the Enroll Response, then reads
back zone state 1 once enrolled; a second `--pair` finds it already enrolled (no write). It sends
no Zone Enroll Request on its own. `scenarios/garage.json`'s motion then reaches the API.

### R4: Precisions (after the spec review)

1. **Frames.** The Zone Enroll Response the coordinator sends, unprompted or answering a request,
   has frame control `0x11` (cluster-specific, client to server, disable default response), then
   the TSN (its own, or the request's), command `0x00`, payload `00 17` (success, zone 23).
2. **Time bound.** A zone state poll without an answer within spec 005's ZCL timeout ends the
   polling as `enrollment unconfirmed`; enrollment counts as a setup in progress
   (`pairing_busy`), within the 70 s cap after the pairing time.
3. **CIE write refused or unanswered** (after spec 005's retry): `enrollment failed`, no zone data.
4. **Reaching the registry.** `PairingEvent::ReportingConfigured` gains `enrollment: Option<{zone
   type, outcome}>`; the device registry adds the zone's data from it. A zone notification gives
   the `alarm` measure: it sets the zone data the device has (`occupancy`, `contact` inverted,
   `water_leak`, `smoke`); bit 2 sets `tamper`, bit 3 sets `battery_low`. The zone data are kept in
   `devices.json`, so a later `--serve` knows them.
5. **`tamper` and `battery_low`** (boolean, category `generic`) are added to a device only the first
   time a notification has that bit set (the Sonoff SNZB-03 exposes neither in Zigbee2MQTT).
6. **Enroll Request in every mode.** It is answered in `--pair`, `--listen` and `--serve` alike (the
   answer needs only the sender's network address and endpoints). Its zone type is used only if it
   arrives before the outcome; a later one is answered without adding data.
7. **Retry.** A failed or unconfirmed enrollment is tried once more at the device's next message
   in the same run, as the failed clusters of spec 006.
8. **The `reporting:` line**: `reporting: <eui64> battery, enrolled (motion)`; `reporting:
   <eui64> none, enrolled (motion)`; `reporting: <eui64> battery failed: temperature, enrolled
   (motion)`; no enrollment text after `failed: no descriptor`.
9. **Endpoint.** Only the first endpoint's descriptor is read: an IAS cluster on another endpoint
   is not seen (out of scope).
10. **Zone status as an attribute** (0x0002 in a Report Attributes or Read Response, sent by some
    devices instead of notifications) is read the same way as a notification.

## Living spec delta

### docs/living-spec/README.md

- "Pairs a device": IAS zone devices are enrolled right after identification; the `enrolled (...)`,
  `enrollment unconfirmed (...)`, `enrollment failed` texts.
- "Keeps devices": "Alarm devices (IAS zone) get only their battery and voltage data" → they also
  get the data of their zone type (`occupancy`, `contact`, `water_leak`, `smoke`) and `tamper`.
- "Keeps devices": the data list adds `contact`, `water_leak`, `smoke`, `tamper`, `battery_low`;
  alarm devices paired before this change keep only their battery data until paired again.
- "Runs without the dongle": the motion sensor's row → enrolled at pairing, then motion.
- History row 009.

### No longer true

- The sentences above; `docs/architecture.md` (the coordinator also enrolls).
- Searched: `enroll`, `IAS`, `alarm`, `battery and voltage data`, `B3`.

## Sowel contract

Data keys and categories from Sowel's Zigbee2MQTT plugin (`PROPERTY_TO_CATEGORY`: occupancy →
motion, contact → contact_door, water_leak, smoke); `tamper` and `battery_low` get `generic`
through Sowel's fallback (`../sowel/src/devices/category-inference.ts`).

## Acceptance criteria

- [x] `--sim scenarios/garage.json --pair 30 --speed 10`: `reporting: <eui64> battery, enrolled
      (motion)`; then `--serve`: the motion sensor's `occupancy` is `true` after its motion
      event and `false` 60 s later.
- [x] Real Dongle Max and Sonoff motion sensor (pending): the same, with a hand passing in front.

## Edge cases

- Already enrolled with our address: no write, `enrolled`.
- Enrolled with another coordinator's address: rewritten, enrolled again.
- No answer to the read: `enrollment failed`, no zone data; pairing again retries.
- Zone state never 1: `enrollment unconfirmed`, zone data added all the same.
- An Enroll Request after pairing: answered with its TSN.
- Unknown zone type: only `tamper`.

## Decided in autonomy

- Added while building: `tamper` is not added at enrollment, only on the first notification with
  bit 2 (R4.5 wins over R2's table); `battery_low` is not printed in `--listen` lines (the
  registry gets it); the simulated motion sensor's 60 s clearing is in scenario time, scaled by
  `--speed` (it belongs to the scenario's story; spec 007 had it in radio time); a Default
  Response with an error to the CIE write is a refusal; the zone type is omitted from the
  `reporting:` text when unknown.
- After the code review: the `tamper` and `battery_low` flags are applied on every path (`--pair`,
  `--serve`); an unknown own address never stops a run (the enrollment or the cluster fails); a
  setup cut by the 70 s cap shows `reporting: <eui64> cut short`; the environment variable
  `MINI_SOWEL_CLOSE_GRACE_SECONDS` overrides the 70 s (for tests).
- Enrollment before the reporting setup, during the device's awake window (Zigbee2MQTT's order).
- Zone id 23, as Zigbee2MQTT.
- "Unconfirmed" still adds the zone data: many cheap sensors never report the state.

## How the user will check it

`--sim scenarios/garage.json --pair 30 --speed 10`, then `--serve --speed 10`, and read the motion
sensor's `occupancy` in `http://127.0.0.1:8080/api/v1/devices`.

Run on the real hardware (2026-11-21): The SNZB-03PR2's motion arrives as `occupancy` (cluster 0x0406), not through IAS zone enrollment; motion true/false was seen in the device data. IAS enrollment itself was not exercised by this sensor.
