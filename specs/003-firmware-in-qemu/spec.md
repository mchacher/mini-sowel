# Spec 003: firmware-in-qemu

## Context

The ESP32 of the Dongle Max has about 300 KB of internal RAM and 2 MB of PSRAM (external RAM).
Memory is one of the three main risks of the framing note (`inputs/note-de-cadrage-mini-sowel.md`,
"Risks and unknowns", and "Development and tests": "the CI builds for the target, tracks the
binary size and boots the firmware under QEMU"). Today the firmware is built but never run: the
Dongle Max is not flashed during development (decision D6), and it had not arrived on 2026-11-20.
Espressif's QEMU emulates an ESP32 on the Mac; a first try on 2026-11-20 booted the firmware and
showed its banner.

This increment makes that run a check: the firmware boots in QEMU, shows how much memory it has
left, and a broken boot fails `sh scripts/validate.sh`. It also turns on the PSRAM of the Dongle
Max in the firmware's settings, so the memory shown is the one the box will have.

Specified in autonomy: the user approved in advance (2026-11-20) the specs reviewed by
`spec-reviewer` without an important finding. Choices made without the user are listed under
"Decided in autonomy".

## Goals

1. Run the real firmware on the Mac, without hardware, in one command.
2. See, at every start, the free memory: internal RAM and PSRAM.
3. Catch a firmware that does not boot (panic, reset loop, missing banner) before a commit.

## Out of scope

- Memory ceilings asserted in the tests (milestone B, when the engine allocates).
- Networking, HTTP, OTA or the radio's UART in QEMU (milestone E).
- Flashing the Dongle Max (decision D6).
- Tracking the binary size over time.

## Requirements

### R1: The firmware's settings match the Dongle Max

`firmware/sdkconfig.defaults` sets the flash size to 16 MB, turns the PSRAM on, added to the heap
(`malloc` can use it), lets the firmware boot if no PSRAM is found, and sets the minimum chip
revision to 3 (`CONFIG_ESP32_REV_MIN_3=y`; the Dongle Max's ESP32-D0WDR2-V3 is revision 3). Below
revision 3 ESP-IDF needs a PSRAM cache workaround applied by the C compiler only, which Rust code
would not get: the firmware is therefore for revision 3 and later only.

QEMU emulates a revision 0 chip by default (seen on 2026-11-20: `Chip rev: v0.0`). The QEMU run
emulates revision 3 (through QEMU's eFuse settings). If QEMU cannot emulate it, the minimum
revision stays at its default, the spec's "Decided in autonomy" records the fallback and why, and
the risk is written in the report to the user.

### R2: The firmware shows its memory

After its banner, the firmware writes one line at level info:

`memory: internal <free> KB free (largest block <largest> KB), psram <free> KB free of <total> KB`

Sizes in KiB, rounded down. Without PSRAM: `psram none`. The text is built by the core (a pure
function from the numbers to the line), so it is tested on the Mac; the firmware only reads the
numbers from ESP-IDF's heap functions.

### R3: One command boots it in QEMU

`sh scripts/qemu.sh`:

1. builds the firmware (release) if needed;
2. assembles a 16 MB flash image (bootloader, partition table, application) with the `esptool` of
   the ESP-IDF that the firmware build downloaded;
3. starts Espressif's QEMU (`qemu-system-xtensa`, machine `esp32`) with 2 MB of PSRAM, the serial
   output captured;
4. waits for the banner `mini-sowel <version>` (the version read from `crates/core/Cargo.toml`'s
   workspace version, not written in the script) and the line starting `memory: `, prints both,
   stops QEMU and exits 0;
5. exits non-zero, printing the last 40 lines of the serial output, if within 60 seconds the
   memory line has not appeared, or as soon as the output holds `Guru Meditation Error`,
   `abort() was called` or `panicked at`, or a second `rst:` line (the ROM's reset line: a second
   one is a reboot).

The bootloader and partition table are taken from the newest `esp-idf-sys-*/out/` build folder
of the release target (there can be several).

If QEMU is not installed, the script installs it (a download, like ESP-IDF's at the first build;
the README says so) with ESP-IDF's tool installer (`idf_tools.py
install qemu-xtensa`, into `firmware/.embuild/`, as the build does for ESP-IDF) and says so. On
macOS QEMU needs `libgcrypt glib pixman sdl2 libslirp` from Homebrew; if one is missing, the
script says which command installs them and exits non-zero.

### R4: The check

`sh scripts/validate.sh` runs `sh scripts/qemu.sh` after the firmware build ("Firmware: boots in
QEMU"). The CI installs QEMU's system libraries on Ubuntu, and installs QEMU in its own step (the
ESP-IDF cache key does not change when QEMU is added, so it would be downloaded on every run).

## Living spec delta

### docs/living-spec/README.md

- Changed: "On the ESP32: the firmware starts ESP-IDF and writes `mini-sowel 0.1.0` to the serial
  log" → it also writes its free memory (internal RAM and PSRAM), and it is run on the Mac in
  Espressif's QEMU with `sh scripts/qemu.sh`.
- New: History row 003.

### No longer true

- `CLAUDE.md`, Commands: "`sh scripts/validate.sh` is the verdict: docs, format, lint, types,
  tests, UI size budget, ESP32 firmware build" → "..., ESP32 firmware build and boot in QEMU".
- `scripts/validate.sh` (header comment) and `.github/workflows/ci.yml` (header comment): the list
  of steps → the QEMU boot added.
- `README.md`, Install step 3: QEMU's system libraries added (`brew install libgcrypt glib pixman
  sdl2 libslirp` on macOS; `libgcrypt20 libglib2.0-0 libpixman-1-0 libsdl2-2.0-0 libslirp0` on
  Debian or Ubuntu), and the QEMU download at the first validate.
- `docs/architecture.md`: "writes the banner to the serial log" → also its memory; the Builds
  section gains the QEMU run.
- `README.md`, `CLAUDE.md`: the commands list → `sh scripts/qemu.sh` added.
- Searched: `QEMU`, `qemu`, `serial log`, `sdkconfig`, `PSRAM`, `unsafe`, `verdict`, `Install`,
  `A2` in `docs/`,
  `README.md`, `CLAUDE.md`, `.claude/`.

## Acceptance criteria

- [x] `sh scripts/qemu.sh` prints the banner and the memory line, with a PSRAM total of at least
      2000 KB (the measured value written in "How the user will check it").
- [x] A firmware that panics at start makes `sh scripts/qemu.sh` fail (checked once on purpose).
- [x] `sh scripts/validate.sh` runs the QEMU step and is green.

## Edge cases

- QEMU missing: installed, then run.
- System libraries missing (macOS): the command to install them, non-zero exit.
- Firmware panics or reboots in a loop: non-zero exit, last lines shown.
- Banner never shows (hang): non-zero exit after 60 seconds.
- No PSRAM: the line says `psram none`, the boot is not a failure.
- QEMU left running: the script always stops it, also on failure or Ctrl-C.

## Sowel contract

None.

## Decided in autonomy

- PSRAM turned on in the firmware now: the box has it, and memory measured without it would be
  misleading.
- No ceiling yet: the firmware allocates nothing; the ceilings come with the engine (milestone B).
- The firmware's lint goes from `unsafe_code = "forbid"` to `"deny"`: reading the heap sizes calls
  ESP-IDF's C functions (`heap_caps_get_free_size`...), which Rust marks `unsafe` (the compiler
  cannot check C code); one small function in `firmware/src/memory.rs` holds the only
  `#[allow(unsafe_code)]`, with a `SAFETY` comment. Recorded as decision D11.
- Minimum chip revision 3 (spec review): safe PSRAM with Rust code; the Dongle Max is revision 3.
- Flash image made with ESP-IDF's `esptool` rather than `espflash`: already installed with
  ESP-IDF, on the Mac and in the CI.

## How the user will check it

Run `sh scripts/qemu.sh` and read the banner and the memory line.

Result of the run (2026-11-20, Espressif QEMU 9.2.2, emulated ESP32 revision 3 with 2 MB of
PSRAM): `mini-sowel 0.1.0`, then `memory: internal 362 KB free (largest block 160 KB), psram
2045 KB free of 2048 KB`, in about 6 seconds. Demo approved in advance (autonomous run); the user
runs it on 2026-11-21.
