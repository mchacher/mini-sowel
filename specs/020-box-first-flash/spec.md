# Spec 020: The box's first flash

## Context

Roadmap increment E3, cut by the user on 2026-11-22 into E3a (this spec), E3b (the radio over
the serial link, the engine on the real radio in the box), E3c (Wi-Fi, the HTTP and WebSocket API,
the precompressed interface, set up by `scripts/demo-garage.sh`) and E3d (the access point
`Solite-XXXX`, the Wi-Fi retry, the pairing route at 404, a day of the garage with the Mac off).

Everything so far runs on the Mac or in QEMU; the Dongle Max still runs Sonoff's firmware
(decision D6), and `docs/hardware.md` says "nothing was measured on a box". E3a saves the box's
original firmware, proves the way back by restoring it once, flashes Solite's images, and
measures on the real ESP32 what QEMU could only bound: the free memory at boot, the engine's peak
with the realistic home, the FAT partition, and the stall of the 94 KB `devices.json` save (inbox
of `docs/roadmap.md`). The user asked for maximum autonomy: choices are recorded below, but every
write to the box keeps its own explicit yes (D6), which autonomy never covers.

## Goals

1. One script, `scripts/box.sh`, identifies the box, backs up its whole flash, restores it, flashes
   a Solite image and checks its boot log; every write asks for a typed confirmation.
2. The original firmware is saved, outside the repository, and restored once, before Solite is
   flashed: the way back is proven, not assumed.
3. The box's image and the measuring image boot on the Dongle Max; the measured figures replace the
   estimates of `docs/memory-budget.md` and the unknowns of `docs/hardware.md` that a boot settles.

## Out of scope

- The radio over the serial link (UART), Wi-Fi, Ethernet, HTTP and WebSocket in the firmware, the
  interface embedded, the access point, updates over the air, secure boot: E3b to E3d, then J.
- Any eFuse write: no command of this spec burns an eFuse.
- Measures that need the network or the real radio (internal RAM with Wi-Fi up, the 33 requests of
  an equipment with 16 + 16 bindings, the serial link's buffers): E3b and E3c.

## Requirements

### R1: `scripts/box.sh`, its tools and its rules

1. `sh scripts/box.sh <command> [--port <device>] [--baud <rate>] [options]`, commands `identify`,
   `backup`, `restore`, `flash`, `log`. The port is `--port`, or the only serial device that
   looks like a USB bridge (`/dev/cu.usbserial*`, `/dev/cu.SLAB_USBtoUART*`, `/dev/ttyUSB*`); none
   or several: exit 2 with the list found. The baud rate is 921600 by default.
2. Tools: `esptool.py`, `espefuse.py` and `gen_esp32part.py` taken from the `PATH` when there,
   else from ESP-IDF's Python environment in `firmware/.embuild/` (found as `scripts/qemu.sh`
   finds it). espflash is not used. The lookup, the partition table's build, the application image
   (`esptool.py elf2image --flash_mode dio --flash_size 16MB`, its `.bin` size checked against the
   `ota_0` slot) and the boot-log checks live in `scripts/lib/firmware.sh`, which `scripts/qemu.sh`
   also sources (its behaviour unchanged).
3. `identify`, `backup`, `restore` and `flash` need the box in download mode (`log` needs a normal
   boot). The script lets `esptool.py` reset it through the USB bridge (its default); when that
   gets no answer, it exits 4 and prints the manual way (hold the button while plugging the cable,
   as reported in `docs/hardware.md`) and that the original firmware's TCP bridge (port 6638)
   stops while the box is in download mode.
4. `backup`, `restore` and `flash` first run the checks of R2, then print what they will do (file,
   offsets, sizes, regions erased or read), then ask for the last 6 hex digits of the box's MAC
   address, upper or lower case alike, read from `/dev/tty`, never from standard input: with no
   terminal, exit 1 (`box: no terminal to confirm, nothing done`); a piped answer is not read.
   Anything else, or an empty answer: exit 1, `box: not confirmed, nothing done`. `--dry-run`
   prints the plan and does nothing. No option skips the question. `backup` asks too: it reboots
   the box into download mode and stops the TCP bridge.
