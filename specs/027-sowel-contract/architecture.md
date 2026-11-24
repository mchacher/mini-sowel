# Architecture: Spec 027

## Overview

Three parts, all in the core except the check's files and the interface's type list:

1. **The check** (`crates/core/tests/sowel_contract.rs`): a small extractor of TypeScript text
   (string unions, interface fields, `EngineEvent` members, `home.*` literals, `app.<method>(`
   routes) builds a `Snapshot` from a Sowel repository; the checked-in
   `contract/sowel-snapshot.json` is the same structure, written by the test itself when
   `SOWEL_SNAPSHOT=write`. Solite's side is read from the code: enum lists (`ALL` constants),
   the device catalog's specs, the core's event names, `settings::KEYS`, `api::ROUTES`, and the
   JSON keys of serialised sample values. `docs/sowel-subset.md` is parsed (markdown tables under
   known headings, first column in backticks) for the subset and the deviations.
2. **Equipments** (`crates/core/src/equipments/`): `EquipmentType` with 31 variants and `ALL`;
   `EquipmentFamily` (6); `Extras`, Sowel's twelve optional fields, held as
   `Option<Box<ExtrasInner>>` in `Equipment` (8 bytes when none), flattened into its JSON;
   `rules.rs`, the port of Sowel's binding rules for eight types; `propose.rs`, the recognition.
3. **Time zone route** (`crates/core/src/api/system.rs`): `GET /system/timezone` from the
   settings and `settings::tz` (the offset at `now`).

## Data

- `Extras` (in `Equipment`, `#[serde(flatten)]`): `energy_profile`, `solar_profile`,
  `timed_command: Option<Box<RawValue>>` (a JSON object, as written); `meteringParentId`,
  `usualVehicleId: Option<Box<str>>` (1 to 64 characters); `family: Option<EquipmentFamily>`;
  six `Option<bool>`. Absent when all are `None`. Bounds: `MAX_EXTRAS_BYTES` = 1024 per equipment, `MAX_EXTRAS_TOTAL_BYTES` = 8192 for all, the fields
  serialised (keys included), written next to the type.
- `Proposal { kind: EquipmentType, data: Vec<(Id, Alias)>, orders: Vec<(Id, Alias)> }`, each
  `Vec` at most `MAX_BINDINGS` (16): built on demand, never kept.
- `api::ROUTES: &[(&str, &str)]`: method and path pattern (`/api/v1/equipments/:id`), the routes
  Solite serves (about 35).
- `contract/sowel-snapshot.json`: `{commit, unions: {name: [values]}, interfaces: {name:
  [fields]}, slotTypes, events, settings, routes}`, sorted, about 15 KB.

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/src/equipments/mod.rs` | 31 types, `ALL`, `is_light` with `light_color`; `Equipment.extras` |
| `crates/core/src/equipments/extras.rs` (new) | Sowel's optional fields: shape checks, bounds (1024 each, 8192 in all), PUT rules on the resulting type |
| `crates/core/src/equipments/rules.rs` (new) | Port of `RELEVANT_DATA`, `RELEVANT_ORDERS`, aliases, `resolveAlias`, `uniqueAlias` for eight types |
| `crates/core/src/equipments/propose.rs` (new) | `propose(device)` |
| `crates/core/src/home/equipment_ops.rs` | `EquipmentUpdate.extras`; `family` dropped off a non-switch; refused fields |
| `crates/core/src/api/equipments.rs` | Read the PUT's optional fields |
| `crates/core/src/home/mod.rs` (load) | Extras past the bound or of a wrong shape dropped with a warning |
| `crates/core/src/api/system.rs` (new), `api/mod.rs` | `GET /system/timezone`; `ROUTES` |
| `crates/core/src/settings/tz.rs` | The offset of a zone at a time, if not already public |
| `crates/core/src/devices/catalog.rs` | `all_data()`, `all_orders()` for the check |
| `crates/core/src/live/events.rs` | `EngineEvent::NAMES` (or a function listing them) |
| `crates/core/tests/sowel_contract.rs` (new), `contract/sowel-snapshot.json` (new) | The check |
| `crates/core/tests/equipments.rs`, `crates/core/tests/propose.rs` (new), `crates/core/tests/settings.rs` | Tests of R2 to R4 |
| `crates/core/tests/memory_worst_case.rs` | 64 equipments with extras at the bound |
| `crates/core/tests/lock_time.rs` (new) | R5's measurement, ignored by default |
| `docs/sowel-subset.md` (new) | Subset and deviations tables |
| `scripts/validate.sh` | Step `Sowel contract` |
| `ui/src/components/equipments/equipment-type-meta.tsx`, `bindingUtils.ts`, `EquipmentForm.tsx` and its test, `ui/src/lib/binding-candidates.ts`, the i18n files | `light_color` offered, its rules, a type outside the offered list kept |
| `docs/living-spec/README.md`, `docs/decisions.md` (D43, D44), `docs/memory-budget.md`, `docs/roadmap.md`, `docs/architecture.md`, `docs/specs-index.md` | Docs |

## Reused

- The interface's binding plan (`ui/src/components/equipments/bindingUtils.ts`,
  `equipment-type-meta.tsx`, spec 019) is the reference of `rules.rs` and `propose.rs`, itself a
  port of Sowel's.
- `crates/core/tests/sowel_json.rs` compares JSON with Sowel examples; the check reuses its idea
  (serialise, read the keys).
- `settings::tz` (spec 026) gives the local offset.
- `crates/core/tests/realistic/` builds the home of `lock_time.rs`.
