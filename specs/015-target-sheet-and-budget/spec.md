# Spec 015: The target on paper and in the core

## Context

Roadmap increment E1, "Prepare the target", first of two steps (the second is spec 016, the engine
in the firmware under QEMU). Before anything is flashed (decision D6), the unknowns of the Dongle
Max are written down and the box's resources are shared out: how the board is wired, how its
original firmware is saved and restored, how the 2 MB of PSRAM and the internal RAM are budgeted,
how the 16 MB of flash are partitioned, and how the box chooses its network. The user approved on
2026-11-21: Wi-Fi and Ethernet in one image, only one active, chosen at boot, Wi-Fi first; the
realistic home keeps D20's 300 KB, a separate worst case gets about 1 MB of PSRAM; the network
code itself (Wi-Fi, Ethernet, access point) is E3, since QEMU has no Wi-Fi.

## Goals

1. A hardware sheet of the Dongle Max, every fact with its source and its level of trust.
2. The memory budget of the box, item by item, written in `docs/`.
3. A test with every bound of the engine at its maximum, under a ceiling of 1 MB.
4. ESP-IDF settings that place the memory as the budget says, and a partition table for the box.
5. The network setting and the choice made at boot, in the core, tested.

## Out of scope

- The engine run in the firmware, the data partition mounted, the QEMU measure of the realistic
  home: spec 016.
- Wi-Fi, Ethernet, the access point, cable detection, TLS: E3 (the budget counts them).
- Any write to the Dongle Max. The read-only checks of the demo (chip, eFuses, flash backup) need
  the user's yes and are listed as such.

## Requirements

### R1: Hardware sheet

`docs/hardware.md` describes the Dongle Max for whoever flashes or wires it:

1. The chips: ESP32-D0WDR2-V3 (revision 3, 2 MB PSRAM in the package), EFR32MG24, 16 MB flash,
   IP101 Ethernet PHY, CP2102(N) USB bridge.
