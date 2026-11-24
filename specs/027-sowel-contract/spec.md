# Spec 027: sowel-contract

## Context

The user's rule (2026-11-24): Solite rests on the same fundamentals as Sowel for every entity
(categories, equipments, zones, devices, recipes, modes, settings, events, API routes). A subset
is allowed, a deviation is not: what Solite has is exactly Sowel's (names, values, semantics,
JSON; Sowel's `src/shared/types.ts` wins), and anything outside the subset is refused or kept
untouched, never altered. Until now the rule held by care only: decision D3 limited equipment types
to seven of Sowel's 31, Solite's `Equipment` dropped every optional field of Sowel's, and nothing
checked that a new name or value stayed inside Sowel's. This is iteration 5 of milestone I
("aligned on Sowel"); the inbox lines it answers: "the binding plan in the core (Rust), when the
box makes equipments itself", "the home lock held for a whole batch of measures: measure it before
device recognition adds work per measure", and D41's `GET /api/v1/system/timezone`.

## Goals

- A check, run by `sh scripts/validate.sh` and in CI, that fails when Solite's entities hold a
  name or value Sowel does not have, and accepts a documented subset.
- Every equipment type of Sowel accepted, stored and served; Sowel's optional equipment fields
  kept instead of dropped.
- `light_color` gets its behaviour (bindings, lights count) next to the seven types that have it.
- A paired device's data and orders give a proposed equipment type and bindings, in the core.
- Sowel's `GET /api/v1/system/timezone`.
- The time the home lock is held per batch of measures, measured and written down.

## Out of scope

- Reading the colour cluster (`0x0300`: colour temperature, colour) and sending its commands: a
  real colour bulb still shows `state` and `brightness` only, so recognition proposes
  `light_dimmable` for it until then (roadmap line).
- Behaviour for the other 23 types (door, gate, awning, heater, water valve, meters...): accepted,
  stored and served, never proposed, no zone counts of their own (Sowel's `doorsTotal`,
  `waterValvesTotal`... are not in Solite's aggregation).
- The rules Sowel attaches to `energyProfile`, `solarProfile`, `timedCommand`,
  `meteringParentId`, `usualVehicleId`, `separateSupply`, `localProduction`: those fields are kept
  as stored, and refused on `PUT` (R2.4).
- A route that proposes an equipment for a device: Sowel has none (its `GET /devices/suggest` goes
  the other way, a type to devices); the proposal stays internal, for milestone C.
- D42 (the backup document): unchanged; its paths are Sowel's, so it is no name deviation: D43 records it.
- Sowel's modes, history, energy, users: Solite has none; nothing to check.

## Requirements

### R1: The contract check

1. **Snapshot**: `contract/sowel-snapshot.json`, checked in, holds what Solite is compared with,
   extracted from the Sowel repository: the values of the string unions `EquipmentType`,
   `EquipmentFamily`, `DataType`, `DataCategory`, `OrderCategory`, `DeviceStatus`, `PowerSource`
   and `DeviceSource` (its listed values) of `src/shared/types.ts`, and `TimezoneSource` of
   `src/core/timezone.ts`; the field names of the interfaces `Device`, `DeviceData`,
   `DeviceOrder`, `Zone`, `ZoneAggregatedData`, `Equipment`, `DataBinding`, `OrderBinding`,
   `RecipeInfo`, `RecipeSlotDef`, `RecipeInstance` and `RecipeLogEntry`; the slot types of
   `RecipeSlotDef.type`; the `type` of every member of `EngineEvent`; the settings keys
   (`home.*` string literals in `src/` and `ui/src/`, tests excluded: Sowel's interface is what
   writes `home.name`); and every route `app.<method>("<path>")` of `src/api/` (method in capitals,
   `:name` parameters written `:`; `GET /ws` among them).
   The snapshot records the Sowel commit it was taken from.
2. **Against Sowel's repository**: when the Sowel repository is present (`SOWEL_REPO`, else
   `../sowel` beside the Solite repository), the check extracts the same from it and fails if it
   differs from the snapshot, prints a warning naming what differs and the command that rewrites
   the snapshot (`SOWEL_SNAPSHOT=write cargo test -p solite-core --test sowel_contract`), and
   checks Solite against what it extracted (R1.3): a Sowel that moved on fails the check only
   when Solite no longer fits it (a new equipment type, a value Solite has that Sowel removed).
   When it is absent,
   it prints `sowel contract: ../sowel not found, checked against the snapshot only (<commit>)`
   and goes on with the snapshot. CI has no Sowel repository: it checks against the snapshot.
