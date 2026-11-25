# Spec 031: colour-cluster

## Context

Roadmap inbox line of 2026-11-24 (spec 027), fourth item of the "Order of work while the box is
away" (2026-11-25): "Read the colour cluster (`0x0300`: colour temperature, colour) and send its
commands, so a real colour bulb shows `color_temp` and `color` and is proposed as `light_color`
(today `light_dimmable`)".

Spec 027 gave the core Sowel's `light_color` type, its binding rules and a proposal that picks
`light_color` when a device has a colour data or order; but no device has one: the radio reads
on/off and level only, so a colour bulb gets `state` and `brightness` and is proposed as
`light_dimmable`. Colour bulbs are the most common Zigbee lights (`docs/sowel-subset.md`).

This run is autonomous (the user is away): open questions are decided here and recorded
(decision D51, and "Decisions taken in this spec" below).

## Goals

1. A device with the Color Control cluster (`0x0300`) reports its colour temperature and its
   colour, and gets the data `color_temp` and `color` and the orders `color_temp` and `color`,
   named, typed and categorised as Sowel's Zigbee2MQTT plugin and Sowel's model name them.
2. An equipment's `color_temp` and `color` orders move the bulb (Move to Color Temperature, Move to
   Color, Move to Hue and Saturation), and the new value is read back.
3. Such a device is proposed as `light_color` with its four data and four orders bound.
4. A simulated colour bulb and a synthetic scenario show it all without a real bulb.

## Out of scope

- A colour or colour-temperature control in the interface: Sowel's `LightControl` has none, so
  there is nothing to port (D18, D19: Sowel's components, unchanged). The device and equipment
  pages already show the `color` value as JSON text (`formatDataValue`). Inbox: a Sowel issue.
- Reading `ColorCapabilities` (`0x400A`) to give a colour-temperature-only bulb (tunable white) no
  `color` data or order: every `0x0300` device gets both here (D51). Inbox.
- Zigbee2MQTT's other colour forms in an order (`hex`, `rgb`, `h`/`s`, presets such as
  `"warmest"`), the colour mode (`color_mode`), transitions, the enhanced hue, `color_temp_startup`.
  Inbox for `hex` (the form a colour picker gives).
- Colour in the zone aggregation or a recipe: recipes get the colour as `null` (the recipes' event
  value has no object form, `EventValue`); no recipe reads it.
- The realistic home's fixtures (shared with the QEMU image): unchanged, no colour bulb.

## Requirements

### R1: Reading the colour cluster

The radio decodes these attributes of cluster `0x0300` (from a report or a read answer):

| Attribute | ZCL type | Measure kind | Value | Invalid (no measure) |
| --------- | -------- | ------------ | ----- | -------------------- |
| `0x0007` ColorTemperatureMireds | uint16 (`0x21`) | `light_color_temp` | mireds, unit `mired` | 0, above 1000 |
| `0x0003` CurrentX | uint16 | `color_x` | raw / 65535, rounded to 4 decimals | above `0xFEFF` |
| `0x0004` CurrentY | uint16 | `color_y` | same | above `0xFEFF` |
| `0x0000` CurrentHue | uint8 (`0x20`) | `color_hue` | raw × 360 / 254, rounded, unit `°` | 255 |
| `0x0001` CurrentSaturation | uint8 | `color_saturation` | raw × 100 / 254, rounded, unit `%` | 255 |

Another type for one of these attributes gives no measure (as today for every cluster).

The reporting setup (`measures::REPORTING`) gets a ninth entry, after level and before power,
named `light_color` in the `reporting:` line: ColorTemperatureMireds (min 1 s, max 3600 s,
change 1), CurrentX and CurrentY (min 1 s, max 3600 s, change 16). Hue and saturation are not
set up for reporting; they are read back after a hue and saturation order (R3). The setup counts
as done when the device accepts at least one of the three (as today for every cluster). The
simulated radio's list of reportable clusters follows.

### R2: Data and orders of a colour device

A device whose reporting setup included `0x0300` gets (`devices::catalog`):

