# Plan: Spec 031

## Steps

- [x] 1. Living spec updated (the delta of `spec.md`), decision D51 written
- [x] 2. Core types: `ValueType::Json`; `zcl` colour constants and `Command` variants
- [x] 3. Core logic: `measures::decode` and `REPORTING`; catalog; registry merge of the colour
       parts; `orders::resolve` by category; `radio::commands` read-back and route
- [x] 4. Adapters: simulator `color-light` kind, ncp answers, scenario fields,
       `scenarios/color-light.json`; host and firmware: not applicable (no change)
- [x] 5. API or WebSocket: not applicable (the routes exist; the JSON follows the types)
- [x] 6. Interface: not applicable (Sowel's `LightControl` has no colour control; the value shows
       as JSON text already); inbox line
- [x] 7. Tests (table below), each failing once with the code broken
- [x] 8. Memory: worst case with a four-field colour object per device, `docs/memory-budget.md`
- [x] 9. Docs: sowel-subset, roadmap (inbox line removed, log line, test line for a real bulb,
       inbox lines of the out-of-scope items), specs index, "No longer true" corrected

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| `radio::measures` (`tests/measures.rs`) | CT 370 (`0x0007`, uint16) | `light_color_temp` 370, unit `mired` |
| `radio::measures` | CT 0, CT 1001, CT with type uint8 | no measure |
| `radio::measures` | X `0x5000`, Y `0x5500` (uint16) | `color_x` 0.3125, `color_y` 0.332 (raw/65535, 4 decimals) |
| `radio::measures` | X `0xFF00` | no measure |
| `radio::measures` | hue 127, saturation 254 (uint8) | `color_hue` 180, `color_saturation` 100 |
| `radio::measures` | hue 255, saturation 255 | no measure |
| `radio::measures` | `Display` of the colour measures | `light_color_temp 370 mired`, `color_x 0.3125`, `color_hue 180 °`, `color_saturation 100 %` |
| `radio::measures` | `REPORTING` | 9 entries; `0x0300` after `0x0008`, before `0x0001`, attributes 0x0007, 0x0003, 0x0004; `cluster_name(0x0300)` is `light_color` |
| `radio::zcl` (`tests/zcl.rs`) | `MoveToColorTemperature(370)` frame | `01 tsn 0A 72 01 00 00 00 00`, cluster 0x0300, id 0x0A |
| `radio::zcl` | `MoveToColor{x:0x5000,y:0x5500}` frame | `01 tsn 07 00 50 00 55 00 00 00 00`, id 0x07 |
| `radio::zcl` | `MoveToHueSaturation{hue:84,saturation:254}` frame | `01 tsn 06 54 FE 00 00 00 00`, id 0x06 |
| `devices` (`tests/devices.rs`) | reporting configured with 0x0006, 0x0008, 0x0300 | data `state, brightness, color_temp, color` (types, categories, unit `mired`); orders `state, brightness, color_temp (150..500), color` |
| `devices` | Sowel's JSON of the colour data and orders (`tests/sowel_json.rs`) | `{"key":"color","type":"json","category":"light_color",...}`, `{"key":"color_temp","type":"number","category":"set_color_temp","min":150,"max":500}`; no `unit` on `color` |
| `devices` | X and Y in one frame | `color` = `{"x":..,"y":..}`, one update for `color` with previous `null` |
| `devices` | then X alone | `x` replaced, `y` kept, `lastChanged` moved |
| `devices` | the same X and Y again | `lastUpdated` moved, `lastChanged` not |
| `devices` | hue and saturation and X and Y in one frame | four fields |
| `devices` | hue and saturation alone after X/Y | `{hue, saturation}`, XY dropped |
| `devices` | `color` value from storage is a number | replaced by the next fields alone |
| `devices` | colour bulb at `MAX_ORDERS` (4 orders) | exactly 4 |
| `devices::catalog` | sum of `orders_for_cluster` over every cluster | ≤ `MAX_ORDERS` |
| `devices` | hue/saturation, then an XY frame | `color` is `{x, y}` only |
| `devices` | stored `color` `{"x":0.1,"y":0.1,"z":1,"hue":"a"}` then a Y frame | `{x, y}` only, `z` and the string dropped |
| `devices` | persistence: the colour object saved and read back | the same object, `type` `json` |
| `contract` (`tests/sowel_contract.rs`) | `ValueType::ALL` ⊂ Sowel's `DataType`; catalog categories ⊂ Sowel's | passes with `json`, `light_color_temp`, `light_color`, `set_color_temp`, `set_color` |
| `orders::resolve` (`tests/orders.rs`, simulated colour bulb) | `color_temp` 250, `"300"` | Move to Color Temperature 250 / 300; success; data `color_temp` read back |
| `orders::resolve` | `color_temp` 149, 501, 300.5, `"abc"`, `{}` | 400 `Invalid value for color_temp`, nothing sent |
| `orders::resolve` | `color` `{"x":0.2,"y":0.6}` | Move to Color (13107, 39321); success; `color` reads `{"x":0.2,"y":0.6}` |
| `orders::resolve` | `color` `{"hue":120,"saturation":100}` | Move to Hue and Saturation (85, 254); success; `color` reads `{hue:120, saturation:100}` |
| `radio::commands` | Move to Color acknowledged | one Read Attributes request with X and Y; exactly one `color` update (no intermediate) |
| `orders::resolve` | colour order to the simulated bulb while off | sent, success, applied |
| `orders::resolve` | `color` with both forms | Move to Color |
| `orders::resolve` | `color` `{"x":0.2}`, `{"x":1.5,"y":0}`, `{"hue":10}`, `{}`, `null`, `"red"`, `5` | 400 `Invalid value for color` |
| `orders::resolve` | a refusing bulb (`refuse_commands` 0x81), a silent bulb | 502 with the reason |
| `orders::resolve` | `state` and `brightness` on the colour bulb | unchanged behaviour (On, Move to Level) |
| `radio::commands` (`tests/coordinator.rs` or `tests/radio.rs`) | a colour-cluster report from a device | its endpoint learned for orders |
| `equipments::propose` (`tests/propose.rs`) | the simulated colour bulb paired through the registry (0x0006, 0x0008, 0x0300) | `light_color`, data aliases `state, brightness, color_temp, color`, orders the same |
| `sim::scenario` (`crates/sim/tests/scenario.rs`) | `color-light` report with `color_temp` 370 and `color` {x,y} | parsed; played as CT and X/Y reports once reporting is set up |
| `sim::scenario` | `color` on a relay; `color_temp` 100; `color` `{"x":2,"y":0}`; `color` without `y`; `color` with an extra `z` | error naming the event and the field |
| `sim::memory` | a memory file written before (no colour fields) | read, colour state at defaults |
| `sim::devices` (`crates/sim/tests/devices.rs`) | `color-light` identity, clusters, mains | `TEST` / `CLR01`, the seven clusters |
| `sim` | `scenarios/color-light.json` loads | parsed without error (as the other shipped scenarios are tested) |
| `memory_worst_case` | one four-field `color` object on each of the 16 lights | under the 1 MB ceiling; figure printed |
| existing tests | whole suite | unchanged behaviour (relay, dimmer, sensors, proposals) |

## Validation

- [x] Validate command green
- [x] Agent review done, blocking findings fixed (none blocking; the should-fix on the XY fallback fixed)
- [ ] Demo seen and approved by the user (autonomous run: demo run on the Mac, recorded in the report)