5. Output stays short (a few lines per step); a build or a long tool output goes to a log file in
   `$TMPDIR/solite-box/` and its path is printed on failure. Captured serial logs and backups never
   go into the repository. `identify` and `log` run under `sh scripts/quiet.sh`; `backup`,
   `restore` and `flash` are run by the user in their terminal, as their prompt needs one.
6. Exit codes: 0 done, 1 failed or refused at the prompt, 2 wrong usage or port, 3 the box is
   unfit (R2.2), 4 no answer from the box.

### R2: `identify` (read-only)

1. Runs `esptool.py flash_id` and `espefuse.py summary`, and prints one line each: chip and
   revision, MAC address, flash size, secure boot, flash encryption.
2. The box is **unfit** (exit 3, `box: STOP, <reason>; nothing is written to this box`) when secure
   boot is on (`ABS_DONE_0` or `ABS_DONE_1` true), flash encryption is on (`FLASH_CRYPT_CNT` not
   zero), the chip revision is below 3 (the image needs `CONFIG_ESP32_REV_MIN_3`; it would not
   boot), or the flash is not 16 MB. `backup`, `restore` and `flash` run these checks first and stop
   the same way.

### R3: `backup` (reads only; reboots the box into download mode)

1. `sh scripts/box.sh backup --out <folder>`: refuses (exit 2) without `--out`, a folder inside
   the repository, or a file that already exists. Writes `dongle-max-<mac>-<YYYY-MM-DD>.bin` (MAC
   in lower case, without colons). The backup, the second read and the `.sha256` are created with
   mode 0600 (they may hold credentials).
2. After R1.4's confirmation, reads the whole flash (`read_flash 0x0 0x1000000`) twice, to the
   file and to a second file.
   The two must be equal, 16 777 216 bytes long, and not one repeated byte (an erased or empty
   read). Then it writes `<file>.sha256` beside it, deletes the second read, and prints the SHA-256
   and the row to paste into `docs/hardware.md` (date, chip revision, SHA-256; no MAC, R7.3).
3. Different reads, a short file or a uniform one: exit 1, both files kept for inspection, no
   `.sha256`, `box: backup NOT valid, nothing to record`.

### R4: `restore` (writes)

1. `sh scripts/box.sh restore <file>`: the file is 16 MB, its SHA-256 equals its `.sha256`, and
   the MAC in its name is the connected box's; otherwise exit 1 before any prompt.
2. After R1.4's confirmation: `write_flash --flash_mode keep --flash_freq keep --flash_size keep
   0x0 <file>` (esptool checks what it wrote), then the box reboots on its original firmware.
3. A failed write: exit 1, with the two ways out: run `restore` again at `--baud 115200`, or
   Sonoff's web flasher (`docs/hardware.md`). Writes to flash cannot reach the ROM bootloader, so
   download mode stays reachable.
4. The way back is proven when, after the restore, the host identifies the radio over TCP
   (`cargo run -p solite-host -- --radio tcp://<address>:6638`), as it did before the backup. The
   date of that proof goes into the backup's row in `docs/hardware.md`.

### R5: `flash` (writes)

1. `sh scripts/box.sh flash --image box|sim --backup <file>`: refuses (exit 1, before any prompt)
   without `--backup`, when the file's SHA-256 is not in a row of the backup table of
   `docs/hardware.md` that has a restore date (R4.4), or when the MAC in the file's name is not the
   connected box's. The sheet read is `SOLITE_HARDWARE_MD` when set (the tests never read or edit
   the real one).
2. Takes the built application `firmware/target/xtensa-esp32-espidf/release/solite-firmware` (the
   folder is `SOLITE_TARGET_DIR` when set, for the tests) and checks it is the image asked for:
   `box` holds no `scenario: started`; `sim` holds `written to flash` (R6, feature `flash-saves`).
   Missing or the other image: exit 1 with the build command to run. The bootloader is the newest
   one of the build, as `scripts/qemu.sh` takes it; the table is built from
   `firmware/partitions.csv`; the application goes to the offset of `ota_0`, and is refused when
   larger than the slot (R1.2).
3. After R1.4's confirmation: erases `nvs`, `otadata` and `storage` (offsets and sizes read from
   the built table; the original firmware's settings, which may hold Wi-Fi credentials, are in the
   saved backup), writes the bootloader at 0x1000, the table at 0x8000 and the application, then
   resets the box.

