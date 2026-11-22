# Architecture: Spec 021

## Overview

```
 firmware, feature radio
  main (core 0): banner, memory, storage, network setting ──► spawns `engine` (core 1, 32 KB)
  engine thread
   ├─ radio_uart.rs: run core::radio::serial::reset_sequence() on GPIO12 (low) then GPIO15 (pulse)
   ├─ UART1 (TX 17, RX 13), rx ring 4 KB, ISR in IRAM ──► UartTransport: RadioTransport
   ├─ core::radio::serial::probe(probe_order(RADIO_BAUD), try) ──► Radio::new(&mut uart).identify()
   ├─ Radio::start_network (on_formed ──► /data/zigbee.json, core::radio::backup text, write_text)
   ├─ pairing_at_boot(marker /data/booting, reset reason)? ──► home.pairing_mut().request(180)
   ├─ core::engine::run(radio, home, saver, ..., sink = log); /data/booting removed at 10 min
   │      every read: SerialStats::observe(buffered) ; every 240 s: `serial:` and `heap:` lines
   ├─ Err(RunError::Other) ──► logged, loop stopped, EFR32 untouched
   └─ Err(RunError::Radio) ──► core::radio::serial::restart(closures): save, pairing finished,
          reset_sequence, probe, listen (never forms, no window); past 3 in 10 min:
          `radio: stopped after 3 restarts in 10 min: <e>`
 EFR32MG24 (EmberZNet NCP) ◄── ASH / EZSP over the UART ──► Zigbee devices
 Mac: scripts/box.sh flash --image radio (MAC prompt, ELF from target/radio, no reset at the end)
      scripts/box.sh log --image radio (resets: first boot, live output, radio checks)
```

## Data