| | Key | Type | Category | Unit / bounds |
| - | --- | ---- | -------- | ------------- |
| data | `color_temp` | `number` | `light_color_temp` | `mired` |
| data | `color` | `json` | `light_color` | none |
| order | `color_temp` | `number` | `set_color_temp` | min 150, max 500 |
| order | `color` | `json` | `set_color` | none |

- `ValueType` gains Sowel's `json` (`DataType` of `types.ts`).
- Order of the data: `state`, `brightness`, `color_temp`, `color`, then the battery ones (as the
  table of spec 008 extended); order of the orders: `state`, `brightness`, `color_temp`, `color`.
  A colour bulb thus has four orders, `MAX_ORDERS` (4).
- The `color` value is Zigbee2MQTT's colour object: the fields received among `x`, `y` (0 to 1),
  `hue` (0 to 360), `saturation` (0 to 100), at most these four, in two groups: XY (`x`, `y`) and
  HS (`hue`, `saturation`). The colour measures of one frame update the object once: the fields
  they carry are replaced, the other fields of their group kept; a frame that carries a field of
  one group and none of the other drops the other group (its values would contradict the new
  colour). From the previous value only numeric `x`, `y`, `hue`, `saturation` are kept (a stored
  value that is not an object, or extra fields, are dropped), so the object never holds more than
  four fields. One `device.data.updated` per frame for `color`, with the previous object;
  `lastChanged` moves only when the object differs.
- `color_temp`'s value is the number of mireds (a number, as the other numeric data).
- `--listen` and `--pair` show `light_color_temp 370 mired`, `color_x 0.3127`, `color_y 0.329`,
  `color_hue 120 °`, `color_saturation 100 %`.

### R3: Colour orders

