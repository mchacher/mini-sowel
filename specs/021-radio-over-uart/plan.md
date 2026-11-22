# Plan: Spec 021

## Steps

- [x] 1. Living spec updated (spec's delta) and the "No longer true" sentences rewritten; the living
      spec names no roadmap increment (`check-docs.sh` refuses them).
- [x] 2. Core: `radio/serial.rs` (baud order and probe, pin roles and reset sequence,
      `pairing_at_boot`, `SerialStats`, `Restarts` and `restart` driven by closures, their lines)
      with its tests; `impl RadioTransport for &mut T`; `engine::run` returns `RunError` (radio or
      other, same texts; host updated, its output unchanged).
- [x] 3. Core: `radio/backup.rs` (text, `is_network` and a backup's PAN, moved from
      `crates/host/src/backup.rs`); the host uses it, its tests unchanged and green; the host's
      file stays `network.json`, the box's is `zigbee.json`, same format.
- [x] 4. Core: `memory::heap_line`.
- [x] 5. Firmware: feature `radio` and its `compile_error!` with `sim`; `sdkconfig.radio.defaults`
      (`CONFIG_UART_ISR_IN_IRAM=y`), read through `ESP_IDF_SDKCONFIG_DEFAULTS` with
      `CARGO_TARGET_DIR=target/radio` (spec R1.1), `sdkconfig.defaults` untouched; `storage.rs`'s
      `write_text` for `sim` or `radio`; `radio_uart.rs`, `radio_engine.rs`, `main.rs`. Built,
      clippy clean.
- [x] 6. `scripts/box.sh` and `scripts/lib/firmware.sh`: `--image radio` (ELF from `target/radio`,
      `--after no_reset`, live output, the checks of R8.2); `scripts/validate.sh` builds and lints
      the fourth image with its folder and settings.
- [x] 7. `crates/host/tests/box_script.rs`: the pin test rewritten deliberately, the radio cases.
- [x] 8. Decision D32 in `docs/decisions.md`:
      "D32: The radio over the UART in a radio image, pairing at boot. Date 2026-11-22. Choice: a
      fourth image (feature `radio`) drives the EFR32 over UART1 (TX GPIO17, RX GPIO13, 8N1, no
      flow control), a 4 KB receive ring and the interrupt in internal RAM; the radio and the engine
      share one thread on core 1 with 32 KB of stack. The interrupt setting is in
      `sdkconfig.radio.defaults`, built in `target/radio`, so the other images keep their settings.
      The speed is probed at each boot, the fixed `RADIO_BAUD` first, then 115200, 460800, 921600.
      The core describes the pins by role (reset active low, bootloader active high, never set
      active) and the reset sequence; the firmware runs it after the ESP32's boot, each level
      written before the pin becomes an output, GPIO12 low before GPIO15 (both strapping pins).
      GPIO12 and GPIO15 are named in `firmware/src/radio_uart.rs` only (a test). Pairing opens for
      180 s at a power-on boot when the marker `/data/booting` (removed after 10 minutes of
      running) is absent: a crash, a watchdog, a brown-out or a boot loop opens nothing. The Zigbee
      backup is `/data/zigbee.json`, the network and trust-centre keys in plain text on a flash
      that is not encrypted, accepted until flash encryption is decided. Only a radio error
      restarts the radio (reset, probe, resume, never a forming nor a window), at most 3 times in
      10 minutes; a storage error stops the loop. The radio image is a development image, in no
      public release before J4. Replaces D31's 'the EFR32 radio stays wired but not driven' and
      D26's 'without it the image is the box's and starts no radio' for the radio image. Why: the
      simplest bounded way to pair without a network (a console command shares the log's port,
      the button's pull-up is unknown); the probe finds a radio at another speed for one try's
      cost; the ring holds more than the radio's ASH window, and the interrupt keeps the FIFO
      (11 ms at 115200) drained while a flash write stalls the caches; logic in the core is
      tested on the Mac."
- [x] 9. Docs: `docs/architecture.md` (lines 42, 57-60, 80), `README.md`, `CLAUDE.md` (radio build
      and `box.sh` lines); `docs/hardware.md` (lines 5-7 and ~98, GPIO15 a strapping pin, step 7
      optional); `docs/roadmap.md` inbox lines: the pairing window at boot replaced by the button or
      the authenticated route (J4); a device removal in the API (an unwanted device that joined
      stays); flash encryption for the keys of `zigbee.json`; `RADIO_BAUD` set in E3c if the run
      found another speed.
- [x] 10. Validate command green (`sh scripts/quiet.sh sh scripts/validate.sh`).
- [x] 11. (Run 2026-11-22. Gaps: capture of 4 minutes of the 10 planned, stopped by the user; no second boot, booting-marker or power-cut case, restart ladder, forming of a network (the network was resumed, so the 0x70 outcome is open) nor ZBMINIR2; `zigbee.json` on the box not checked.) On the box (the user's yes and MAC for the flash; Claude runs `identify` and `log` only):
      flash the radio image once, `log --image radio --seconds 600`, the user pairs the SNZB-03PR2
      (the ZBMINIR2 a bonus). No second flash (it erases the keys in `storage`): the speed found is
      recorded in `docs/hardware.md` with the UART TX row, GPIO17 and the PSRAM, the pins driven;
      `docs/memory-budget.md`: "The serial link's buffers", and in the internal RAM table the 32 KB
      stack, the 5 KB of rings and the peak (free at boot minus the minimums, two or three
      devices: nothing against D27); journal: network formed or resumed, the 0x70 outcome; the
      failure ladder of R8.3 if a step fails. The box ends on the radio image; the original
      firmware is restored only if the user asks.
- [x] 12. Spec's "How the user will check it" ticked with the run (date, models, figures).

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| core `radio::serial` | `probe_order(115200)` | 115200, 460800, 921600 |
| core `radio::serial` | `probe_order(921600)` | 921600, 115200, 460800 |
| core `radio::serial` | `probe_order` of a speed not in the list | the three in list order, no duplicate |
| core `radio::serial` | Probe, answer at the second speed | that speed; `no answer at 115200 baud` then `radio: uart 460800 baud`; the third never tried |
| core `radio::serial` | Probe, no answer at any speed | none; three `no answer` lines and `no answer at any baud rate (115200, 460800, 921600)` |
| core `radio::serial` | Probe, a failure other than no reset answer (driver error, EZSP version) | stops at that speed with the error; no further speed tried |
| core `radio::serial` | Probe, found at a speed other than `RADIO_BAUD` | the warning naming both speeds; the found speed returned |
| core `radio::serial` | Reset sequence order | bootloader set inactive before reset set active; reset released last, then the boot wait |
| core `radio::serial` | Reset sequence, bootloader | never set active in any step |
| core `radio::serial` | Reset sequence, timings | pulse ≥ `RESET_PULSE_MS`, final wait ≥ `BOOT_WAIT_MS` |
| core `radio::serial` | Pin levels | reset active is low, bootloader active is high; inactive the opposite |
| core `radio::serial` | `SerialStats`, highest | the largest sample kept, smaller ones ignored |
| core `radio::serial` | `SerialStats`, full | counted once per reach of the capacity, again only after it drops below |
| core `radio::serial` | `SerialStats`, report timing | none before 240 s; one at 240 s, the next at 480 s; the `serial:` line format |
| core `radio::serial` | `Restarts` | 3 allowed in 10 min, the 4th refused with its line; allowed again once the first is 10 min old |
| core `radio::serial` | `pairing_at_boot`, first boot (no marker, power-on) | open, 180 s |
| core `radio::serial` | `pairing_at_boot`, normal reboot (marker removed, power-on) | open |
| core `radio::serial` | `pairing_at_boot`, marker present (boot within 10 min, boot loop) | not opened, the reason line |
| core `radio::serial` | `pairing_at_boot`, panic, watchdog or brown-out reset without marker | not opened, the reason line |
| core `radio::serial` | Marker removal | due at 600 s of running, not before |
| core `radio::serial` | `restart`, order | save, pairing finished, reset sequence, probe, listen, in that order |
| core `radio::serial` | `restart`, never forms | the `listen` closure is called, never a forming one; a radio with no network ends with its error |
| core `radio::serial` | `restart`, never opens a window | no pairing request after it; an open phase ends `finished` |
| core `radio::serial` | `restart`, probe finds nothing | `no answer at any baud rate`, the loop not run again |
| core `engine` | A storage error in `Saver::tick` | `RunError::Other`, no restart closure called |
| core `engine` | A radio error in a poll | `RunError::Radio`; its text the same as today's |
| host | Existing output on an engine error | the same lines as before (host tests green) |
| core `radio::transport` | `&mut T` transport reused | two `Radio`s in turn on one fake transport both identify |
| core `radio::backup` | Text of a formed network | the host's format byte for byte (PAN, extended PAN, channel, keys) |
| core `radio::backup` | `is_network` | true for the same PAN and extended PAN, false for another |
| core `radio::backup` | PAN of another network's backup text | its PAN id, for the name `zigbee-<PAN id>.json` (the host's `network-<PAN id>.json` unchanged) |
| host `backup` | Existing behaviour unchanged | the host's backup tests green (file, mode 0600, other backup kept) |
| core `memory` | `heap_line` | the `heap:` format, KB rounded as `memory_line` |
| `box_script.rs` | Pin checker on a temporary tree | a file other than `radio_uart.rs` naming GPIO15 is reported |
| `box_script.rs` | Pins in the real `firmware/src/` | GPIO12 and GPIO15 named in `radio_uart.rs`, in no other file |
| `box_script.rs` | `flash --image radio` with a sim build | refused before the prompt, names `--features radio` |
| `box_script.rs` | `flash --image box` with a radio build | refused before the prompt |
| `box_script.rs` | `--image` other than box, sim, radio | exit 2, the three named |
| `box_script.rs` | `log --from-file --image radio`, good log | green; prints the radio, `ezsp:`, network, pairing, `serial:` and `heap:` lines |
| `box_script.rs` | Radio log with `no answer at any baud rate` | exit 1, the reason and the last lines |
| `box_script.rs` | Radio log with `radio: stopped after` | exit 1 |
| `box_script.rs` | Radio log with `task_wdt` | exit 1 |
| `box_script.rs` | Radio log with `pairing: failed:` | exit 1, names the refused opening |
| `box_script.rs` | Radio log with `pairing: not opened at boot` | green; the line printed |
| `box_script.rs` | Radio log with two banners and no `rst:` line | exit 1, a reboot |
| `box_script.rs` | `flash --image radio` | ELF read from `target/radio`; esptool called with `--after no_reset`; the prompt still asked |
| `box_script.rs` | Radio log with the `heap:` internal minimum at 63 KB | exit 1, names the 64 KB reserve |
| `box_script.rs` | Radio log without a `serial:` line (capture too short) | exit 1, names the line and `--seconds` |
| `box_script.rs` | Radio log with a panic after the banner | exit 1 (existing check) |
| `box_script.rs` | `log --image box` and `sim` | existing tests green, unchanged |
| firmware build | `--features radio`, and `radio` with `sim` | builds and lints clean; the pair is a compile error |
| firmware build | The box, sim and measuring images | built without `sdkconfig.radio.defaults` (no UART interrupt setting in their `sdkconfig`) |
| `scripts/qemu.sh` | The sim and box images | as before (QEMU has no EFR32 on its UART: the radio image is not run there) |
| box (run) | First boot of the radio image | `radio: uart <baud> baud`, `ezsp:`, radio address, PSRAM still found (GPIO17 settled) |
| box (run) | Pairing at boot | the SNZB-03PR2 joins, its motion in the log; bonus: the ZBMINIR2 and its state |
| box (run) | An unwanted device joins in the window | not provoked; it stays in the store (inbox line), covered by the review |
| box (run) | Network | formed (0x70 seen or not) or resumed; recorded |
| box (run) | Figures | `serial:` and `heap:` lines; internal minimum ≥ 64 KB; written to the docs |

## Validation

- [x] Validate command green
- [ ] Agent review done, blocking findings fixed
- [ ] Demo seen and approved by the user
