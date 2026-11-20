# Plan: Spec 003

## Steps

- [x] 1. Living spec updated (delta of spec.md)
- [x] 2. Core types: `MemoryStats`
- [x] 3. Core logic: `memory_line`
- [x] 4. Adapters: the firmware reads the heap sizes; settings in `sdkconfig.defaults`
- [x] 5. API: not applicable
- [x] 6. Interface: not applicable
- [x] 7. `scripts/qemu.sh`; validate step; CI libraries
- [x] 8. Tests of the test plan; each seen failing once
- [x] 9. Docs: architecture, README, CLAUDE.md, decision D11; "No longer true" search

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| core `memory_line` | Internal and PSRAM sizes in bytes | `memory: internal 245 KB free (largest block 110 KB), psram 2039 KB free of 2048 KB` |
| core `memory_line` | No PSRAM (`None`) | `... psram none` |
| core `memory_line` | Sizes not a multiple of 1024 | rounded down (1023 bytes → 0 KB) |
| core `memory_line` | Zero everywhere | zeros, no panic |
| core banner | Existing banner test | unchanged, passes |
| `scripts/qemu.sh` | Normal firmware | banner and memory line printed, PSRAM total at least 2000 KB, chip revision 3 accepted, exit 0 |
| `scripts/qemu.sh` | Reset loop (a temporary `esp_restart()` after the banner) | second `rst:` seen, exit non-zero, last lines shown |
| `scripts/qemu.sh` | Hang (a temporary endless loop before the banner) | exit non-zero after 60 s, last lines shown |
| `scripts/qemu.sh` | QEMU started without PSRAM (temporary removal of `-m`) | `psram none`, exit 0 |
| `scripts/qemu.sh` | A Homebrew library missing (simulated by a temporary change of the check) | the install command printed, exit non-zero |
| `scripts/qemu.sh` | Ctrl-C during the run (SIGINT sent to the script) | no `qemu-system-xtensa` process left |
| `scripts/qemu.sh` | Firmware that panics at start (a temporary `panic!` in main, not committed) | exit non-zero, last lines with the panic shown |
| `scripts/qemu.sh` | QEMU binary missing (temporary rename) | installed, then run |
| `scripts/qemu.sh` | After any run | no `qemu-system-xtensa` process left |

The script scenarios are run by hand once each (shell script, no test harness), their result
written in this plan; the nominal one runs in every `sh scripts/validate.sh`.

### Results of the script scenarios (2026-11-20, by hand)

- Core `memory_line` tests: 4 tests; each broken once (`/ 1000`, `"n/a"` for no PSRAM, rounding up,
  changed wording): the tests failed each time, then passed once restored.
- Normal: exit 0 in about 6 s. Banner `mini-sowel 0.1.0` and `memory: internal 362 KB free (largest
  block 160 KB), psram 2045 KB free of 2048 KB`. Bootloader log: `chip revision: v3.0`, accepted
  with `CONFIG_ESP32_REV_MIN_3=y`. Without the eFuse image (revision 0), the bootloader refuses
  (`Required >= v3.0, found v0.0`) and the script fails: the eFuse image is what makes it pass.
- Reset loop (temporary `esp_restart()` after the memory line): exit 1, "the firmware panicked or
  rebooted", last 40 lines shown (second boot). The 3 s grace after the banner catches it.
- Panic (temporary `panic!` at start): exit 1, `abort() was called` shown in the last lines.
- Hang (temporary endless loop before the banner): exit 1 after 62 s, last lines shown.
- No PSRAM (`-m 2M` removed): `psram none`, exit 0 (ESP-IDF logs `PSRAM enabled but initialization
  failed` and boots on).
- Homebrew library missing (bogus name added to the list): `brew install libnotinstalled ...`
  printed, exit 1.
- Ctrl-C: SIGINT sent to the script (default disposition, as from a terminal) while QEMU ran: exit
  130, no `qemu-system-xtensa` process left. (A `&` background job of `sh` ignores SIGINT: the test
  must give the script the default disposition, e.g. `perl -e '$SIG{INT}="DEFAULT"; exec ...'`.)
- QEMU missing (folder `firmware/.embuild/espressif/tools/qemu-xtensa` renamed): installed again
  by `idf_tools.py` (archive already in `dist/`, extracted), then the run passed, exit 0.
- After every run: no `qemu-system-xtensa` process left.
- `sh scripts/validate.sh` (full): green, including "Firmware: boots in QEMU".

## Validation

- [x] Validate command green
- [x] Agent review done, blocking findings fixed
- [ ] Demo seen and approved by the user (to run on 2026-11-21)
