# Plan: Spec 027

## Steps

- [ ] 1. Living spec delta applied (`docs/living-spec/README.md`), `docs/sowel-subset.md` drafted.
- [ ] 2. Core types: `EquipmentType` (31, `ALL`), `EquipmentFamily`, `Extras` with its bound.
- [ ] 3. Core logic: `rules.rs`, `propose.rs`, the PUT rules, the load rule, `is_light`.
- [ ] 4. Adapters: not applicable (no hardware; the host and firmware serve the core's API).
- [ ] 5. API: `GET /system/timezone`, `api::ROUTES`, the PUT fields.
- [ ] 6. The check: extractor, snapshot written from `../sowel`, Solite's lists, subset and
      deviations tables; `validate.sh` step.
- [ ] 7. Interface: `light_color` offered with its rules.
- [ ] 8. Docs: decisions D43 and D44, memory budget (lock time, extras), roadmap, architecture,
      what is no longer true.

## Test plan

### Rust, core: the check (`crates/core/tests/sowel_contract.rs`, new)

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| extractor | A union over several lines with comments | Its values, comments ignored |
| extractor | An interface with optional fields, comments and nested object types | Its top-level field names only |
| extractor | `EngineEvent` members | Every `type: "..."` of the union |
| extractor | A route registered as `app.post<{...}>(\n  "/api/v1/x/:id",` | `POST /api/v1/x/:` |
| extractor | A name not found in the file | Fails naming the file and the name |
| snapshot | `../sowel` present | Extracted equals the snapshot, else fails naming the difference |
| snapshot | `../sowel` absent | Prints the skip line, checks against the snapshot |
| snapshot | `SOWEL_REPO` pointing at a folder without `src/shared/types.ts` | Fails naming the file |
| snapshot | Snapshot differs from `../sowel` while Solite fits both | A warning, passes |
| events | `EngineEvent::NAMES` | One per variant (exhaustive `match` in a unit test) |
| contract | Solite's lists against the snapshot | Pass |
| contract | `EquipmentType` missing one of Sowel's values (unit on the comparison function) | Fails naming it |
| contract | A Solite name outside Sowel, not in the deviations table (unit on the comparison function: `device.renamed`) | Fails naming it |
| contract | A deviation row whose name Solite no longer has, or Sowel has | Fails naming the row |
| contract | The subset table differs from the code (a row missing, a row extra) | Fails naming it |
| contract | Equipment JSON keys with every optional field set | Equal to Sowel's `Equipment` fields |
| routes | Every entry of `api::ROUTES` | Answers something other than the route-level 404 `Not found` |

### Rust, core: equipments (`crates/core/tests/equipments.rs`)

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| types | `POST` each of the 31 types | 201, the type as written |
| types | `POST` type `lamp` | 400 `Invalid equipment type: lamp` |
| types | `GET /equipments?type=<t>` for each of the 31 types | Only those |
| motion-light | A `door` equipment given as a light | Refused as today (D17) |
| extras | `equipments.json` with the twelve fields | Read, served by `GET`, written back with the same values |
| extras | `equipments.json` with a field of the wrong shape, or past 1024 bytes | That field dropped, a warning; the rest kept |
| extras | `PUT {"requireConfirmation":true,"invertDirection":true,"plugInHybrid":false,"trackOrderOutcome":null}` | 200; the booleans set, `trackOrderOutcome` absent |
| extras | `PUT {"requireConfirmation":"yes"}` | 400 `Invalid requireConfirmation`, nothing changed |
| extras | `PUT {"family":"climate"}` on a switch | Kept |
| extras | `PUT {"family":"climate"}` on a `light_onoff` | 400 `{"error":"FamilyNotApplicable","message":...}`, nothing changed |
| extras | `PUT {"family":null}` on a switch with a family | The family goes |
| extras | `PUT {"type":"solar_panel"}` on an equipment with `separateSupply: true` stored | 400 `SeparateSupplyNotApplicable` |
| extras | `PUT {"type":"main_energy_meter"}` with `localProduction: true` stored | 400 `LocalProductionNotApplicable` |
| extras | `PUT {"energyProfile":null}` with one stored | 400 `Not supported: energyProfile` |
| extras | `PUT` an object field with the stored bytes | 200, nothing changes |
| extras | `equipments.json` with an unknown key | Ignored, the rest read |
| extras | `equipment.created` and `equipment.removed` | Carry the fields as stored |
| extras | Equipments whose fields total past 8192 bytes at load | The fields past the total dropped, warnings |
| extras | `PUT {"type":"light_onoff"}` on a switch with a family | The family goes |
| extras | `PUT {"energyProfile":{...}}` | 400 `Not supported: energyProfile`, nothing changed |
| extras | `PUT {"energyProfile":null}` with none stored; the stored value given back unchanged | 200, nothing changes |
| extras | `PUT` making one equipment's fields pass 1024 bytes, or all past 8192 | 400 `Equipment fields too large (1024 bytes each, 8192 in all)` |
| extras | `POST` with `requireConfirmation` | 201, the field absent (ignored as Sowel's create) |
| extras | `equipment.updated` after a PUT | Carries the fields |
| aggregate | A `light_color` with `state` on | `lightsOn` 1, `lightsTotal` 1 |
| aggregate | A `door` equipment with a `light_state` binding | Not counted as a light |
| unchanged | The existing equipment tests | Pass |

### Rust, core: recognition (`crates/core/tests/propose.rs`, new)

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| propose | The garage's relay (ZBMINIR2: state data and order) | `light_onoff`, data `state`, order `state` |
| propose | The garage's SNZB-02 (temperature, humidity, battery, voltage) | `sensor`: temperature, humidity, battery |
| propose | The motion sensor (occupancy, battery) | `sensor`: occupancy, battery |
| propose | A dimmer (state, brightness and their orders) | `light_dimmable`: state, brightness both ways |
| propose | A colour bulb (state, brightness, color_temp data; state, brightness, color_temp orders) | `light_color`: the aliases of Sowel (`color_temp`) |
| propose | A thermostat (temperature, setpoint, set_setpoint order) | `thermostat` |
| propose | A shutter (`shutter_position` data, `set_shutter_position` order) | `shutter`: `position` |
| propose | A button (`action`, battery) | `button` |
| propose | Battery only; nothing read | `None` |
| propose | Two `light_state` data | `state`, `state_2` |
| propose | 20 sensor data | 16 bindings |

### Rust, core: time zone route (`crates/core/tests/settings.rs`)

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| system | Unset | `{"tz":"UTC","source":"fallback","offsetHours":0}` |
| system | `Europe/Paris`, a January date; a July date | `offsetHours` 1; 2; `source` `env` |
| system | `Asia/Kolkata` | 5.5 |
| system | `POST` | 405 |
| system | A location set, no time zone | `fallback`, `UTC`, 0 |

### Rust, core: memory (`crates/core/tests/memory_worst_case.rs`)

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| worst case | Extras at their 8192-byte total | Under the 1 MB ceiling; figure written |

### UI (Vitest)

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| EquipmentForm | The type list | Has `light_color` |
| bindingUtils | A colour bulb planned as `light_color` | state, brightness, color_temp bound |
| EquipmentForm | Editing a `door` equipment | Its type kept, shown as written |
| binding-candidates | A colour bulb for `light_color` | One candidate |
| equipment-type-meta | `deduceType` of a colour bulb (a `light_color_temp` data) | `light_color` |

## Validation

- `sh scripts/validate.sh --only rust` and `--only ui` green on the Mac; the firmware images by CI.
- The check broken once on purpose (`device.renamed`), then restored.
