# Spec 034: heap-largest-and-interned-texts

## Context

The memory audit of 2026-11-25 (after the fix of the 363 KB QEMU peak) found no unbounded
collection, and named two things for a box that must run a month alone:

- **Fragmentation is not measured.** The radio image's periodic `heap:` line gives the free
  internal RAM and its minimum since boot, not the largest block that can still be allocated in
  one piece. A heap riddled with small holes can refuse a 4 KB block while plenty is free in all;
  the largest free block is the figure that shows it, and it must be in the line the box logs every
  240 seconds so a month's logs show its trend.
- **About a thousand small blocks are held for nothing.** A device's data (`DeviceData`) and orders
  (`DeviceOrder`) keep their `key`, `category`, `unit`, `valueOn` and `valueOff` as
  `Cow<'static, str>`. A device paired in this run points them at the catalog's constant texts (no
  allocation), but a device read back from `devices.json` at boot gets a heap copy of each, because
  serde always reads a `Cow` as an owned `String`. The realistic home (50 devices, 6 data and
  1 order each) holds about a thousand such blocks of 2 to 16 bytes, each with the ESP-IDF heap's
  own header; blocks of 4 KB or less go first to the internal RAM, the scarce one shared with
  Wi-Fi (audit gain 3). Reading the devices also builds a first list of read structures, then
  copies it into a second one, and the same for every device's data (audit gain 4).

This change adds the largest free block to the `heap:` line, and shares those texts with the
catalog's constants when the devices are read, in one pass. It serves the true north's "runs for a
month without intervention": it measures the risk and removes the small blocks that cause it.

Inputs: the audit (`memory-audit-report.md`, section 2 gains 3 and 4, section 3) and its
follow-up (`memory-next-steps.md`, parts a, c, d), in the session's scratchpad, not in the
repository.

## Goals

1. The radio image's `heap:` line prints the largest free block of the internal RAM and of the
   PSRAM, and every script that reads the line keeps working.
2. Devices read back from storage hold their data's and orders' constant texts without a heap copy
   whenever the text is one of the catalog's; the JSON written and read is byte-identical.
3. `parse_list` reads the devices in one pass, with no intermediate list.
4. The gain is measured before and after (Mac counting allocator, QEMU) and written down honestly.

## Out of scope

- The internal-RAM floor measured on the box with Wi-Fi up and the realistic home (follow-up
  part c, the box measurement): needs the box; the inbox line keeps that part.
- Loading the home in a stream (audit gain 2) and the QEMU ceiling's scope (follow-up part b):
  inbox lines, unchanged.
- The host program prints no `heap:` line (it has no ESP-IDF heap): nothing to fill there. The
  `HeapStats` value is only built by the radio image (`firmware/src/radio_engine.rs`); the
  simulated image (`--features sim`, the QEMU one) logs the `engine:` line, not `heap:`, and is
  not changed.
- `value`, `lastUpdated` and the other fields of `DeviceData` (audit gain 6).
- Ids, names and the device's own texts (`Device` already keeps exact-size `Box<str>`).

## Requirements

### R1: The largest free block in the `heap:` line

1. `HeapStats` (`crates/core/src/memory.rs`) gets two fields: `internal_largest` (the largest free
   block of internal RAM, bytes) and `psram_largest` (of PSRAM, bytes; 0 when there is no PSRAM).
2. `heap_line` writes them right after each memory's free figure and its minimum:
   `heap: internal 120 KB free (min 90 KB), largest block 60 KB, psram 1700 KB free (min 1650 KB),
   largest block 1500 KB, engine stack 20 KB unused of 32 KB`, followed as before by
   `, http stack 6 KB unused of 10 KB` when the image serves HTTP. Sizes in KiB, rounded down.
   The prefix `heap: internal <n> KB free (min <m> KB)` is unchanged, so `scripts/lib/firmware.sh`'s
   presence check (`heap: internal`) and minimum extraction (the `sed` on `(min <m> KB)`) read the
   new line as they read the old one, and `box.sh log --image radio` still fails when the minimum
   is below the 64 KB reserve.
3. The radio image fills them with `heap_caps_get_largest_free_block(MALLOC_CAP_INTERNAL)` and
   `heap_caps_get_largest_free_block(MALLOC_CAP_SPIRAM)` (`firmware/src/radio_engine.rs`,
   `heap_stats`; the internal one is already read by `firmware/src/memory.rs`).
4. `scripts/box-session.sh` (B3 and B9) already reports the whole `heap:` line after `heap: `: the
   new field appears in its report with no change; its test fixture gets the new format.

Choice recorded here (no decision needed): the field goes after the minimum and outside its
parentheses, so the parsers that read `(min <m> KB)` keep working; the same words as the
`memory:` line ("largest block").

