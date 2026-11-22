# Spec 021: The radio over the serial link

## Context

Roadmap increment E3b (E3 cut on 2026-11-22: E3a spec 020, done; E3c Wi-Fi, API and interface in
the box; E3d the access point). The box runs Solite's images but its EFR32MG24 radio is wired and
not driven (D31). E3b drives it from the ESP32 over the serial link (UART), runs the core's engine on
it, and shows the result on the serial log; the box has no network in E3b. The user: "continue with
E3b, we optimise the saves when we know it works"; a message lost while a save runs is accepted.
Maximum autonomy: choices under "Decided in autonomy"; every write to the box keeps its typed MAC
confirmation (D6, D31). The radio image is a development image: it is in no public release before
J4 (its pairing window and its plain-text keys are accepted for the test box only).

## Goals

1. A radio image (cargo feature `radio`) drives the EFR32 over the UART and runs the engine on it.
2. The roadmap's sensor, a motion sensor (Sonoff SNZB-03PR2), is paired with no network and no API;
   its measures show on the serial log. A plug (Sonoff ZBMINIR2) is a bonus.
3. The serial buffers, the engine's stack and the internal RAM are measured on the box.

## Out of scope

- The network, the API and the interface in the box (E3c, E3d); SNTP; flash encryption.
- The save optimisations (the radio apart from the writer, one file per device): the 3.8 s stall
  stays as measured. Updating the EFR32's firmware; leaving or re-forming the radio's network.
- A second flash of the radio image in this spec (R4.4).

## Requirements

### R1: The radio image

1. Built into its own target folder with its own ESP-IDF settings (R3.2):
   `cd firmware && CARGO_TARGET_DIR=target/radio ESP_IDF_SDKCONFIG_DEFAULTS="sdkconfig.defaults;sdkconfig.radio.defaults" cargo build --release --features radio`.
   The box, sim and measuring images are built as before, with the same settings and folder;
   `radio` with `sim` is a compile error (`radio and sim are two images`).
2. After the banner, `memory:`, `storage:` and `network:` lines (unchanged), the image starts one
   thread, `engine`, pinned to core 1, 32 KB of stack, which owns the UART, the radio and the
   engine's loop (`solite_core::engine::run`, as `--serve` runs it, without HTTP, `verbose`: the
   measures and pairing lines as `--listen` and `--pair` show them). The home is read from `/data`;
   saves go to the FAT partition. `/data` is empty at the first boot (`flash` erases `storage`).

### R2: The radio's pins, described in the core

1. `solite_core::radio::serial` describes the pins by role, never by number: `Reset` (active low)
   and `Bootloader` (active high), and the hardware reset as a list of steps: bootloader inactive,
   reset active, wait `RESET_PULSE_MS` (100), reset inactive, wait `BOOT_WAIT_MS` (1000) for the
   EFR32's application to start. The bootloader line is never set active.
