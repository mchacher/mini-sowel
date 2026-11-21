# Plan: Spec 015

## Steps

- [x] 1. Living spec updated (delta of the spec)
- [x] 2. `crates/core/src/network.rs`: `NetworkSetting`, `Link`, `parse`, `choose`; tests
- [x] 3. `memory_worst_case.rs`; rename the realistic-home test
- [x] 4. `firmware/partitions.csv`, `sdkconfig.defaults`, `scripts/qemu.sh`; QEMU boots
- [x] 5. `docs/hardware.md`, `docs/memory-budget.md` (with the worst case's measured peak)
- [x] 6. Decisions D23 to D25, architecture, roadmap; what this makes false searched for
- [x] 7. Types of the core mirroring Sowel: not applicable (no Sowel type). API, WebSocket,
      interface: not applicable.

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| network | `{"link": "wifi"}` | `Wifi`, no reason |
| network | `{"link": "ethernet"}` | `Ethernet`, no reason |
| network | `{"link": "auto"}` | `Auto`, no reason |
| network | `{"link": "wifi", "x": 1}` (extra field) | `Wifi`, no reason |
| network | No text (absent setting) | `Auto`, `Missing` |
| network | `{}` | `Auto`, `Missing` |
| network | Empty text, blank text | `Auto`, `Empty` |
| network | 257 bytes | `Auto`, `TooLong` |
| network | Invalid JSON, `null`, `{"link": 5}` | `Auto`, `Invalid` |
| network | Unknown value `"lte"` | `Auto`, `Unknown("lte")` |
| network | Wrong case `"WiFi"` | `Auto`, `Unknown("WiFi")`, log text `network: setting unknown "WiFi", auto` |
| network | `choose(Auto, cable)` | `Ethernet` |
| network | `choose(Auto, no cable)` | `Wifi` |
| network | `choose(Wifi, cable)` | `Wifi` (the setting wins over the cable) |
| network | `choose(Ethernet, no cable)` | `Ethernet` (as asked; E3 reports the missing link) |
| memory_worst_case | Every bound at its maximum | Peak under 1024 KB, printed |
| memory_worst_case | One past each collection's bound | Refused by the core, peak unchanged |
| memory_worst_case | A `pub const MAX_*` in `crates/core/src` missing from the header list | The completeness test fails, naming it |
| memory_worst_case | Ceiling lowered below the peak (checked once by hand, then restored) | The test fails |
| memory_ceiling | Realistic home, unchanged | Peak under 300 KB (existing test, renamed) |
| firmware (QEMU) | Boot with the new table | Banner and memory line, no reboot |

## Validation

- [ ] Validate command green
- [ ] Agent review done, blocking findings fixed
- [ ] Demo seen and approved by the user
