# Architecture: Spec 031

## Overview

```
bulb ──report/read answer (0x0300)──▶ radio::measures::decode ──Measure──▶ devices::registry::measures_with
                                          (light_color_temp, color_x/y/hue/saturation)      │ color parts of one frame merged
                                                                                            ▼ into the `color` object (json)
                                                                                   device data + live events
API POST /equipments/:id/orders/:alias ──▶ orders::resolve (by order category) ──▶ zcl::Command
   (color_temp: number; color: {x,y} | {hue,saturation})          MoveToColorTemperature / MoveToColor / MoveToHueSaturation
                                                     ──▶ radio::commands (send, Default Response, read back) ──▶ measures path
```

Everything is in the core, tested on the Mac with the simulated radio; the host and the firmware
are unchanged (they already carry the core's measures, orders and API).

## Data

| Name | Type | Unit | Where |
| ---- | ---- | ---- | ----- |
| `ValueType::Json` | enum variant, JSON `"json"` | | `devices/model.rs` |
| data `color_temp` | number | mired | device data (`devices.json` definitions, values file) |
| data `color` | JSON object, ≤ 4 fields `x`, `y`, `hue`, `saturation` | x, y 0..1; hue °; saturation % | same |
| orders `color_temp` (150..500), `color` | `DeviceOrder` | | same |
| `zcl::Command::{MoveToColorTemperature(u16), MoveToColor { x: u16, y: u16 }, MoveToHueSaturation { hue: u8, saturation: u8 }}` | enum variants | raw ZCL units | `radio/zcl.rs` |
| `CLUSTER_COLOR_CONTROL` 0x0300 and its attribute ids | consts | | `radio/zcl.rs` (shared by measures and commands) |
| `REPORTING` | 9 entries (was 8) | | `radio/measures.rs` |
| simulated device state: `color_temp`, `color_x`, `color_y`, `hue`, `saturation` | u16/u8 fields of `SimDevice` | raw ZCL units | `crates/sim/src/devices.rs`, memory file |

No new collection. The colour object is bounded by its four known fields.

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/src/radio/zcl.rs` | colour cluster and attribute constants; three `Command` variants, their cluster, id and frame |
| `crates/core/src/radio/measures.rs` | `decode` of the five attributes; the ninth `REPORTING` entry; "Eight entries" comment |
| `crates/core/src/radio/commands.rs` | `note_route_of` learns the endpoint from `0x0300` too; `StateRead` holds up to 4 attributes of one cluster (fixed array) read in one request; `on_command_answer` reads back the colour attributes per command |
| `crates/core/src/devices/model.rs` | `ValueType::Json`, `ALL`, `as_str`, doc comment |
| `crates/core/src/devices/catalog.rs` | `COLOR_TEMP`, `COLOR` data and orders for `0x0300`; `key_for_measure` (`light_color_temp`); `data_rank`, `order_rank` |
| `crates/core/src/devices/registry.rs` | `measures_with`: the colour parts of one frame merged into `color` with one `set_value` (two groups XY and HS; only the four numeric fields kept from the previous value); `json` values accepted for a `Json` data |
| `crates/core/src/orders/resolve.rs` | `command_for` by order category: colour temperature and colour commands, their value rules |
| `crates/sim/src/devices.rs` | `Kind::ColorLight` (`color-light`), its identity and clusters, colour state; colour report frames |
| `crates/sim/src/ncp.rs` | `REPORTABLE` + `0x0300`; colour commands and reads answered, reports after a command |
| `crates/sim/src/scenario.rs` | `color_temp` and `color` report fields for `color-light`; error texts naming the kinds |
| `crates/sim/src/memory.rs` | the simulated device's colour state kept in the memory file (if the relay's state is kept there) |
| `scenarios/color-light.json` | new synthetic scenario |
| `crates/core/tests/*.rs`, `crates/sim/tests/*.rs` | tests of the plan |
| `docs/living-spec/README.md`, `docs/memory-budget.md`, `docs/sowel-subset.md`, `docs/decisions.md` (D51), code comments of "No longer true", `docs/roadmap.md`, `docs/specs-index.md` | docs |

## Reused

- The decode table and `REPORTING` of `radio/measures.rs` (spec 006), the per-cluster catalog of
  `devices/catalog.rs` (spec 008), the zone-flag pattern of `measures_with` (a measure that is not
  a data of its own, spec 009) for the colour parts.
- The "whole number or numeric string" parsing of `level_command` (`orders/resolve.rs`), moved into its own helper shared by `brightness` (its 0 to 255 range kept there) and `color_temp` (150 to 500).
- The read-back of `on_command_answer` (spec 011) and the `Dimmer` kind of the simulator (spec 011)
  as the model of `color-light`.
- The proposal (`equipments/propose.rs`) and binding rules (`equipments/rules.rs`) of spec 027:
  unchanged, they already know `light_color`.