2. The firmware runs them after the ESP32's boot (from the `engine` thread), in that order. Both
   pins are strapping pins of the ESP32: GPIO12 (MTDI, the flash voltage) and GPIO15 (MTDO: held
   low at reset it silences the ROM's boot messages). Each pin's level is written before it is
   switched to an output (GPIO12 low, then GPIO15 high, then the pulse), so GPIO15 never goes low
   before GPIO12 is low. At an ESP32 reset both return to inputs with their default pulls.
3. GPIO12, GPIO15, GPIO13 and GPIO17 are named in one file only, `firmware/src/radio_uart.rs`.

### R3: The UART transport

1. `firmware/src/radio_uart.rs` implements the core's `RadioTransport` over UART1: TX GPIO17, RX
   GPIO13, 8 data bits, no parity, 1 stop bit, no flow control (`docs/hardware.md` lists none).
2. Receive ring `RX_RING_BYTES` (4096) and transmit ring `TX_RING_BYTES` (1024), both in internal
   RAM; the interrupt is in internal RAM (`ESP_INTR_FLAG_IRAM`, which needs
   `CONFIG_UART_ISR_IN_IRAM=y`), so bytes keep arriving while a flash write stalls both cores'
   cache (the chip's FIFO, 128 bytes, lasts about 11 ms at 115200 baud). That setting is in
   `firmware/sdkconfig.radio.defaults`, read for the radio image only (R1.1): the other images and
   their QEMU and box figures are unchanged.
3. `read(buf, timeout_ms)` waits at most `timeout_ms` and returns `Ok(0)` when nothing came; it never
   blocks for ever. A driver error is `TransportError::Io(<ESP-IDF error name>)`; a UART never
   closes, so `Closed` is not returned. `write` writes every byte or returns `Io`.
4. The core implements `RadioTransport` for `&mut T`: one UART, a fresh `Radio` per try (R4, R6).

### R4: The baud probe

1. `RADIO_BAUD` (115200) is the speed fixed in the core; `probe_order(RADIO_BAUD)` tries it first,
   then the others of 115200, 460800, 921600 in that order.
2. For each speed: the UART is set to it, a fresh `Radio` is identified (ASH RST, then EZSP).
   No RSTACK within `RESET_TIMEOUT_MS` (5000) logs `radio: no answer at <baud> baud` and the next
   speed is tried; any other failure stops the probe with its error (the speed answered). Found:
   `radio: uart <baud> baud`, then the core's `ezsp:` and `radio address:` lines.
3. Found at a speed other than `RADIO_BAUD`: `radio: answered at <baud> baud, RADIO_BAUD is <n>`
   (a warning), and the run goes on. No answer at all: `radio: no answer at any baud rate (115200,
   460800, 921600)`, the engine is not started, the ESP32 stays up (no reboot loop).
4. The speed found is recorded (`docs/hardware.md`); `RADIO_BAUD` is changed in E3c if it differs.
   The radio image is not flashed a second time in this spec: `flash` erases `storage`, the only
   copy of the network's keys once the box forms a network.

### R5: The network, its backup and the pairing window at boot

1. The radio's network is started as `--pair` starts it: resumed, or formed on channel 15. The
   random bytes come from the hardware generator with its entropy source on
   (`bootloader_random_enable`) around the forming, since no radio of the ESP32 runs.
2. When formed, the backup is written to `/data/zigbee.json` before the next step, in the host's
   format (the host's backup stays `network.json`; in the box `network.json` is the link setting).
   It is written with `storage::write_text`, made available to the radio image
   (`cfg(any(feature = "sim", feature = "radio"))`). A `zigbee.json` of another network is kept,
   renamed `zigbee-<PAN id>.json`, as the host keeps `network-<PAN id>.json`. The file holds the
   network and trust-centre keys in plain text on a flash that is not encrypted (secure boot and
   flash encryption are off): accepted until flash encryption is decided.
3. A marker file, `/data/booting`, is written at each boot and removed after `STABLE_MS` (10 min)
   of running. Pairing opens for `PAIR_AT_BOOT_SECONDS` (180) only when the reset is a power-on
   (power, or the USB bridge's reset) and no marker was there at boot: a crash, a watchdog, a
   brown-out or a boot loop opens nothing (`pairing: not opened at boot (<reason>)`). The rule is a
   pure core function (`pairing_at_boot`). The window goes through the home's pairing window (the
   home records the request, the loop opens it); then the box only listens. Lines: `pairing: open
   for 180 s` ... `pairing: closed`; the radio refusing to open joining logs `pairing: failed:`.
4. A setting refused while forming is logged as on the Mac (`the radio refused setValue (status
   0x70)`, journal of 2026-11-21; R8.4).

### R6: A wedged radio

1. `engine::run` tells a radio error from the others (`RunError::Radio` and `RunError::Other`, the
   same texts as today, so the host's output is unchanged). Only a radio error (link lost, an ERROR
   frame, a driver error) restarts; a storage error (`Saver::tick`) or a failed `measures` logs and
   stops the loop without resetting the EFR32.
2. The restart is a core function (`radio::serial::restart`) driven by closures: save what is due,
   end an open pairing phase (`finished`), run the reset sequence, probe, then `listen` (resume
   only, never `start_network`: never forms), never a pairing window. At most
   `MAX_RADIO_RESTARTS` (3) in `RESTART_WINDOW_MS` (10 min); past it: `radio: stopped after 3
   restarts in 10 min: <last error>`, and the ESP32 stays up. The box run does not exercise it
   (no safe way to wedge the radio): the core's tests and the review do.

### R7: What the image measures

Every `REPORT_EVERY_MS` (240 000) of the engine's time, the adapter logs two lines:

- `serial: <baud> baud, rx buffer 4096 B, highest <n> B, full <k> times`: the most bytes waiting in
  the receive ring, sampled before each read; "full" counted once each time it reaches its size;
- `heap: internal <n> KB free (min <m> KB), psram <p> KB free (min <q> KB), engine stack <s> KB
  unused of 32 KB`: minimums since boot, the stack's high-water mark. Formats and timing are in
  the core (tested); the adapter reads the numbers.

The home holds two or three devices, not the realistic home: the peak (free at boot minus the
minimums) says nothing against D27's 350 KB. It is recorded with the 32 KB stack and the 5 KB of
rings in the internal RAM table of `docs/memory-budget.md`; the gate is the 64 KB reserve (D23).

### R8: `scripts/box.sh`, and the run on the box

1. `--image box|sim|radio`. `flash --image radio` reads the ELF from `firmware/target/radio`, refuses
   a build without the radio image's marker (`radio: uart `) and names R1.1's command; `--image box`
   and `sim` refuse a radio build. Same checks and MAC confirmation as the other images. It ends
   without resetting the box (`--after no_reset`), and says to run `log --image radio`: the first
   boot, and its pairing window, is the one `log` captures.
2. `log --image radio` captures 300 s by default and shows the serial output live (the user sees
   the window open); it checks the banner, memory, storage and network lines, then `radio: uart`,
   `ezsp:`, a `network: formed|resumed` line, `pairing: open for` or `pairing: not opened`, and the
   `serial:` and `heap:` lines; it fails on `no answer at any baud rate`, `radio: stopped after`,
   `pairing: failed:` (named), `task_wdt`, a panic, a second banner (a reboot: GPIO15 may silence
   the ROM's `rst:` lines, so the banners are counted), and a `heap:` internal minimum below 64 KB.
3. Failure ladder on the box. No answer at any speed: stop, record it in `docs/hardware.md`, flash
   the box image back; the user decides. `psram none` or a panic once GPIO17 drives TX: flash the
   box image back, record the GPIO17 conflict, stop (no other TX pin is wired). A wedged radio:
   R6; still wedged after three restarts, record it.
4. Recorded from the run: in `docs/hardware.md` the baud, GPIO17 and the PSRAM; in the roadmap's
   journal the network formed or resumed, and the 0x70 outcome (seen, not seen, or not formed).
5. The box ends E3b on the radio image (E3c goes on from it); restoring the original firmware is
   optional, on the user's request.

## Living spec delta

### docs/living-spec/README.md

- Changed, "Talks to the radio": also over the serial link in the radio image (R2 to R6).
- Changed, "Pairs a device": the radio image's window at boot (R5.3); backup `/data/zigbee.json`,
  keys in plain text.
- Changed, "Starts and shows its name": the radio image's lines (R4, R5, R7).
- Changed, "Stays within a memory budget": the radio image's `heap:` line and its 64 KB floor; the
  stale "once a box has run (none has)" → the box's figures are in `docs/memory-budget.md`.
- Changed, "Flashes the box and brings it back": `--image radio` and its checks (the list of `log`'s
  checks, about line 440); GPIO12 and GPIO15 named only in `firmware/src/radio_uart.rs`; the box
  and measuring images leave the radio untouched.
- History: row 021.

### No longer true

- `docs/living-spec/README.md`: "No file of `firmware/src/` names GPIO12 or GPIO15 ...: the measuring
  image leaves the radio untouched" → R2.3; "talks to the radio to identify it ..." (the host only)
  → the host and the radio image; "(none has)", line 483 → the box has run.
- `crates/core/src/radio/transport.rs:2` "will implement it over the UART", `firmware/Cargo.toml:24`
  "no radio yet", `firmware/src/main.rs:2` "starts no radio", `crates/host/tests/box_script.rs:7`
  and its "the radio's pins are not driven", `scripts/lib/firmware.sh:99` and `box.sh`'s usage
  (`box|sim`), `docs/architecture.md:42` (the firmware row) → the radio image, in the present.
- `docs/architecture.md:57-60` "without them, the image is the box's" → names `radio` and its
  settings file; `docs/architecture.md:80` "The box stores no data" → the box keeps the engine's
  files, `zigbee.json` and its marker in `/data`.
- `docs/hardware.md:5-7` "its rows stay as they are until a run ... settles them" and ~98 "The
  firmware codes no pin until the line it concerns is settled" → the pins the radio image drives,
  settled by the run; the TX and baud rows; step 7 "Restore the original firmware at the end" →
  optional, on the user's request.
- `docs/memory-budget.md:109,150`: "the radio's buffers the radio (E3b)", "needs the radio (E3b)" →
  the figures.
- `docs/decisions.md` D26 ("starts no radio"), D31 ("stays wired but not driven"): history, not
  edited; D32 records the change.
- Searched: "radio: none", "not driven", "starts no radio", "no radio yet", "GPIO12", "over the
  UART", "serial link", "box|sim", "stores no data", "codes no pin", "none has", "at the end".

## Acceptance criteria

- [x] `cargo test --workspace` green, the pin test included (only `radio_uart.rs` names GPIO12, 15).
- [x] `sh scripts/validate.sh` green: four images built and linted; `qemu.sh` and `--box` unchanged.
- [ ] On the box: `log --image radio` green; the baud found recorded. Baud recorded (115200, 2026-11-22); the capture was 4 minutes of the 10 planned (the user stopped it), so the script's verdict on a full capture was not run.
- [x] On the box: the SNZB-03PR2 paired at boot and its motion logged (2026-11-22). The ZBMINIR2 bonus was not run.
- [ ] `docs/memory-budget.md` and `docs/hardware.md` hold the run's figures (done); the journal the 0x70: no network was formed (the box resumed another program's), so the 0x70 outcome is not answered and stays open in the roadmap.

## Edge cases

- Wrong speed: no RSTACK, garbage dropped by ASH's CRC, next speed (15 s at worst). Silent at every
  speed: R4.3, no reboot loop. A driver error: R6.
- The receive ring full (a long stall): bytes lost, ASH NAKs and the radio resends; counted in `full`.
- The radio in a network the box has no backup of (formed from the Mac in E2): resumed, `network:
  resumed (made by another program)`, never destroyed; its devices join again to enter the store.
- A `zigbee.json` of another network: kept as `zigbee-<PAN id>.json` (R5.2).
- The old `zigbee.json` cannot be renamed: nothing is written over it, the boot stops with the error.
- An unwanted device joins during a window: it stays in the store (no removal yet in the API: a
  roadmap inbox line).
- The radio refuses to open joining: `pairing: failed:`, `log` fails and names it.
- A crash, watchdog or brown-out reboot, or a reboot within 10 min: no window (R5.3); to pair again
  the user waits 10 min after the last boot, then resets (`log`).
- No real-time clock: dates are 1970 plus the uptime (SNTP comes with the network, E3c).
- A storage error: the loop stops, the EFR32 is not reset (R6.1).
- The engine stack too small: a stack overflow panic, `log` fails; the stack is raised and recorded.
- `radio` and `sim` both on: compile error.

## Sowel contract

None: nothing exposed is added; the files in `/data` are the host's, in Sowel's format.

## Decided in autonomy

- Pairing at boot, 180 s: no input path to build; a USB console command shares the log's port, the
  button (GPIO34) has no known pull-up. Bounded by the marker (review) and the reset reason: a
  crash after 10 min of stable running opens nothing either. `flash --image radio` leaves the box
  in download mode (R8.1), so the first boot is `log`'s and its window is not lost to the marker
  of a boot nobody watched. The pairing route stays off
  in the box (D28) until J4; the image is a development image, in no release before J4.
- The probe stays, the fixed speed first; no second flash here (R4.4): the found speed is recorded
  and `RADIO_BAUD` follows in E3c.
- One `engine` thread for radio and engine, on core 1 (core 0 for Wi-Fi in E3c).
- 4 KB of receive ring: above the radio's ASH window (7 frames of at most about 450 stuffed bytes),
  past which the radio waits for an acknowledgement.
- `CONFIG_UART_ISR_IN_IRAM` in a radio-only settings file and target folder (R1.1): the measured
  images keep their settings; the radio image's ESP-IDF is built once more, in its own folder.
- The Zigbee backup is `/data/zigbee.json`; its text and PAN check move from the host to the core,
  so both write the same format. Keys in plain text accepted (user, 2026-11-22) until flash
  encryption is decided (roadmap inbox).
- A restart resumes only and never opens a window; only radio errors restart (R6).
- No leave or re-forming: the 0x70 question is answered only if the run forms a network.
- The box ends on the radio image; the original firmware's restore is optional (user, 2026-11-22).
- Decision D32 records the UART, the probe, the pins, the boot window, the keys and the pin test.

## How the user will check it

On the Dongle Max on USB, the user's yes and MAC for the flash: `sh scripts/box.sh flash --image
radio --backup <file>`, then `sh scripts/box.sh log --image radio --seconds 600`. When `pairing:
open for 180 s` shows, the user puts the SNZB-03PR2 in pairing mode (long press), then the
ZBMINIR2; the log shows `joined:`, `device:`, `reporting:` and the measures; the user moves before
the sensor and toggles the plug by hand.

- [x] `radio: uart <baud> baud` and the radio's address (2026-11-22, 115200 baud, its address (not recorded here))
- [x] the SNZB-03PR2 paired and its motion seen (2026-11-22; joined 0.75 s after the window opened, motion true twice)
- [ ] bonus: the ZBMINIR2 paired and its state seen: not run in this session
- [x] the `serial:` and `heap:` lines written in `docs/memory-budget.md` (2026-11-22, after 4 minutes, not the 10 planned)
- [ ] the network formed or resumed, and the 0x70 outcome, in the journal: resumed (made by another program, one channel and PAN, not recorded here), recorded; no forming happened, so the 0x70 outcome is not answered. Also not run: a second boot, the booting-marker and power-cut cases, the restart ladder