### R6: The measuring image (feature `flash-saves`)

1. A cargo feature `flash-saves` of the firmware, which turns on `sim`: the engine runs the
   realistic home as in QEMU (spec 016), but every save goes to the flash, timed. The image QEMU
   runs (`sim` alone) is unchanged.
2. Instead of `scenario: <n> saves run without the emulator's flash`, it logs `saves: <n> written to
   flash, longest <ms> ms (<file>, <KB> KB), total <ms> ms`: the time each `Storage::write` call
   takes, which is the time the radio loop is held. A count, a total and a maximum are kept, no
   list.
3. On the box the EFR32 is wired but not driven: no firmware image touches its pins. A test
   (`crates/host/tests/box_script.rs`) fails when a file of `firmware/src/` names GPIO12 or GPIO15
   (the radio's bootloader and reset lines; GPIO12 is also a strapping pin, `docs/hardware.md`).

### R7: `log`, and what the boot shows on the box

1. `sh scripts/box.sh log --image box|sim [--seconds <n>] [--replug] [--expect-kept]` resets the box (the bridge's
   DTR and RTS, as `esptool.py` does; `--replug` waits for the port to come back after the user
   plugs the cable again), captures the serial output at 115200 baud with pyserial (from ESP-IDF's
   Python environment; `SOLITE_SERIAL_CAPTURE` replaces it in the tests) to
   `$TMPDIR/solite-box/serial-<date>.log`, and checks it with the same
   checks as `scripts/qemu.sh`: banner, memory line, `storage:`, `network:`, then `radio: none`
   (box) or the `engine:` line (sim: peak at most 350 KB, internal minimum at least 64 KB, every
   device reported, no callback dropped after `scenario: started`) and the `saves:` line. It also
   fails on `psram none` (the box has 2 MB in its package), on a panic, and on a boot loop: a ROM
   reset line (`rst:`) after the first banner (`solite <version>`); the resets before it, from the
   script's own reset or from DTR and RTS toggled when the port opens, are not counted. With
   `--expect-kept` it also fails on `storage: formatted` (the second boot keeps its partition).
   Default time: 60 s (box), 240 s (sim). It prints the checked lines.
2. `--from-file <log>` checks a log already captured (no port); `--raw` captures without checks
   (the original firmware's boot log, before the backup).
3. Nothing of the box's identity goes into the repository: no MAC address, serial number, network
   name or captured log; `docs/hardware.md` holds dates, chip revision, firmware version and the
   backup's SHA-256 (CLAUDE.md, "Secrets and data").

### R8: The order on the box, and the figures written

1. Identify; capture the original boot log (`log --raw`); back up; restore once and prove it (R4.4);
   flash `box`, check its log, then boot it a second time (`log --image box --expect-kept`: the
   partition kept across a reboot); flash `sim`, check its log; restore the
   original firmware at the end, so the Mac's TCP development path works until E3b. Each write is
   its own yes; a refused write stops the order there, recorded, with no other way around it.
2. `docs/memory-budget.md` gains "On the box" figures: free memory at boot (box image), the engine
   line of the measuring image (peak, internal and PSRAM minimum), the FAT partition's size and use
   after the realistic home, and the save stall (longest and total). Rows that need the network or
   the radio stay estimates, marked "needs Wi-Fi (E3c)" or "needs the radio (E3b)".
3. `docs/hardware.md`: what the boot logs settle (chip revision, PSRAM found and its size, flash
   size, eFuses, whether the bridge resets the chip, the original firmware's version), and the
   procedure rewritten around `scripts/box.sh`. Its backup table has exactly the columns
   `| Date | Chip revision | Firmware version | SHA-256 of the backup | Restore proven |`: the date
   of the backup (YYYY-MM-DD), the revision `esptool.py` reads (`v3.x`), the version the web console
   shows, the 64 hex digits of the SHA-256 in lower case, and the date of R4.4's proof (empty until
   then). `flash` reads the fourth and fifth columns.
4. A save stall above 1 s, or a peak above D27's 350 KB on the box, is recorded with its figure and
   becomes a line in the roadmap's inbox (or a decision for the ceiling): never a raised limit.

## Living spec delta

### docs/living-spec/README.md

- New section, "Flashes the box and brings it back": `sh scripts/box.sh` identifies the Dongle
  Max (chip, revision, flash, secure boot and flash encryption, and stops on a box it cannot
  flash), saves its whole flash outside the repository with a checked SHA-256, restores it, flashes
  the box's image or the measuring image with `firmware/partitions.csv`, and checks the boot log as
  QEMU's is checked; the backup and every write ask, on the terminal, for the end of the box's MAC
  address; no command writes an eFuse.
- Changed, "Starts and shows its name", ESP32: the image runs on the Dongle Max as in QEMU; the
  measuring image (feature `flash-saves`) runs the realistic home on the box with every save
  written to flash, and logs the `saves:` line.
- Changed, "Stays within a memory budget": the engine's figures are also measured on the Dongle
  Max, with the measuring image (`docs/memory-budget.md`, "On the box").
- Changed, "Runs without the dongle": "The simulated radio runs on the Mac and in the emulated
  ESP32" → "... on the Mac, in the emulated ESP32, and on the Dongle Max in the measuring image".
- History: row 020.

### No longer true

- `docs/hardware.md`: "Nothing was measured on a box for this sheet" → what was measured, and
  when; step 3's "the chip's MAC address" → no MAC in the repository (R7.3); step 6's `espflash`
  command → `sh scripts/box.sh flash`.
- `docs/architecture.md`: the simulator "the Mac and that image, never the box's" → also the
  measuring image flashed on the box; the firmware paragraph gains `flash-saves` and `box.sh`;
  "given to the box at flashing" names `scripts/box.sh`.
- `README.md`: "The firmware is built, not flashed" → flashed only through `scripts/box.sh`, with the
  user's yes for each write; `cargo install ... espflash` → espflash dropped from the list.
- `CLAUDE.md` commands: one line for `sh scripts/box.sh`.
- `firmware/.cargo/config.toml`: the `runner = "espflash flash --monitor --partition-table
  partitions.csv"` line, a write path to the box that skips every check → removed (`cargo run` in
  `firmware/` no longer flashes; `scripts/box.sh flash` is the only way).
- `firmware/sdkconfig.defaults:27`: "the table reaches the box through `espflash
  --partition-table partitions.csv`" → "through `scripts/box.sh flash`".
- `docs/memory-budget.md`: "estimates until the box reports them", "measured again on the box
  (E3)" → the box's figures, or the increment that measures them.
- `docs/decisions.md` D25 ("through `espflash --partition-table`") and D26 ("not for the box"):
  history, not edited; decision D31 records the change.
- `docs/roadmap.md` (a step, not living spec): E3 split into rows E3a to E3d; the inbox line of the
  `devices.json` stall removed (measured here); the line of the 33 requests sent to E3c.
- Searched: "espflash", "runner", "Nothing was measured", "MAC address", "not flashed", "never the box",
  "until the box", "measured again on the box".

## Acceptance criteria

- [x] `cargo test --workspace` runs the script tests with fake tools, green; no test sends a write
      without the confirmation, and none sends an eFuse burn.
- [x] `sh scripts/qemu.sh` and `sh scripts/qemu.sh --box` behave as before, on the shared library.
- [x] On the box: two equal backup reads, SHA-256 recorded; a restore proven by `esptool verify_flash`
      on the bootloader, the partition table and both application slots (digests match). The box was
      not on the network, so the radio over TCP 6638 was not tried; a full-flash digest differs after
      the original firmware has booted (it rewrites its own data).
- [x] No file of `firmware/src/` names GPIO12 or GPIO15 (a test checks it).
- [x] On the box: `log --image box` green, then `log --image box --expect-kept` green; `log --image sim` green with
      its `engine:` and `saves:` lines.
- [x] `docs/memory-budget.md` and `docs/hardware.md` hold the box's figures. The box ends on the
      measuring image, not on its original firmware: it is the user's test box and the saved copy
      is proven (decision of 2026-11-22, the user said the box is for tests).
- [x] `sh scripts/validate.sh` is green (it also lints the `flash-saves` image).

## Edge cases

- No serial device, or several: exit 2 with the list; a wrong `--port`: the tool's error, exit 4.
- The box not in download mode (no answer to the reset): exit 4, the manual way printed.
- Secure boot or flash encryption on: STOP (exit 3), no backup is trusted (it is encrypted) and no
  flash is written; recorded in `docs/hardware.md`; the spec ends there and the user decides.
- Chip revision below 3, or a flash other than 16 MB: STOP (exit 3), nothing written.
- The two backup reads differ (a bad cable, a high baud rate): not valid; run again, at 115200.
- A backup file renamed, or of another box (MAC), or altered (SHA-256): `restore` refuses.
- The restore fails or the box does not come back: `restore` again, then Sonoff's web flasher;
  Solite is not flashed until the way back is proven.
- The box boot-loops or panics after `flash`: `log` fails with the last lines; the user's yes for
  a restore, or a flash of the other image; the cause is written in `docs/hardware.md`.
- PSRAM not found (`psram none`, which `CONFIG_SPIRAM_IGNORE_NOTFOUND` lets boot): `log` fails; the
  measuring image is not flashed; the GPIO16/17 risk of `docs/hardware.md` is read from the boot
  log.
- The bridge does not reset the chip for `log`: `--replug`.
- The user refuses a write or the backup: `box: not confirmed, nothing done`; the order stops there.
- No terminal (an agent, a pipe): `backup`, `restore` and `flash` exit 1 without asking.
- The box resets when the port opens (the CP2102's DTR and RTS): the resets before the first banner
  are not a boot loop.
- The application larger than `ota_0`, or the wrong image built: `flash` refuses before the prompt.

## Sowel contract

None: no Sowel type, field, event or route is touched. The engine's files on the box are the ones
the host already writes in Sowel's format.

## Decided in autonomy

- One script with five commands, not five scripts: the port, the tools and the checks are shared.
- esptool for every step, flash included (espflash dropped): one tool to fake in the tests, and the
  same image parts as `scripts/qemu.sh` (bootloader, table from `partitions.csv`, `ota_0` offset).
- The confirmation is the end of the box's MAC (either case) read from `/dev/tty`, never standard
  input: it proves the right box. The terminal requirement stops a pipe and a script's plain
  standard input; it does not stop a pseudo-terminal, which the tests use. The guarantee is the
  user's explicit yes in the conversation plus the MAC typed at the prompt. Claude never types it,
  and runs only `identify` and `log`. The tests answer it through a pseudo-terminal, and one test
  checks a piped answer is refused.
- The full-flash read asks the same question (download mode stops the TCP bridge for a few
  minutes), and its files are mode 0600.
- `flash` names its backup (`--backup`): the SHA-256 ties it to a row of `docs/hardware.md` with a
  proven restore, and the MAC of its name to the connected box; `SOLITE_HARDWARE_MD` lets the tests
  use their own sheet.
- The `espflash` runner of `firmware/.cargo/config.toml` is removed rather than pointed at
  `box.sh`: `cargo run` would build one image and flash it without naming a backup.
- The measuring image runs on the box with the real EFR32 wired but not driven (spec review, Q2);
  a test keeps GPIO12 and GPIO15 out of `firmware/src/`.
- The restore is proven once before any Solite flash, and the box ends E3a on its original
  firmware: the Mac's development path keeps working until E3b flashes for good.
- `flash` erases `storage`: the first boot formats, as in QEMU, rather than mounting the original
  firmware's leftovers.
- No MAC address in the repository (CLAUDE.md, no real addresses): the backup's file name holds
  it, outside the repository; this replaces `docs/hardware.md` step 3's MAC column.
- A measuring image (`flash-saves`) rather than timing saves in the QEMU image: QEMU's flash would
  take a quarter of an hour, and its timing says nothing of the box's.
- The save stall has no gate in E3a: it is measured, then judged (R8.4).
- Decision D31: the simulator runs on the box in the measuring image, never in the box's image;
  esptool replaces espflash.

## How the user will check it

On the Dongle Max, on USB to the Mac: `identify`, the backup (two reads, the SHA-256), the
restore and the radio seen again over TCP, then the box's image's boot lines twice, the measuring
image's `engine:` and `saves:` lines, and the final restore. The run (date, the box, what was
seen) is written here once done.