### R2: Shared texts in the devices read back

1. The catalog (`crates/core/src/devices/catalog.rs`) has one list of every constant text a data
   or an order may carry: the keys, categories and units of every `DataSpec`, the keys,
   categories, `valueOn` and `valueOff` of every `OrderSpec`. A test checks that every text of
   `all_data()` and `all_orders()` is in it, so a spec added to the catalog without its words
   fails the tests.
2. A function of the catalog turns a read text into a `Cow<'static, str>`: `Cow::Borrowed` of the
   constant when the text equals one of the list (exact, case-sensitive), `Cow::Owned` otherwise
   (a key or a unit written by an older release, or one the catalog does not know, is kept as
   read). An absent or `null` optional text reads `None`.
3. `DeviceData`'s `key`, `category`, `unit` and `DeviceOrder`'s `key`, `category`, `valueOn`,
   `valueOff` are read through it (serde `deserialize_with`). Writing is unchanged: the JSON of the
   device API and of `devices.json` is byte-identical before and after, and a file written before
   this change reads back equal.
4. A test reads the realistic home's `firmware/fixtures/devices.json` with a counting allocator and
   checks the bytes it holds against a bound (`crates/core/tests/devices_load_memory.rs`): the
   figure measured after the change plus a margin that stays below the figure before (70 KB,
   about 6 %), so undoing the sharing fails it.

### R3: The devices read in one pass

1. `parse_list` (`crates/core/src/devices/persist.rs`) builds the list of `DeviceWithDetails`
   while it reads the JSON array, one device at a time, instead of reading a list of read
   structures and copying it into a second list.
2. A data's `lastChanged` is read straight into `DeviceData` (it is still never written by the
   device API, only by `devices.json`), so a device's data are read into their final list with no
   copy either.
3. What is read is unchanged: a file written before spec 010 (no `lastChanged`) reads `null`; one
   written before spec 011 (no `nwk`, `endpoint`) reads no route; a malformed file gives the same
   kind of error, and the storage is then never overwritten (unchanged behaviour of `load_home`).

### R4: Measured before and after

