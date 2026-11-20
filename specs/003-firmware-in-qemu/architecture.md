# Architecture: Spec 003

## Overview

```
crates/core  memory_line(MemoryStats) -> String          (pure, tested on the Mac)
      ▲
firmware/src/memory.rs  reads ESP-IDF heap sizes  ──►  MemoryStats
firmware/src/main.rs    banner, then memory_line(...)  ──►  serial log

scripts/qemu.sh
  cargo build --release (firmware)
  esptool elf2image + merge_bin  ──►  16 MB flash image (in firmware/target/, ignored by git)
  qemu-system-xtensa -machine esp32 -m 2M -drive file=flash,if=mtd,format=raw -serial file:log
  watch the log: banner + memory line → print, exit 0
                 panic / second boot / 60 s → last 40 lines, exit 1
  QEMU always killed on exit (trap)
```

## Data

| Name | Type | Unit | Where |
| ---- | ---- | ---- | ----- |
| `MemoryStats` | struct: `internal_free`, `internal_largest`, `psram: Option<PsramStats { free, total }>` (`None` when absent: PSRAM is fully there or not) | bytes (`usize`) | `crates/core/src/memory.rs` |

Nothing is stored; nothing grows.

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/src/memory.rs`, `crates/core/src/lib.rs` | New: `MemoryStats`, `memory_line` |
| `crates/core/tests/memory.rs` | New: tests of `memory_line` |
| `firmware/src/memory.rs` | New: reads the heap sizes from ESP-IDF (`heap_caps_get_free_size`, `heap_caps_get_largest_free_block`, `heap_caps_get_total_size` with `MALLOC_CAP_INTERNAL` and `MALLOC_CAP_SPIRAM`) |
| `firmware/src/main.rs` | Logs the memory line after the banner |
| `firmware/Cargo.toml` | `unsafe_code = "deny"` |
| `firmware/sdkconfig.defaults` | Flash 16 MB; PSRAM on (`CONFIG_SPIRAM`, `CONFIG_SPIRAM_USE_MALLOC`, `CONFIG_SPIRAM_IGNORE_NOTFOUND`) |
| `scripts/qemu.sh` | New: R3 |
| `scripts/validate.sh` | Step "Firmware: boots in QEMU" |
| `.github/workflows/ci.yml` | QEMU's system libraries on Ubuntu (`libgcrypt20 libglib2.0-0 libpixman-1-0 libsdl2-2.0-0 libslirp0`) |
| `docs/living-spec/README.md` | The delta |
| `docs/architecture.md`, `README.md`, `CLAUDE.md` | The QEMU run and command |
| `docs/decisions.md` | D11: `unsafe` in the firmware's adapters only |

## Reused

- The QEMU run tried on 2026-11-20 (machine `esp32`, flash as `mtd`, serial to a file), and the
  `esptool` of the ESP-IDF Python environment in `firmware/.embuild/espressif/python_env/`.
- The banner pattern of spec 001: the core formats, the firmware logs.