3. **Solite against the snapshot**, from Solite's own code (not from a hand-written list):
   - `EquipmentType`: Solite's values equal Sowel's, all of them, in any order;
   - `EquipmentFamily`, `DataType`, `DeviceStatus`, `PowerSource`: Solite's values are Sowel's;
   - the data and order categories Solite's devices can carry (every entry of the device catalog)
     are Sowel's `DataCategory` and `OrderCategory` values; the integration Solite's devices
     name (`source`) is one of Sowel's `DeviceSource` values or a plain name (Sowel's union is
     open);
   - the JSON keys of a device with its data and orders, of a zone, of the zone aggregation, of
     an equipment (every optional field set), of its bindings, of a recipe's info, its slots, an
     instance and a log entry are fields of the Sowel interface they mirror; for `Equipment`, the
     keys equal Sowel's (R2.3 keeps all of them);
   - every slot type of motion-light is a `RecipeSlotDef.type` value;
   - every event Solite emits (`EngineEvent` of the core, listed by `EngineEvent::NAMES`, whose
     completeness a unit test checks through an exhaustive `match` on the enum) has a Sowel
     `EngineEvent` type;
   - every settings key is one of Sowel's;
   - every route Solite serves (a table in the core, `api::ROUTES`, checked by a test to answer
     something other than the route-level `Not found`; `GET /ws` is served by the host and the
     box, not by the core's router, so it is not in the table) is one of Sowel's. The table is checked one
     way only (a route in the table is served); a route served and missing from the table is
     caught by the code review, not by the check (the router is a `match` on path segments).
4. **Subset table**: `docs/sowel-subset.md` lists, per entity, what Solite has, in one table per
   kind whose first column the check reads: the data categories, the order categories, the
   events, the settings keys, the routes, and the equipment types that have behaviour. The check
   fails when a table differs from what the code has (a value added in the code and not in the
   table, or the other way), so the table stays true. Each row says why Solite has it.
5. **Deviations**: the same file has a `Deviations` table: `kind | name | decision`. A name of
   Solite outside Sowel's passes only when it is listed there with a decision of
   `docs/decisions.md`; a row whose name is no longer in Solite, or is now Sowel's, fails the
   check (no stale row). Listed today: the route `GET /api/v1/settings/timezones` (D41), the routes
   `GET` and `POST /api/v1/integrations/solite/permit-join` (D28), the document of
   and every other one
   the check finds once built: each gets its decision, or is fixed. Found once built: none
   beyond these three names (every JSON field, category, event and settings key of Solite is
   Sowel's). Value-level differences (bounds, texts of errors,
   `null` not stored, refused fields) are not names: D43 lists them.
6. The check is one Rust test file, `crates/core/tests/sowel_contract.rs`, so `cargo test`
   (validate's Rust group, CI) runs it; `scripts/validate.sh` also runs it as its own step,
   `Sowel contract`, so its verdict line shows. A deviation breaks it: checked once by adding a
   value on purpose (an event named `device.renamed`), then removed.

### R2: Equipment types and fields

1. `EquipmentType` has Sowel's 31 values, written as Sowel writes them. `POST` and `PUT
   /api/v1/equipments` accept each of them; `GET /api/v1/equipments?type=` filters by each.
2. Behaviour (Sowel's rules, from Sowel's `ui/src/components/equipments/bindingUtils.ts` and
   `src/zones/zone-aggregator.ts`) for eight types: `light_onoff`, `light_dimmable`,
   `light_color`, `shutter`, `switch`, `sensor`, `button`, `thermostat`: the data categories and
   order keys each may bind (Sowel's `RELEVANT_DATA`, `RELEVANT_ORDERS`, `BIND_ALL_ORDER_TYPES`),
   the aliases (`resolveAlias` and its maps), in the core. `light_color` counts as a light in the
   zone aggregation (`lightsOn`, `lightsTotal`), as Sowel's `LIGHT_EQUIPMENT_TYPES`. The other 23
   types have none: no proposal, no zone count; their bindings are added by hand, as for any
   type (Sowel's API checks no relevance either).
3. Sowel's optional `Equipment` fields are kept: `energyProfile`, `requireConfirmation`,
   `trackOrderOutcome`, `meteringParentId`, `separateSupply`, `localProduction`, `family`,
   `plugInHybrid`, `usualVehicleId`, `solarProfile`, `invertDirection`, `timedCommand`. Read from
   `equipments.json` and written back unchanged; shown wherever an equipment is (`GET`, the
   `equipment.*` events). Their shape is Sowel's: booleans for `requireConfirmation`,
   `separateSupply`, `localProduction`, `plugInHybrid`, `invertDirection`; a boolean for
   `trackOrderOutcome`; a string of 1 to 64 characters for `meteringParentId` and
   `usualVehicleId`; one of Sowel's six `EquipmentFamily` values for `family`; a JSON object for
   `energyProfile`, `solarProfile`, `timedCommand`, kept as written (its inside unchecked). A
   `null` (Sowel's "absent / null = default") is not stored: the field is absent. An object is
   compared as written (its bytes), key order included.
4. `PUT /api/v1/equipments/:id` takes, with Sowel's shape: `requireConfirmation`,
   `plugInHybrid` (boolean), `trackOrderOutcome` (boolean or `null`), `family`
   (a family or `null`). A wrong shape: 400 `Invalid <field>`. Sowel's rules on the resulting
   type, with Sowel's body `{"error": <code>, "message": <text>}`
   (`src/api/routes/equipments.ts`): a non-null `family` when the resulting type is not `switch`:
   400 `FamilyNotApplicable`, `<name> is a <type> — only a switch can be given a family`; a type
   change away from `switch` drops the stored family (`equipment-manager.ts`); a stored or given
   `separateSupply: true` with a resulting type among `main_energy_meter`,
   `energy_production_meter`, `solar_panel`, `electric_vehicle` (Sowel's `NON_SUBMETER_TYPES`):
   400 `SeparateSupplyNotApplicable`, `<name> is the house total or a production meter — the
   reconciliation reference cannot be outside itself`; a stored `localProduction: true` with such
   a type or with `separateSupply`: 400 `LocalProductionNotApplicable` with Sowel's two texts. The
   eight (with `invertDirection`, which Sowel applies to orders: found by the code review)
   fields whose rules need what Solite does not have (`energyProfile`, `solarProfile`,
   `timedCommand`, `meteringParentId`, `usualVehicleId`, `separateSupply`, `localProduction`) are
   refused when given with a value other than what is stored (`null` when one is stored
   included): 400 `Not supported: <field>`, nothing changed. `POST /api/v1/equipments` ignores all twelve (Sowel's create takes none of
   them).
5. **Bounds**: the twelve fields of one equipment take at most 1024 bytes as written in JSON (the
   keys included; all twelve with Sowel-shaped objects fit), and those of all equipments together
   at most 8192 bytes (so the worst case grows by 8 KB, not 64). A `PUT` that would pass either:
   400 `Equipment fields too large (1024 bytes each, 8192 in all)`. At load, in the file's order,
   an equipment whose fields pass a bound, or a field of the wrong shape, loses that field (the
   memory bound wins over keeping it; D43), with a warning naming the equipment and the field.
   The worst-case memory test counts the 8192 bytes.
6. The interface: `light_color` joins the types the equipment form offers, with Sowel's label and
   icon and its binding rules (Sowel's entries of `RELEVANT_DATA`, `RELEVANT_ORDERS`,
   `STANDARD_ALIASES`, the category aliases `light_color`, `light_color_temp`, `set_color`,
   `set_color_temp`), Sowel's candidate rule for it (`light_color` is in Sowel's
   `CANDIDATE_BASED_TYPES`, `src/shared/binding-candidates.ts`; Solite's port is
   `ui/src/lib/binding-candidates.ts`), and `deduceType` offering it. An equipment of a type the
   interface has no label for (`door`) shows the type as written; the form keeps that type when
   the equipment is edited. The gzip cost is measured and written in `docs/design-system.md`
   (budget 170 KB).

### R3: Recognition of a paired device

1. `equipments::propose(device)` in the core gives, for a device with its data and orders,
   `None` or a proposal: the equipment type (the interface's `deduceType`, spec 019, extended to
   `light_color`) and the bindings (device data id or device order id, alias) by Sowel's
   `RELEVANT_DATA` and `RELEVANT_ORDERS` (R3.2), not by the form's candidate path: a two-relay
   device gets `state` and `state_2` here, one channel in the form. The type, first that fits:
   - `light_color`: an on/off order (`light_toggle` or `toggle_power`) and a `light_color` or
     `light_color_temp` data, or a `set_color` or `set_color_temp` order;
   - `light_dimmable`: an on/off order and a `light_brightness` data;
   - `light_onoff`: an on/off order (a relay such as the ZBMINIR2 is proposed as a light, as the
     interface does: `light_state` is the category Sowel's Zigbee2MQTT plugin gives it);
   - `shutter`: a `shutter_position` data or a `set_shutter_position` order;
   - `thermostat`: a `setpoint` data or a `set_setpoint` order;
   - `button`: an `action` data;
   - `sensor`: a data of one of Sowel's sensor categories (temperature, humidity, pressure,
     luminosity, co2, voc, noise, motion, contact_door, contact_window, water_leak, smoke);
   - else `None` (a device with only a battery, or nothing read yet).
2. Bindings: every data whose category the type may bind (R2.2), and every order whose key it
   may bind, under the alias `resolveAlias` gives; a second binding that would take an alias
   already taken gets Sowel's `uniqueAlias` (`<alias>_2`, `_3`...); at most 16 of each kind
   (`MAX_BINDINGS`), aliases of at most 32 characters.
3. No API route and no event: the function is for milestone C's proposals; nothing calls it per
   measure, so the home lock (R5) does not grow.
4. Tested with the devices of the simulator's scenarios (`scenarios/*.json`, the garage's
   SNZB-02, motion sensor and ZBMINIR2) and with synthetic devices (a colour bulb, a dimmer, a
   thermostat, a shutter, a button, a battery-only device, a device with 20 data), no real
   personal data.

### R4: The time zone route

1. `GET /api/v1/system/timezone` answers Sowel's `{tz, source, offsetHours}`: with
   `home.timezone` set, `tz` is it, `source` `"env"` (the setting is the box's `TZ`: the POSIX
   rule the core applies, see D44) and `offsetHours` its offset from UTC now, in hours (`1`, `2`,
   `5.5`, `-3`); unset, `{"tz":"UTC","source":"fallback","offsetHours":0}`, as Sowel without a
   `TZ` and without a location; unset with a location set: the same `fallback` (Sowel derives
   `auto` from the location with `tz-lookup`, which the box has no room for). Sowel takes its
   offset once at start; Solite's is the offset now, so it follows a daylight-saving change while
   running. Both differences are D44's. Other methods: 405.
2. `home.timezone` is a name Sowel has (Sowel's `GLOBAL_READABLE_KEYS`,
   `src/plugins/scoped-deps.ts`, the keys every plugin may read), though Sowel's own time zone
   does not read it yet: the Sowel issue of the roadmap stays open (Sowel to read it before
   `TZ`). `GET /api/v1/settings/timezones` stays (D41): Sowel has no list of zones, and the box
   can apply only the zones of its list.

### R5: The home lock per batch of measures

Measured on the Mac (release build, the realistic home, one live client listening to
everything): one turn of the engine under the lock (`measures`, `recompute`, `tick`,
`run_recipes`) takes 70 µs on average and 142 µs at most for a temperature report, 56 µs and
98 µs for a motion report that drives motion-light (`crates/core/tests/lock_time.rs`, run by
hand). No threshold: it is a figure for the box's later measure (the ESP32 runs this code
roughly 20 to 50 times slower, so 1.5 to 7 ms are expected there). Written in `docs/memory-budget.md` with the command; the box's figure is measured later
(roadmap). Nothing changes in the engine: recognition adds no work per measure (R3.3).

## Living spec delta

### docs/living-spec/README.md

- "Organises the home in zones and equipments", **Equipments**: replace the seven types by
  "Sowel's 31 types are accepted, stored and served; eight have Sowel's binding rules
  (`light_onoff`, `light_dimmable`, `light_color`, `shutter`, `sensor`, `switch`, `thermostat`,
  `button`); `light_color` counts as a light in the zone aggregation". Add Sowel's optional
  equipment fields (R2.3 to R2.5), the PUT rules and texts, the bounds (1024 bytes each, 8192 in all).
- Same section, new bullet **Proposals**: a paired device's type and bindings proposed by the core
  (R3), not exposed by the API.
- "Keeps the home's settings": add `GET /api/v1/system/timezone` (R4).
- New section "Checks the Sowel contract": the check (R1), the snapshot, the subset table, the
  deviations table, what fails it.
- "Shows the home in the browser": the equipment form offers `light_color`.
- "Stays within a memory budget": the lock time per batch (R5) and the extras bound counted.
- History: row 027.

### docs/sowel-subset.md (new)

The subset and deviations tables (R1.4, R1.5).

### No longer true

- Decision D3's "Equipment types are limited to light, shutter, sensor, switch, thermostat and
  button" and D14's "only the six equipment kinds of D3; no ... energy flags": replaced by D43
  (a new decision; D3 and D14 are not edited).
- D41's "plus Solite's own `home.timezone`": the key is a Sowel name (D44); D3, D14 and D41 get
  a "(in part replaced by D43 / D44)" note in their title, as earlier replaced decisions do.
- D14's "unknown fields ... ignored on equipment routes": seven Sowel fields are now refused
  (R2.4); unknown fields stay ignored.
- `docs/living-spec/README.md` ("a name of `GET /api/v1/settings/timezones`"), rewritten with
  the route of R4.
- `crates/core/tests/equipments.rs` (a test expecting `light_color` refused, the comment "none of
  the six kinds"), `ui/src/components/equipments/EquipmentForm.test.tsx` ("offers the seven
  types"), `EquipmentForm.tsx`, `ui/src/lib/binding-candidates.ts` (their port notes),
  `docs/design-system.md` (the seven types).
- Roadmap lines naming D3's six kinds or Solite's own `home.timezone`.
- The living spec's list of seven types; the comments "the seven values of decision D3's six
  kinds" (`crates/core/src/equipments/mod.rs`), "its optional flags ... are left out", and the UI's
  "The equipment types Solite makes (decision D3)" and the port notes naming D3's seven types
  (`equipment-type-meta.tsx`, `bindingUtils.ts`).
- Any other sentence found by `grep -rn -i "seven\|D3's\|six kinds\|equipment type"` in `docs/`
  (roadmap and decisions excepted), `README.md`, `CLAUDE.md`, `.claude/`.

## Acceptance criteria

- [x] `sh scripts/validate.sh --only rust` shows `Sowel contract` green, against the snapshot here
      and against `../sowel` on the user's Mac.
- [x] A value added on purpose outside Sowel (an event `device.renamed`) fails the check, naming
      it; removed after.
- [x] A row of `docs/sowel-subset.md` removed, or a stale deviation row added, fails the check.
- [x] `POST /api/v1/equipments` with each of Sowel's 31 types answers 201; an unknown type 400.
- [x] An equipment with all twelve optional fields in `equipments.json` is read and written back
      byte-for-byte in its fields, and served by `GET`.
- [x] `PUT` with `requireConfirmation: true` keeps it; with `energyProfile` 400 `Not supported:
      energyProfile`; `family` on a `light_onoff` 400 `FamilyNotApplicable`, as Sowel.
- [x] `propose` gives `light_onoff` with `state` bindings for the garage's ZBMINIR2, `sensor` for
      the SNZB-02 and the motion sensor, `light_color` for a synthetic colour bulb.
- [x] `GET /api/v1/system/timezone` answers Sowel's shape, set and unset.
- [x] The lock time is in `docs/memory-budget.md`.
- [x] The interface offers `light_color`; Vitest green; the size budget holds.

## Edge cases

- Sowel adds a value (a new equipment type): the check fails against `../sowel` until the
  snapshot is rewritten and `EquipmentType` gets it; CI keeps passing on the old snapshot.
- Sowel's repository present but `types.ts` unreadable or a union not found: the check fails
  naming the file and the name, never passes silently.
- A type Solite has no behaviour for (`door`) with bindings: stored and served; not counted in the
  zone aggregation; motion-light still refuses it (it takes `light_onoff` lights, D17).
- An equipment's type changed from `switch` to `light_onoff`: its `family` goes.
- An extras field `null` in a PUT for one of the seven refused fields when nothing is stored:
  accepted (nothing changes); with a value stored: 400 (clearing would change Sowel's state).
- `equipments.json` with an unknown key (a Sowel field this snapshot does not know): ignored at
  load, as today.
- A device with more than 16 relevant data: 16 bindings proposed, the rest left out.
- A device whose two data resolve to the same alias (two `state`): `state`, `state_2`.
- `home.timezone` set to a zone with daylight saving: `offsetHours` follows the date (1 in
  winter, 2 in summer for Paris).

## Sowel contract

- `src/shared/types.ts` (Sowel commit `ed57e7d4`): `EquipmentType` T:339, `EquipmentFamily` T:386,
  `Equipment` T:419, the unions and interfaces of R1.1, `EngineEvent` T:1843.
- `ui/src/components/equipments/bindingUtils.ts`: `RELEVANT_DATA`, `RELEVANT_ORDERS`,
  `STANDARD_ALIASES`, `ORDER_CATEGORY_ALIASES`, `DATA_CATEGORY_ALIASES`, `resolveAlias`,
  `uniqueAlias`.
- `src/zones/zone-aggregator.ts`: `LIGHT_EQUIPMENT_TYPES`.
- `src/api/routes/equipments.ts`: the PUT schema of the optional fields;
  `src/equipments/equipment-manager.ts`: `VALID_EQUIPMENT_TYPES`, the `family` rule.
- `src/api/routes/system.ts`, `src/core/timezone.ts`: `GET /api/v1/system/timezone`,
  `TimezoneSource`.
- `src/plugins/scoped-deps.ts`: `home.timezone` among the global keys.
- Deviations kept: D28 (permit-join), D41 (`/settings/timezones`), D42 (backup document), D44
  (`source: "env"` for the setting), and those the check finds (R1.5).

## How the user will check it

On the Mac: `sh scripts/validate.sh --only rust` shows the `Sowel contract` step and its line
`sowel contract: OK (... checked against ../sowel at <commit>)`; `docs/sowel-subset.md` lists
what Solite has and why, and its deviations with their decisions. With the simulator:
`cargo run -p solite-host -- --sim scenarios/garage.json --serve --speed 10`, then
`curl -X POST localhost:8080/api/v1/equipments -d '{"name":"Lampe","type":"light_color","zoneId":"<id>"}'`
answers 201, and `curl localhost:8080/api/v1/system/timezone` answers
`{"tz":"UTC","source":"fallback","offsetHours":0}`. Checked on the box later (one pass, D39, at
most one flash write): `GET /api/v1/system/timezone` after the time zone is set, and the lock time
per batch measured on the box (a log line), with the roadmap line of this spec.
