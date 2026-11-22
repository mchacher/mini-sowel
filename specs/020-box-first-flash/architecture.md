# Architecture: Spec 020

## Overview

```
 Mac ── USB (CP2102) ── Dongle Max's ESP32, in download mode
  scripts/box.sh ─┬─ identify ── esptool flash_id, espefuse summary ──► STOP (exit 3) if unfit
                  ├─ backup ──── checks, MAC prompt (/dev/tty), read_flash ×2, compare, SHA-256
                  │              ──► files (0600) outside the repository
                  ├─ restore ─── checks, MAC prompt, write_flash 0x0 ──► original firmware
                  ├─ flash ───── --backup: SHA-256 in docs/hardware.md with a restore date, MAC;
                  │              checks, MAC prompt, erase nvs/otadata/storage,
                  │              bootloader + table (partitions.csv) + app at ota_0
                  └─ log ─────── reset, pyserial capture ──► checks of scripts/lib/firmware.sh
                                 (rst: counted after the first banner; --expect-kept)
  scripts/qemu.sh ── sources scripts/lib/firmware.sh (same tools, table, checks)

 firmware, feature flash-saves (⊃ sim): RunStorage writes every save, timed ──► `saves:` line
```

## Data

| Name | Type, unit | Where |
| ---- | ---------- | ----- |
| `dongle-max-<mac>-<date>.bin` | 16 777 216 bytes, the whole flash, mode 0600 | the user's folder, outside the repository |
| `<backup>.sha256` | `shasum -a 256` line, mode 0600 | beside the backup |
| `serial-<date>.log`, build and tool logs | text | `$TMPDIR/solite-box/` |
| Backup table of `docs/hardware.md` | `Date`, `Chip revision`, `Firmware version`, `SHA-256 of the backup`, `Restore proven` (spec R8.3) | repository (`SOLITE_HARDWARE_MD` in the tests); `flash` reads columns 4 and 5 |
| `saves:` line's counters | count, longest (ms, file, KB), total (ms) | firmware, `flash-saves` only; three numbers, no list |

## Files changed

| File | Change |
| ---- | ------ |
| `scripts/box.sh` | New: the five commands of R1 to R7 |
| `scripts/lib/firmware.sh` | New: tool lookup (`PATH`, then `firmware/.embuild`), table build and `ota_0` offset, bootloader lookup, `elf2image --flash_mode dio --flash_size 16MB` and the `.bin` checked against the slot, boot-log checks; moved out of `scripts/qemu.sh` |
| `scripts/qemu.sh` | Sources the library; behaviour and messages unchanged |
| `scripts/validate.sh` | Clippy of the `flash-saves` image beside the two others |
| `firmware/Cargo.toml` | Feature `flash-saves = ["sim"]` |
| `firmware/src/simulated.rs` | With `flash-saves`: every save to the flash, timed; the `saves:` line |
| `crates/host/tests/box_script.rs` | New: `scripts/box.sh` with fake tools on `PATH` (as `demo_script.rs` tests its script), the prompt answered through a pseudo-terminal; the GPIO12 and GPIO15 check of `firmware/src/` |
| `firmware/.cargo/config.toml` | The `espflash` runner removed |
| `firmware/sdkconfig.defaults` | Comment of line 27: the table reaches the box through `scripts/box.sh flash` |
| `docs/living-spec/README.md` | Delta of the spec |
| `docs/hardware.md`, `docs/memory-budget.md` | Measured facts, backup row, procedure through `box.sh` |
| `docs/decisions.md` | D31 |
| `docs/architecture.md`, `README.md`, `CLAUDE.md` | What the spec makes false |
| `docs/roadmap.md` | E3 split into E3a to E3d; inbox: the stall line removed, the 33 requests sent to E3c |

## Reused

- `scripts/qemu.sh`'s tool lookup, table build, bootloader choice and line checks, moved to the
  library rather than copied.
- `firmware/src/simulated.rs`'s `RunStorage`, `seed` and `run`; the realistic home of
  `firmware/fixtures/`.
- `crates/host/tests/demo_script.rs`'s way of running a script in a test (temp folder dropped at
  the end).
