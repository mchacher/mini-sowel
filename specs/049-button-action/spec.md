# Spec 049: A button's `action` (I1, second part)

Written in an autonomous run (user, 2026-11-27: continue through the night, merge on green).

## Context
The roadmap's I1: the equipment type `button` exists and accepts `action`
(`crates/core/src/equipments/rules.rs:59`, `propose.rs:71`), and `motion-light` reacts to any
`action` of its `buttons` (`recipes/scripts/motion-light.js:260,444`), but nothing makes an
`action`: Solite parses no command a device sends (only IAS's Zone Status Change,
`crates/core/src/radio/zcl.rs:489`). A Zigbee button (Sonoff SNZB-01, Ikea, Tuya remotes) sends
On/Off (`0x0006`), Level (`0x0008`) or Scenes (`0x0005`) commands to what it is bound to.

## Goals
- A paired button shows its last `action` and every press reaches the recipes, even the same
  press twice.

## Out of scope
- Device-specific names (Sonoff's `single`/`double`/`long`, Ikea's arrows): Zigbee2MQTT's
  converters per model; here the generic names of the command, as Zigbee2MQTT gives a device it
  does not know. A later part of I1 (the converters of the roadmap's "rest of I").
- Multi-endpoint remotes beyond the first endpoint with output clusters.

## Requirements

### R1: the binding
- At the interview, an endpoint that has `0x0006`, `0x0008` or `0x0005` among its **output**
  clusters and not `0x0006` among its input clusters (a remote, not a light) gets each of those
  output clusters bound to the coordinator (`Bind_req`, the existing path), so its commands reach
  Solite.

### R2: the data
- Key `action`, category `action` (Sowel `DataCategory`), a text value, no unit, made when R1
  binds at least one cluster.
- Value, from the command received (cluster-specific, client to server):
  | Cluster | Command | `action` |
  | --- | --- | --- |
  | `0x0006` | 0x00 Off, 0x01 On, 0x02 Toggle | `off`, `on`, `toggle` |
  | `0x0006` | 0x40 Off with effect, 0x42 On with timed off | `off`, `on` |
  | `0x0008` | 0x01/0x05 Move (up/down by the mode byte) | `brightness_move_up`, `brightness_move_down` |
  | `0x0008` | 0x02/0x06 Step (up/down) | `brightness_step_up`, `brightness_step_down` |
  | `0x0008` | 0x03/0x07 Stop | `brightness_stop` |
  | `0x0005` | 0x05 Recall scene (scene id n) | `recall_<n>` |
- Any other command of these clusters: no value (logged once per device and command at debug).
- Every command received is an event, even when its value equals the last one: the listeners of
  the `action` data (recipes, `/ws`) get each press.
- The device's default response is sent when the frame asks for one (the existing ZCL path).

### R3: the simulator
- A simulated button (output clusters `0x0006`, `0x0008`) that sends On, Off, Toggle and a Move
  from a scenario.

## Living spec delta
### docs/living-spec/README.md
- The recognised data (around 350): `action` of a button, the generic command names, each press an
  event.
### docs/sowel-subset.md
- The data category table: a row `action`, On/Off, Level and Scenes commands, generic names.
### No longer true
- The roadmap's I1 count (18 → 19); its note "the types `button` ... exist but nothing feeds them"
  for `button`.

## Acceptance criteria
- [ ] A simulated button: `action` `toggle`, then `toggle` again: two events (tested).
- [ ] Each row of R2's table decodes (tested).
- [ ] R1: a remote's output clusters are bound; a light's are not (tested).
- [ ] `motion-light` with the button in its `buttons`: a press toggles the lights (tested, Mac).
- [ ] validate green.

## Edge cases
- The same command twice: two events (R2).
- A command of another cluster from the button: ignored as today.
- A device with On/Off as input and output (a light that also sends): not a remote, not bound.
- Scene id 0..255: `recall_0` .. `recall_255`.

## Sowel contract
`DataCategory` "action" (`src/shared/types.ts:7` of the Sowel repository); the equipment type
`button`.

## How the user will check it
`--sim` with a button scenario: the device page's `action`, and `motion-light` driven by it.