`POST /api/v1/equipments/:id/orders/:alias` (and the recipes' orders) resolve by the bound order's
category:

- `set_color_temp` (`color_temp`): a whole number or a numeric string (the parsing shared with
  `brightness`, whose 0 to 255 range stays its own), inside the order's bounds (150 to 500) → Move to Color Temperature (`0x0A`):
  `01 <tsn> 0A <mireds LE16> 00 00 00 00` (transition 0, options mask and override 0).
- `set_color` (`color`): a JSON object
  - with numbers `x` and `y`, each 0 to 1 → Move to Color (`0x07`):
    `01 <tsn> 07 <X LE16> <Y LE16> 00 00 00 00`, X and Y = round(value × 65535), at most `0xFEFF`;
  - else with numbers `hue` (0 to 360) and `saturation` (0 to 100) → Move to Hue and Saturation
    (`0x06`): `01 <tsn> 06 <hue> <sat> 00 00 00 00`, hue = round(hue × 254 / 360), sat =
    round(saturation × 254 / 100);
  - `x`/`y` win when both forms are given; other fields are ignored.
- `set_brightness` and `light_toggle`: unchanged.
- Anything else (missing field, out of range, not a number, `null`, a string, a number for
  `color`, an object for `color_temp`) → 400 `Invalid value for <alias>`, nothing sent.
- After the device acknowledges, the value is read back through the usual path of measures, as
  today for on/off and level: On/Off always; ColorTemperatureMireds after a colour temperature
  order; CurrentX and CurrentY after a Move to Color; CurrentHue and CurrentSaturation after a Move
  to Hue and Saturation. A read-back carries up to four attributes of one cluster in one Read
  Attributes request (a fixed array, no allocation), so the colour comes back in one frame and
  gives one `color` update (no intermediate colour). The reads go into the existing bounded read
  queue (`radio/commands.rs` `MAX_READS` 16; a read past it is dropped, as today); a colour order
  costs two reads.
- The options mask and override are 0, as Zigbee2MQTT sends them by default: a bulb that is off
  may ignore a colour order (ZCL's "execute if off" rule) while answering success; the value read
  back is then unchanged. The simulated bulb applies it while off.
- A message of the colour cluster tells the device's endpoint, as on/off and level do (the
  route an order is sent to).
- The answers are unchanged: `{"success":true}`, 502 with the reason, 404, 400.

### R4: Proposal

The proposal of spec 027 is unchanged; with R2, a device paired with `0x0006`, `0x0008` and
`0x0300` is proposed as `light_color`, its data bound as `state`, `brightness`, `color_temp`,
`color` and its orders as `state`, `brightness`, `color_temp`, `color` (Sowel's aliases).

### R5: Simulated colour bulb and scenario

- A new kind `color-light` in the simulator (`crates/sim`): identity `TEST` / `CLR01`
  (synthetic), mains powered, clusters `0x0000 0x0003 0x0004 0x0005 0x0006 0x0008 0x0300`. It
  answers On/Off, Move to Level with On/Off, Move to Color Temperature, Move to Color and Move to
  Hue and Saturation with a Default Response (status 0), keeps its state (on, level, mireds, X, Y,
  hue, saturation; kept in the radio's memory file like the relay's), reports what the host set up
  for reporting after a command, and answers Read Attributes of these attributes. A hue and
  saturation command leaves X and Y unchanged (the simulator does not convert). Its
  `silent` and `refuse_commands` switches apply to colour commands too. The new state fields of
  the radio's memory file have defaults, so a memory file written before reads.
- A scenario's `report` for `color-light` allows `light_state`, `color_temp` (150 to 500) and
  `color` (`{"x": .., "y": ..}`, each 0 to 1, played as one frame carrying CurrentX and CurrentY);
  any other field, an extra sub-field of `color`, or a value out of range, is an error naming the
  event and field, as today.
- A shipped synthetic scenario `scenarios/color-light.json`: one colour bulb joins, then reports a
  colour temperature and a colour.

### R6: Memory

- No new collection: the colour object has at most four fields by construction (R2), the reads
  and orders use the existing bounded queues, the reporting table grows from 8 to 9 entries
  (fixed).
- The worst-case test (`memory_worst_case`) holds one `color` data whose value is the four-field
  object (632 bytes measured on the Mac, one node of serde_json's map: the largest value the radio
  can give a data) on each of its 16 lights (the devices with a `state` order), and stays under
  its 1 MB ceiling; `docs/memory-budget.md` gets the new figure. Not on all 64 devices: the test's
  devices carry 16 data each with 32-character keys, while a colour bulb has 4 data (`state`,
  `brightness`, `color_temp`, `color`); its 12 data fewer weigh more than its colour object, so a
  home of 64 colour bulbs stays below the worst case's devices (decided during the build, D51).
- A test at the bound: a colour bulb has exactly `MAX_ORDERS` orders, and the orders of every
  cluster of the catalog together stay within `MAX_ORDERS` (the test fails the day a cluster adds
  an order that could not fit).

## Living spec delta

### docs/living-spec/README.md

- "Pairs devices" (reporting setup): the standard clusters list gains "colour" (`light_color` in
  the `reporting:` line).
- "Shows measures": the measure names gain `light_color_temp` (mired) and the colour parts
  `color_x`, `color_y` (0 to 1), `color_hue` (°), `color_saturation` (%), with their invalid
  values.
- "Keeps devices, in Sowel's format": data `color_temp` (light_color_temp, mired) and `color`
  (light_color, type `json`, Zigbee2MQTT's object of `x`, `y`, `hue`, `saturation`, merged per
  frame); orders `color_temp` (set_color_temp, 150 to 500) and `color` (set_color) for a colour
  device.
- "Orders": `color_temp` takes 150 to 500 mireds; `color` takes `{"x", "y"}` (0 to 1) or
  `{"hue", "saturation"}` (0 to 360, 0 to 100); the value read back after each.
- "Proposals": a colour bulb (cluster `0x0300`) is proposed as `light_color` (example added).
- "Orders" also says a bulb that is off may ignore a colour order (options 0, as Zigbee2MQTT).
- "Runs without the dongle": the `color-light` kind in the table, its report fields, the shipped
  `scenarios/color-light.json`.
- Specs table: row 031.

### No longer true

Search: `grep -rn -i` for `0x0008`, `level, power`, `light_brightness`, `brightness (0 to 254)`,
`set_brightness`, `two values`, `ValueType`, `Eight entries`, `8 entries`, `REPORTABLE`, `colour`,
`color`, `light_color`, `scenarios/`, `relay).` in `docs/` (roadmap and decisions excepted),
`README.md`, `CLAUDE.md`, `inputs/README.md`, `.claude/`.

- `docs/living-spec/README.md`: "it binds each standard cluster the device has (temperature,
  humidity, pressure, illuminance, occupancy, on/off, level, power)" → "... on/off, level, colour,
  power)".