| Name | Type, unit | Where |
| ---- | ---------- | ----- |
| `RADIO_BAUD` | `u32`, baud; 115200 (the speed found is recorded; changed in E3c if it differs) | `crates/core/src/radio/serial.rs` |
| `BAUD_RATES` | `[u32; 3]`: 115200, 460800, 921600 | same |
| `RadioPin`, `PinStep` | `Reset` (active low), `Bootloader` (active high); `Set(pin, active)`, `Wait(ms)` | same |
| `RESET_PULSE_MS`, `BOOT_WAIT_MS` | 100 ms, 1000 ms | same |
| `SerialStats` | capacity (B), highest (B), times full, next report (ms) | same; one per UART |
| `REPORT_EVERY_MS` | 240 000 ms | same |
| `Restarts`, `MAX_RADIO_RESTARTS`, `RESTART_WINDOW_MS` | the times of the last restarts (at most 3), 3, 600 000 ms | same |
| `PAIR_AT_BOOT_SECONDS`, `STABLE_MS`, `pairing_at_boot` | 180 s; 600 000 ms; (marker present, reset reason) → open or the reason it is not | `crates/core/src/radio/serial.rs` |
| `booting` | empty marker file, written at boot, removed after `STABLE_MS` | `/data` (FAT) |
| `RunError` | `Radio(RadioError)` or `Other(String)`, displayed as the texts of today | `crates/core/src/engine/mod.rs` |
| `RX_RING_BYTES`, `TX_RING_BYTES` | 4096 B, 1024 B, internal RAM | `firmware/src/radio_uart.rs` |
| Engine thread stack | 32 KB, internal RAM, core 1 | `firmware/src/radio_engine.rs` |
| `zigbee.json` | the host's backup text (PAN, extended PAN, channel, network and trust-centre keys in plain text, flash not encrypted), written when a network is formed; another network's kept as `zigbee-<PAN id>.json` | `/data` (FAT); `network.json` stays the link setting; the host's backup stays `network.json` |
| `sdkconfig.radio.defaults` | `CONFIG_UART_ISR_IN_IRAM=y` | `firmware/`, read for the radio image only, with `CARGO_TARGET_DIR=target/radio` |
| `serial:` and `heap:` lines | text, see spec R7 | serial log |

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/src/radio/serial.rs` | New: baud order and probe, pin roles and reset sequence, `pairing_at_boot`, `SerialStats`, `Restarts` and `restart` (closures), the lines of R4, R5.3, R6 and R7; unit tests beside it |
| `crates/core/src/engine/mod.rs` | `run` returns `RunError` (radio or other), same texts |
| `crates/core/src/radio/transport.rs` | `impl RadioTransport for &mut T`; header in the present |
| `crates/core/src/radio/backup.rs` | New: the backup text, `is_network` and the PAN of a backup's text, moved from the host (format unchanged) |
| `crates/core/src/radio/mod.rs` | The two modules declared and exported |
| `crates/core/src/memory.rs` | `heap_line` (R7) beside `memory_line` |
| `crates/core/tests/radio_serial.rs` | New: the probe with a fake `try`, the `&mut T` transport reused by two `Radio`s |
| `crates/host/src/backup.rs`, `crates/host/src/main.rs`, `serve.rs` | Use the core's text and check (file writing and mode 0600 stay); `RunError` shown as before |
| `firmware/Cargo.toml` | Feature `radio`; comments in the present |
| `firmware/sdkconfig.radio.defaults` | New: `CONFIG_UART_ISR_IN_IRAM=y`, radio image only; `sdkconfig.defaults` unchanged |
| `firmware/src/storage.rs` | `write_text` under `cfg(any(feature = "sim", feature = "radio"))`; a rename helper for `zigbee-<PAN id>.json` |
| `firmware/src/radio_uart.rs` | New (feature `radio`): pins (level written before output: GPIO12 low, then GPIO15 high), UART1 driver, `UartTransport`, serial stats; the only file naming GPIO12, 13, 15, 17; `unsafe` only for ESP-IDF calls without a safe wrapper, each with `SAFETY` (D11) |
| `firmware/src/radio_engine.rs` | New (feature `radio`): the engine thread (core 1), probe, network and `zigbee.json`, the marker and the reset reason (`esp_reset_reason`) for the boot window, `engine::run`, the closures of `restart`, the `heap:` reading (minimum free, stack high-water mark), `bootloader_random_enable` around a forming |
| `firmware/src/main.rs` | `run_engine` for `radio`; `compile_error!` with `sim`; header |
| `scripts/box.sh` | `--image radio` for `flash` (ELF from `target/radio`, marker check, `--after no_reset`) and `log` (300 s, live output, radio checks) |
| `scripts/lib/firmware.sh` | `fw_missing_line` and the radio image's checks (`fw_check_radio`: probe, stopped, `pairing: failed:`, `task_wdt`, a second banner, `heap:` minimum) |
| `scripts/validate.sh` | Builds and lints the radio image with R1.1's folder and settings |
| `crates/host/tests/box_script.rs` | Pin test: GPIO12 and GPIO15 named in `radio_uart.rs` and nowhere else in `firmware/src/`; the radio image's `flash` and `log` cases |
| `docs/living-spec/README.md` | Delta of the spec |
| `docs/decisions.md` | D32 |
| `docs/hardware.md`, `docs/memory-budget.md` | The run's facts and figures; GPIO15 named a strapping pin; step 7 optional; the internal RAM table gains the engine stack, the rings and the measured peak |
| `docs/architecture.md`, `README.md`, `CLAUDE.md` | The radio image (row, lines 57-60, line 80, build and `box.sh` command lines) |
| `docs/roadmap.md` | Journal of the run, the baud and the 0x70 outcome; inbox: the boot window replaced by the button or the authenticated route, a device removal in the API, flash encryption for `zigbee.json`'s keys, `RADIO_BAUD` set in E3c if it differs |

## Reused

- `solite_core::engine::run`, `Saver`, `load_home`, the pairing window of the home (spec 017).
- `Radio::identify`, `start_network`, `network_line`, `identity_lines`.
- `firmware/src/storage.rs` (`FatStorage`, `write_text`), `memory.rs`, `measure.rs`'s reading
  style; `simulated.rs`'s thread start.
- `box.sh`'s `flash` and `log` paths, prompt and checks (spec 020).
