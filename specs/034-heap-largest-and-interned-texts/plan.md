# Plan: Spec 034

## Steps

- [x] 1. Living spec updated (the delta of `spec.md`)
- [x] 2. Core types: `HeapStats` +2 fields, `heap_line` (R1)
- [x] 3. Core logic: the catalog's word list and `shared` (R2.1, R2.2); `deserialize_with` on the
      seven texts of `DeviceData` and `DeviceOrder` (R2.3); `lastChanged` read into `DeviceData`
      and `parse_list` in one pass (R3)
- [x] 4. Adapters: `firmware/src/radio_engine.rs` fills the two largest blocks (R1.3); host: not
      applicable (no `heap:` line on the Mac)
- [x] 5. API: not applicable (JSON unchanged)
- [x] 6. Interface: not applicable
- [x] 7. Tests: one per scenario below
- [x] 8. Measure after (R4.2): `memory_ceiling`, `devices_load_memory`, `sh scripts/qemu.sh`; set
      `HELD_BOUND_BYTES` to the measured held figure plus a margin below the figure before (70 KB); write
      "Result" in `spec.md`
- [x] 9. Docs: `docs/memory-budget.md`, roadmap (inbox lines, log), specs-index; the "No longer
      true" search

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| `memory::heap_line` (`tests/memory.rs`) | Nominal, no HTTP | `heap: internal 120 KB free (min 90 KB), largest block 60 KB, psram 1700 KB free (min 1650 KB), largest block 1500 KB, engine stack 20 KB unused of 32 KB` |
| `memory::heap_line` | With the HTTP stack | the same, then `, http stack 6 KB unused of 10 KB` |
| `memory::heap_line` | No PSRAM (`psram_largest` 0) | `largest block 0 KB` after the PSRAM figures |
| `memory::heap_line` | The parsers' prefix | the line starts `heap: internal <n> KB free (min <m> KB)` and the `sed` expression of `scripts/lib/firmware.sh` (copied in the test as a regex) extracts `m` |
| `box.sh log --image radio` (`box_script.rs`) | A good radio log, new `heap:` format | passes and prints the `heap:` line with its largest blocks |
| `box.sh log --image radio` | Minimum below 64 KB, new format | fails naming the minimum, as before |
| `box-session.sh` (`box_session_script.rs`) | B3 with a new-format `heap:` line | the report line carries `largest block` |
| `catalog` (unit) | Every text of `all_data()` and `all_orders()` | is in the word list |
| `catalog::shared` (unit) | A known word (`temperature`, `°C`, `ON`) | `Cow::Borrowed`, pointing at the constant |
| `catalog::shared` | An unknown word (`co2`), a known word in another case (`Temperature`) | `Cow::Owned`, equal to the input |
| `DeviceData` read (`tests/devices.rs` or a unit test) | JSON with a known key, category, unit | the three fields are `Cow::Borrowed` |
| `DeviceData` read | JSON with an escaped unit (`"\u00b0C"`, a backslash-u escape) | read `°C`, borrowed |
| `DeviceData` read | Unknown category and unit | kept owned, written back unchanged |
| `DeviceOrder` read | `state` order with `ON`/`OFF` | key, category, `valueOn`, `valueOff` borrowed |
| `parse_list` round trip | `firmware/fixtures/devices.json` read, then written by `Devices::to_json` (or the stored cursor) | byte-equal to the file (the fixture is written by this code, minified, with `lastChanged: null` and no `nwk`) |
| `parse_list` | A file without `lastChanged` (before spec 010) and without `nwk`/`endpoint` (before spec 011) | reads `last_changed: None` and no route, as before |
| `parse_list` | `[]` | an empty list |
| `parse_list` + `Devices::from_list` | 70 devices in the file | 64 kept, as before (cut in `from_list`) |
| `DeviceData` / `DeviceOrder` read | `unit`, `category`, `valueOn` absent or `null` | `None` |
| `parse_list` | Malformed JSON (truncated array) | `Err`, as before |
| Device API (`sowel_json.rs`, `sowel_contract.rs`) | Existing tests | unchanged, pass: `lastChanged` still absent from the device API |
| `devices_load_memory` | The realistic `devices.json` read back | held under `HELD_BOUND_BYTES`; prints held and peak |
| `memory_ceiling` | Existing test | passes; figure recorded (expected unchanged) |
| Firmware | `cargo build --release --features radio` (inside `validate.sh`) | builds with the two new fields |

## Validation

- [x] Validate command green, QEMU step included (peak under 350 KB)
- [x] Agent review done, blocking findings fixed
- [x] Demo: the figures before and after (the user is away: recorded in "Result"; box check at the next session)