2. The pin map, one row per signal (EFR32 UART RX GPIO13 and TX GPIO17 (*unknown* until the
   GPIO17 risk below is settled: the firmware codes no pin before), EFR32 reset GPIO15 active
   low, EFR32 bootloader GPIO12, Ethernet MDC GPIO23, MDIO GPIO18, RMII clock in on GPIO0, PHY power
   GPIO5, PHY address 1, LED red GPIO4, green GPIO14, blue GPIO2, button GPIO34 active low), with a
   trust column: *confirmed* (Sonoff's own pages), *reported* (community), *unknown*, and the
   source URL.
3. The risks, each with the read-only check that settles it: GPIO17 is the UART's TX while the
   in-package PSRAM of the D0WDR2-V3 may use GPIO16 and GPIO17 (check: `esptool.py chip_id` and
   the original firmware's boot log, which say whether PSRAM is found); GPIO12 is a strapping pin
   (flash voltage) and must not be driven during boot; secure boot or flash encryption (check:
   `espefuse.py summary`); the radio's baud rate of the original firmware (check: its boot log or
   its web console).
4. The procedures: back up the whole flash first, before any other step
   (`esptool.py -b 921600 read_flash 0x0 0x1000000 <file>`, a few minutes; at 115200 baud about
   20 minutes), the file kept outside the repository (it may hold the box's Wi-Fi credentials)
   and its SHA-256 written in the sheet; entering download mode reboots the ESP32 and stops the
   original firmware's radio bridge on TCP 6638 for the time of the backup; restore it
   (`esptool.py write_flash 0x0 <file>`), Sonoff's web flasher as the second way back, and how the
   ESP32 enters download mode. Every write step says it needs the user's yes (D6).

### R2: Memory budget

`docs/memory-budget.md` shares out the box's RAM, one row per item, with its bound (the constant
and its file), its worst case and where it lives (internal RAM or PSRAM):

1. PSRAM (2048 KB): the engine (its worst case, R3, at most 1024 KB; the realistic home's 300 KB
   is shown beside it, not added: the box is in one state or the other), the HTTP and WebSocket
   clients (`live::MAX_CLIENTS`, `api::MAX_BODY_BYTES`), one TLS session at a time, the update's
   buffers, a margin. The arithmetic is written out: engine + clients + TLS + update + margin
   ≤ 2048 KB.
2. Internal RAM (about 300 KB, of which what the firmware reports at boot): ESP-IDF and its tasks'
   stacks, the network stack of the larger of Wi-Fi and Ethernet (Wi-Fi), the DMA buffers, the
   reserve kept internal by the settings of R4.
3. Each figure says whether it is measured (and where: Mac test, QEMU) or estimated (and from
   which source). A figure measured on the Mac (64-bit pointers) is marked as an upper estimate
   of the ESP32's (32-bit).
4. The interface is unchanged: its 170 KB gzip budget is not touched.

### R3: The worst-case test

`crates/core/tests/memory_worst_case.rs` builds a home with every memory bound of the engine at
its maximum. Its header lists every `pub const MAX_*` of `crates/core/src`, with its module path,
and says for each how the test reaches it, or why it bounds no memory (a count of attempts, a
protocol version). At least: `devices::registry::{MAX_DEVICES, MAX_DATA, MAX_ORDERS,
MAX_NAME_CHARS}`; `zones::{MAX_ZONES, MAX_NAME_CHARS, MAX_ICON_CHARS, MAX_DESCRIPTION_CHARS}`;
`equipments::{MAX_EQUIPMENTS, MAX_BINDINGS, MAX_ALIAS_CHARS}`; `recipes::{MAX_INSTANCES,
MAX_PARAMS_BYTES, MAX_LOG_ENTRIES, MAX_LOG_MESSAGE_BYTES}` and `motion_light::{MAX_LIGHTS,
MAX_BUTTONS}`; `home::events::{MAX_EVENTS, MAX_TEXT_BYTES}`; `orders::queue::{MAX_ORDERS,
MAX_UNATTENDED}`; `live::{MAX_CLIENTS, MAX_QUEUED_EVENTS, MAX_QUEUED_BYTES}`;
`api::MAX_BODY_BYTES`; the radio's queues. Then the largest answer in flight and the save path.

1. Its peak stays under 1024 KB (decision D23) and is printed.
2. A second test in the same file reads `crates/core/src` and fails when a `pub const MAX_*`
   exists that the header list does not name: a new bound cannot be forgotten.
3. A bound the public API refuses before its constant: the test uses the largest accepted value
   and the header says so.
4. One past each collection's bound (devices, zones, equipments, recipe instances, live clients),
   the core refuses and the peak does not grow.

The realistic-home test (`memory_ceiling.rs`) keeps its 300 KB; its function
`a_home_of_fifty_devices_ten_zones_and_fifty_equipments_answers_under_200_kb` is renamed
`..._answers_under_300_kb`.

### R4: Memory placement and partitions

1. `firmware/sdkconfig.defaults`: allocations of up to 4 KB stay in internal RAM, larger ones go
   to PSRAM (`SPIRAM_MALLOC_ALWAYSINTERNAL=4096`); 64 KB of internal RAM are kept for DMA and the
   network stack (`SPIRAM_MALLOC_RESERVE_INTERNAL=65536`); mbedTLS takes its buffers from PSRAM
   (`MBEDTLS_EXTERNAL_MEM_ALLOC`); a custom partition table (`PARTITION_TABLE_CUSTOM` and
   `PARTITION_TABLE_CUSTOM_FILENAME`), the 16 MB flash size kept.
2. `firmware/partitions.csv` for 16 MB, no factory slot (with `otadata` empty the bootloader
   starts `ota_0`):

   | Name | Type | Offset | Size |
   | ---- | ---- | ------ | ---- |
   | nvs | data, nvs | 0x9000 | 24 KB |
   | otadata | data, ota | 0xF000 | 8 KB |
   | phy_init | data, phy | 0x11000 | 4 KB |
   | ota_0 | app | 0x20000 | 4 MB |
   | ota_1 | app | 0x420000 | 4 MB |
   | storage | data, fat | 0x820000 | the rest (about 7.9 MB) |

3. `scripts/qemu.sh` writes the application at the offset of `ota_0` in the built table (read from
   it, not hard-coded); the firmware still boots in QEMU and shows its banner and memory line.

### R5: The network setting and the choice at boot

In the core, module `network`:

1. `NetworkSetting` is `auto`, `wifi` or `ethernet`, read from a JSON text `{"link": "auto"}`
   (values case-sensitive, as Sowel's enums; other fields ignored). `parse(Option<&str>)` gives
   the setting, and, when it falls back to `auto`, the reason, one of:
   - `Missing`: no text, or an object without `link`;
   - `Empty`: an empty or blank text;
   - `TooLong`: more than 256 bytes (`network::MAX_SETTING_BYTES`), not parsed;
   - `Invalid`: not JSON, not an object (`null`, a number), or `link` not a string (`5`);
   - `Unknown(value)`: a string other than the three (`"lte"`, `"WiFi"`).
   Each reason has a one-line text for the log, e.g. `network: setting unknown "WiFi", auto`.
2. `choose(setting, cable)` gives the link to start: `ethernet` and `wifi` as asked; `auto` gives
   `ethernet` when a cable is detected at boot, `wifi` otherwise. One link only, chosen once:
   nothing switches while running.
3. The firmware starts no link and reads no setting in this spec (spec 016 reads the setting
   file, E3 detects the cable and starts the link).

## Living spec delta

### docs/living-spec/README.md

- New, in "Starts and shows its name": the firmware's image holds two application slots and a
  data partition (16 MB table); allocations above 4 KB go to PSRAM, 64 KB of internal RAM are kept
  for the network and DMA.
- New section "Chooses its network": the box has one network link, Wi-Fi or Ethernet, set by
  `auto`, `wifi` or `ethernet` (`auto` by default and for a missing or wrong setting); with `auto`
  a cable at boot means Ethernet, else Wi-Fi; the choice holds until the next start. The core makes
  this choice; the firmware starts no link.
- New in "Keeps devices..." or a new "Memory" paragraph: the engine stays under 300 KB for a
  realistic home and under 1 MB with every bound at its maximum, both checked by tests on the Mac.
- History: row 015.

### No longer true

- `docs/decisions.md` D20 says "the figure is read again on the box": still true (spec 016); no
  edit.
- `crates/core/tests/memory_ceiling.rs`: the function `..._answers_under_200_kb` →
  `..._answers_under_300_kb`.
- `docs/architecture.md` "Builds" (about line 52, "16 MB flash, PSRAM in the heap", and line 55,
  the QEMU image at 0x10000) → the partition table, the placement, the image from the table.
- `docs/living-spec/README.md` "Starts and shows its name" (lines 32 to 37, the QEMU image and
  the memory line) → completed by the delta above.
- `docs/roadmap.md` E1: its text and status (first half done with spec 015, second half spec 016).
- Searched as well: "partition", "sdkconfig", "factory", "Wi-Fi", "Ethernet", "budget", "E1",
  "0x10000".

## Acceptance criteria

- [ ] `docs/hardware.md` exists with the pin map, trust levels, sources, risks and procedures.
- [ ] `docs/memory-budget.md` exists; its totals fit 2048 KB of PSRAM and the internal RAM.
- [ ] The worst-case test passes under 1024 KB, and fails when the ceiling is lowered below its
      peak.
- [ ] The firmware builds with the partition table and the settings, and boots in QEMU.
- [ ] The network choice's tests pass.
- [ ] `sh scripts/validate.sh` is green.

## Edge cases

- Network setting missing, `{}`, empty, longer than 256 bytes, invalid JSON, `null`, `{"link":
  5}`, unknown value, wrong case (`"WiFi"`): `auto`, with the reason of R5.1. Extra fields: ignored.
- A bound the API refuses before its constant: the test uses the largest accepted value.
- One past a collection's bound: refused, the peak does not grow (R3.4).
- A new `pub const MAX_*` not in the worst case's list: the test fails (R3.2).
- QEMU and a partition table whose slot does not start where the image puts the application: the
  boot fails, and `scripts/qemu.sh` says so (existing behaviour).

## Sowel contract

None: hardware, memory, partitions and a box setting that Sowel does not have. The network
setting is not exposed through the API in this spec.

## Decided in autonomy

- E1 is split in two specs (015 here, 016 for the engine in the firmware), as the user allowed.
- Data partition in FAT (built into ESP-IDF, with wear levelling) rather than LittleFS (an extra
  component).
- Slots of 4 MB: the firmware is about 1 to 2 MB; 4 MB leaves room for the interface and WebAssembly
  recipes.
- `SPIRAM_MALLOC_ALWAYSINTERNAL=4096` and a 64 KB internal reserve: ESP-IDF's usual values for a
  Wi-Fi application with PSRAM; the budget says they are estimates until the box measures them.
- Case-sensitive network values, as Sowel's enums are.
- The spec review's questions, answered with its recommendations: GPIO17 stays *unknown* until
  `chip_id` and the original boot log settle it, and the firmware codes no pin before; the 1024 KB
  worst case is a ceiling to measure again on the box, as D20's; slots of 4 MB kept, D25 saying the
  table is cheap to change before the first flash and needs a full re-flash after.

## How the user will check it

On the Mac: read `docs/hardware.md` and `docs/memory-budget.md`; `cargo test -p solite-core --test
memory_worst_case` prints its peak; `sh scripts/qemu.sh` boots with the new table.
With the user's yes, read-only on the Dongle Max over USB, the backup first (it reboots the ESP32
and stops the radio bridge while it runs): `esptool.py -b 921600 read_flash 0x0 0x1000000
<outside the repository>` and its SHA-256, then `esptool.py chip_id`, `espefuse.py summary`; their
results go into `docs/hardware.md` (pending until the user says yes).