- `docs/living-spec/README.md`: "Its orders (`DeviceOrder`): `state` ... for an on/off device,
  `brightness` ... for a dimmable one." → adds `color_temp` and `color` for a colour one.
- `docs/living-spec/README.md`: "Shipped scenarios: `scenarios/snzb02.json` (one sensor) and
  `scenarios/garage.json` ..." → adds `scenarios/color-light.json` (one colour bulb).
- `crates/core/src/devices/model.rs`: "Sowel's `DataType`, the two values Solite uses" → three.
- `crates/core/src/radio/measures.rs`: "Eight entries: the bound of the setup's work per device"
  → nine.
- `docs/sowel-subset.md`: the Data categories and Order categories tables gain
  `light_color_temp`, `light_color`, `set_color_temp`, `set_color` (`sowel_contract.rs` requires
  them to list exactly what the catalog has).
- Code comments: `crates/core/src/radio/commands.rs:1` and `:93`, `crates/sim/src/devices.rs:252`
  and `:259`, `crates/sim/src/memory.rs:55` ("On/Off or Level Control" → with Color Control);
  `crates/sim/src/scenario.rs:285` (the list of kinds in the error text gains `color-light`, and the
  dimmer if missing); `crates/core/src/devices/catalog.rs:201` ("`state` then `brightness`" → then
  `color_temp`, `color`); `crates/core/src/orders/resolve.rs:55` (the brightness parsing comment).
- Others found by the search are corrected in the same change and listed in the plan's report.

## Acceptance criteria

- [x] `--sim scenarios/color-light.json --pair 30 --speed 10 --data <fresh folder>` shows
      `reporting: <address> light_state, light_brightness, light_color` and then
      `light_color_temp 370 mired`, `color_x ...`, `color_y ...`.
- [x] `GET /api/v1/devices/<id>` of the bulb lists data `color_temp` (number, light_color_temp,
      mired) and `color` (json, light_color, value `{"x":..,"y":..}`) and orders `color_temp`
      (set_color_temp, 150, 500) and `color` (json, set_color), Sowel's JSON.
- [x] An equipment `light_color` with these bindings: `POST .../orders/color_temp {"value":250}`
      answers `{"success":true}` and the data `color_temp` reads 250; `.../orders/color
      {"value":{"x":0.2,"y":0.6}}` answers success and `color` reads `{"x":0.2,"y":0.6}`;
      `{"value":{"hue":120,"saturation":100}}` succeeds and `color` reads
      `{"hue":120,"saturation":100}` (the XY group dropped); `{"value":600}` on `color_temp` and `{"value":"red"}` on `color` answer
      400 `Invalid value for ...`.
- [x] The proposal for the paired simulated bulb is `light_color` with the four data and four
      order aliases.
- [x] `memory_worst_case` passes with a four-field colour object on each of its 16 lights;
      `docs/memory-budget.md` updated.
- [x] `sh scripts/validate.sh` green.

## Edge cases

- A colour temperature of 0 or above 1000, an X or Y above `0xFEFF`, a hue or saturation of 255,
  a wrong ZCL type: no measure.
- X alone in a frame: `color` gets or replaces `x` only, `y` kept, `hue`/`saturation` dropped.
- Hue and saturation, then an XY report: `color` is `{"x","y"}` only.
- A stored `color` object with extra or non-numeric fields: only the numeric four kept.
- A colour order to a bulb that is off: sent with options 0; a real bulb may ignore it and answer
  success (the value read back unchanged); the simulated bulb applies it.
- A `color` value from storage that is a number or a string: replaced by the next colour fields.
- A colour order with `x` but no `y`, with `x` 1.5, with `hue` but no `saturation`, an empty
  object, `null`, a string: 400.
- A `color_temp` order of 149, 501, 300.5, `"abc"`, an object: 400; `"300"` and `300` are taken.
- A colour order to a bulb that refuses it (Default Response status `0x81`): 502 `device refused
  the command (status 0x81)`; a silent bulb: 502 `device did not answer`.
