# Architecture: Spec 034

## Overview

```
radio image, every 240 s:  heap_caps_* (internal, SPIRAM: free, min, largest)
                             → HeapStats (core) → heap_line → `heap:` log line
                             → scripts/lib/firmware.sh (presence, min) · box.sh log · box-session B3/B9

boot:  devices.json text → parse_list (one pass, a seq visitor)
                             → DeviceWithDetails { data: Vec<DeviceData>, orders: Vec<DeviceOrder> }
                                 key / category / unit / valueOn / valueOff
                                 → catalog::shared(text): Cow::Borrowed(constant) | Cow::Owned
                           → Devices::from_list (shrink_to_fit, unchanged)
```

## Data

- `HeapStats.internal_largest: usize`, `HeapStats.psram_largest: usize` (bytes; the line prints
  KiB rounded down). Not stored.
- `catalog::WORDS` (name free): a `const` array of `&'static str`, every key, category, unit,
  `valueOn` and `valueOff` of the catalog's specs (about 45 words; bounded by the catalog, a
  constant, no allocation). Lookup is a linear search over it, a few hundred comparisons per
  device at boot only; never `all_data()` (which tries 131 072 cluster numbers) at run time.
- `DeviceData.last_changed`: read from `lastChanged` when present (`#[serde(default,
  skip_serializing, with = "iso_option")]` instead of `#[serde(skip)]`); still never written by
  `DeviceData`'s own `Serialize` (the device API), written by `StoredDataOut` for `devices.json` as
  before.

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/src/memory.rs` | `HeapStats` +2 fields; `heap_line` prints them (R1.2); doc comment |
| `crates/core/tests/memory.rs` | the two `heap_line` tests take the new fields and format; one for the parsers' prefix |
| `firmware/src/radio_engine.rs` | `heap_stats` fills both largest blocks (`heap_caps_get_largest_free_block`) |
| `crates/host/tests/box_script.rs` | canned radio log's `heap:` line in the new format; the low-minimum test likewise |
| `crates/host/tests/box_session_script.rs` | canned `heap:` line in the new format (B3 reports it) |
| `crates/core/src/devices/catalog.rs` | the word list, `pub(crate) fn shared(&str) -> Cow<'static, str>`, unit tests (every catalog word listed; known borrowed, unknown owned) |
| `crates/core/src/devices/model.rs` | `deserialize_with` on the seven text fields (two helpers: plain and `Option`, a small `Visitor` taking `visit_str`/`visit_borrowed_str`/`visit_string`); `last_changed` read |
| `crates/core/src/devices/persist.rs` | `StoredIn.data: Vec<DeviceData>`; `StoredDataIn` removed; `parse_list` through a seq visitor that converts each `StoredIn` as it is read |
| `crates/core/tests/devices_load_memory.rs` | new: held bytes of the realistic `devices.json` read back, against a bound |
| `crates/core/tests/devices.rs` or `saves.rs` (where the save round-trips live) | round trip of `firmware/fixtures/devices.json`: read then written equals the file's JSON; an old file without `lastChanged`/`nwk` still reads |
| `docs/living-spec/README.md` | the `heap:` line's format (delta) |
| `docs/memory-budget.md` | the figures before and after; the Mac realistic figure refreshed; the radio image's `heap:` row names the largest block |
| `docs/roadmap.md` | inbox: the interning line removed; the internal-RAM line keeps only the box measurement; log line |
| `docs/specs-index.md` | row 034 |

## What is reused

- `Alias::new` (`crates/core/src/equipments/mod.rs:130-142`): the same idea, sharing the few words
  every item repeats. Here the words are fixed by the catalog, so a `const` list replaces the
  `Mutex<Vec<Arc<str>>>` (no lock, no allocation, nothing to bound at run time).
- `catalog::all_data` / `all_orders` (spec 027's contract check): the test that the list misses no
  word walks them.
- The counting-allocator pattern of `memory_ceiling.rs` (one test per file, `peak_alloc`).
- `firmware/src/memory.rs` already reads `heap_caps_get_largest_free_block(MALLOC_CAP_INTERNAL)`
  for the `memory:` line.