1. Before any code (measured on `main`, 88fe91b, 2026-11-25):
   - `memory_ceiling` (Mac, realistic home built through the registry): peak **174330 bytes
     (170 KB)**, 198180 bytes (193 KB) with every log entry at 200 bytes;
   - the realistic `devices.json` read back (Mac, the new test's code run on `main`): held
     **73938 bytes (72 KB)**, peak of the read **123106 bytes (120 KB)**;
   - QEMU (`sh scripts/qemu.sh`, 84 s): `engine: ... heap 176 KB (peak 327 KB); internal 221 KB
     free (min 95 KB); psram 1980 KB free (min 1916 KB)`, phase lines lowest free 2119, 2050,
     2185 and 2174 KB (phase 1, the home's loading: 2377 − 2119 = **258 KB**); **176 KB held**.
2. After: the same three figures, written in "Result" and in `docs/memory-budget.md`.
3. `memory_ceiling` builds its home through the registry (catalog texts already borrowed), so its
   figure is expected unchanged by R2; it is recorded to show nothing else moved.
4. If the held gain is under about 5 KB on the Mac counter and in QEMU, the result says so, and the
   change is kept only because it is small and the JSON is unchanged.

## Living spec delta

### docs/living-spec/README.md

- Changed (radio image, "every 240 seconds it logs"): `heap: internal <n> KB free (min <m> KB),
  psram <p> KB free (min <q> KB), engine stack ...` → `heap: internal <n> KB free (min <m> KB),
  largest block <l> KB, psram <p> KB free (min <q> KB), largest block <r> KB, engine stack ...`.
- Changed (memory section, "The radio image logs its own `heap:` line (the minimums since boot,
  ..."): adds "the largest free block of each memory, the fragmentation's indicator".
- Nothing user-visible changes for the shared texts and the one-pass read (same JSON, same API).

### No longer true

Searched for `heap:`, `HeapStats`, `heap_line`, `parse_list`, `StoredDataIn`, `intern`, `Cow`,
`largest block` in `docs/` (roadmap and decisions excepted), `README.md`, `CLAUDE.md`,
`inputs/README.md`, `.claude/`:

- `docs/living-spec/README.md`: the two sentences above.
- `docs/memory-budget.md`: the realistic home's Mac figure (`peak 182 KB, 194 KB`, line 18) and
  the PSRAM table's "Realistic home: 300 KB (ceiling; measured 194 KB)" (line 41) are stale
  (170 KB, 193 KB measured on `main`); both rewritten with the figures of R4.
- Nothing else found (`docs/architecture.md` names the `heap:` line without its fields).

## Acceptance criteria

- [x] `heap_line` prints both largest blocks, in the format of R1.2, with and without the HTTP
      stack (unit tests)
- [x] `box.sh log --image radio` reads a `heap:` line in the new format: it prints it, and still
      fails when its minimum is under 64 KB (`box_script.rs`)
- [x] The radio firmware image builds with the two new fields filled
- [x] Every catalog text is in the shared list (test); a read known text is borrowed, an unknown
      one owned (test)
- [x] The device API's JSON and `devices.json` are byte-identical (existing Sowel JSON and saves
      tests, and a round-trip test of the realistic fixture: read then written equals the file)
- [x] `devices_load_memory` passes under its bound; the before and after figures are in "Result"
      and in `docs/memory-budget.md`
- [x] `sh scripts/validate.sh` green, the QEMU step included, its peak still under 350 KB

## Edge cases

- No PSRAM: `psram_largest` 0, printed `largest block 0 KB` (the radio image always has PSRAM; the
  value is defined, not guessed).
- A text with the right letters in another case (`Temperature`): not the constant, kept owned.
- A unit or category the catalog does not know (a file written by a future release): kept owned,
  read and written as before.
- A text that is escaped in the JSON (`"\u00b0C"` for `°C`, written with a backslash-u escape): serde hands an owned string to the
  visitor; it is still matched and borrowed.
- `devices.json` empty list `[]`: an empty list, no allocation beyond it.
- A `devices.json` holding more than `MAX_DEVICES` (64) devices, or more than `MAX_DATA` /
  `MAX_ORDERS` on a device: read whole by `parse_list`, then cut to the bounds by
  `Devices::from_list` (`registry.rs:103-112`), as before; the one-pass reader does not stop early.
- An optional text (`unit`, an order's `category`, `valueOn`, `valueOff`) absent or JSON `null`:
  `None`, as before (`default` kept beside `deserialize_with`).
- A malformed `devices.json`: an error, as before (R3.3).

## Sowel contract

None changed: `DeviceData` and `DeviceOrder` (`src/shared/types.ts` of the Sowel repository,
T:225 and T:236) are written with the same fields and values; `crates/core/tests/sowel_contract.rs`
and `sowel_json.rs` keep passing. `lastChanged` stays out of the device API: it is never written
by `DeviceData`'s own serialisation. Reading it into `DeviceData` means a `DeviceWithDetails` read
from JSON (`model.rs`, `DetailsIn`) now accepts a `lastChanged` on its data where it ignored it;
harmless, since nothing the API writes changes, and Sowel ignores unknown fields the same way.

## How the user will check it

On the Mac: `cargo test -p solite-core --test devices_load_memory -- --nocapture` prints the held
bytes of the 50 devices read back, and "Result" compares them with the figure before; `sh
scripts/qemu.sh` prints the `engine:` line with its held and peak figures. On the box, at the next
session (not run here): `sh scripts/box.sh log --image radio` shows the new `heap:` line with both
largest blocks, and `box-session.sh` B3 and B9 report it.

## Spec review leftovers (minor, not blocking)

- The two subjects (the `heap:` line, the shared texts) are bundled: both small, both memory
  work of the same audit; kept together.

Code review (opus, 2026-11-25): no blocking finding; fixed: a stale "about 260 KB" in
`docs/memory-budget.md`, the SAFETY comment of `heap_stats`, the dated QEMU figure of the box
table.

## Result

Built 2026-11-25, measured on the Mac and in QEMU (the box not run).

| Figure | Before (`main`) | After | Delta |
| ------ | --------------- | ----- | ----- |
| `memory_ceiling`, peak | 174330 bytes (170 KB); 198180 bytes with 200-byte log entries | identical | 0 (as expected: its texts are already borrowed) |
| 50 devices read back, held (Mac) | 73938 bytes (72 KB) | 67450 bytes (65 KB) | -6488 bytes (-8.8 %) |
| Peak of that read (Mac) | 123106 bytes (120 KB) | 109986 bytes (107 KB) | -13120 bytes |
| QEMU held at the end | 176 KB | 159 KB | -17 KB |
| QEMU peak | 327 KB | 311 KB | -16 KB |
| QEMU internal RAM free / minimum | 221 / 95 KB | 238 / 110 KB | +17 / +15 KB |
| QEMU phase 1 (home's loading) | 258 KB | 208 KB | -50 KB |

The gain is above the 5 KB threshold of R4.4 on both counters, modest on the Mac (about 6 KB held),
larger on the 32-bit target (17 KB), and taken from the internal RAM. `HELD_BOUND_BYTES` is 70 KB
(67450 plus about 6 %): a 10 % margin (73 KB) would have stayed above the 73938 bytes before, so
the test would not catch the sharing undone.