- A tunable white bulb (CT only, no X/Y support): it still gets `color` and its order; the order
  is refused by the bulb (502) (D51, inbox).
- A device with `0x0300` but without on/off: data and orders added; not proposed as a light (no
  on/off order), as spec 027.
- Scenario: `color` on a `relay`, `color_temp` 100, `color` `{"x": 2, "y": 0}` or missing `y`:
  error naming the event and the field.

## Sowel contract

- `DataType` `"json"`, `DataCategory` `light_color_temp` and `light_color`, `OrderCategory`
  `set_color_temp` and `set_color`: Sowel's `src/shared/types.ts` (T:5, T:17-18, T:109-110).
- Keys `color_temp` and `color` and their categories: Sowel's `src/shared/constants.ts`
  (`color_temp: "light_color_temp"`, `color: "light_color"`) and the Zigbee2MQTT plugin's
  `PROPERTY_TO_CATEGORY` / `PROPERTY_TO_ORDER_CATEGORY` (`sowel-plugin-zigbee2mqtt`,
  `src/z2m-parser.ts`); the `color` order typed `json`: Sowel's
  `src/shared/binding-candidates.test.ts` (`order("color", "json", { category: "set_color" })`).
- The `color` object and the `color_temp` unit `mired` and bounds 150 to 500: Zigbee2MQTT's light
  expose (`color_xy` / `color_hs` composite of property `color`, `color_temp` numeric in mired,
  150 to 500 by default), which Sowel's plugin reads.
- Divergence: none. Sowel's Zigbee2MQTT plugin seems to flatten the composite `color` into data
  `x` and `y` (`flattenExposes` recurses into composite features), so a Sowel colour bulb would
  not get `color`; Solite follows Sowel's model (`constants.ts`, the binding test), and the inbox
  gets a "Sowel issue" line to check it.

## Decisions taken in this spec

- `color` is Zigbee2MQTT's object, type `json`, merged per frame (D51).
- `color_temp` bounds 150 to 500 (Zigbee2MQTT's default), not read from the bulb's
  `ColorTempPhysicalMin/Max` (D51).
- A `devices.json` holding `"type":"json"` cannot be read by a release from before this spec (the
  host stops on an unreadable `devices.json` and never overwrites it): going back to an older
  release on a box with a colour bulb needs the device re-paired or the file removed (D51; a line
  under J5's rollback).
- Every `0x0300` device gets `color` and `color_temp`; `ColorCapabilities` not read (D51, inbox).
- Hue and saturation are read back, not reported (most bulbs work in XY; Zigbee2MQTT sets up
  reporting of colour temperature and X/Y).
- No interface control (Sowel has none to port; inbox, Sowel issue).

## How the user will check it

On the Mac, without a bulb:

1. `cargo run -p solite-host -- --sim scenarios/color-light.json --pair 30 --speed 10 --data
   <fresh folder>`: the `reporting:` line with `light_color`, then the colour temperature and
   colour lines (a second run on the same folder resumes the network).
2. `cargo run -p solite-host -- --sim scenarios/color-light.json --serve --speed 10`, the
   interface on `:8080`: Administration › Discovery › "Add a device" opens pairing, then Devices › the bulb shows `color_temp` and `color` with their values and
   the four orders; an equipment "Colour light" created from the form with the bulb gets the four
   bindings; `curl` (with the token) `POST .../orders/color_temp {"value":250}` and
   `.../orders/color {"value":{"x":0.2,"y":0.6}}`: the values change on the device page live.

Result on the Mac, 2026-11-25 (autonomous run): step 1 printed `reporting: 0x00124B00C0102030
light_state, light_brightness, light_color`, `light_state on`, `light_color_temp 370 mired`,
`color_x 0.3127`, `color_y 0.329`; the orders, read-backs and proposal of step 2 are covered by
`crates/core/tests/{orders,propose,devices}.rs` against the simulated bulb; the interface step is
for the user's demo.

With a real bulb (not run in this autonomous run; a line in the roadmap's tests to run): a Zigbee
3.0 colour bulb paired from the Mac over the Dongle Max's radio (TCP, nothing flashed): the
same lines and orders, the bulb changing colour.
