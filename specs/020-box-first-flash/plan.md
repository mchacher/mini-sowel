# Plan: Spec 020

## Steps

- [x] 1. Living spec updated (delta of the spec)
- [x] 2. `scripts/lib/firmware.sh` moved out of `scripts/qemu.sh`; `sh scripts/qemu.sh` and
      `--box` green, same lines
- [x] 3. `scripts/box.sh` (R1 to R5, R7) and `crates/host/tests/box_script.rs` with fake tools;
      the `espflash` runner of `firmware/.cargo/config.toml` removed, `sdkconfig.defaults:27` reworded
- [x] 4. Firmware feature `flash-saves` and its `saves:` line; `scripts/validate.sh` lints it
- [x] 5. D31, architecture, README, CLAUDE.md, what this makes false; `docs/roadmap.md`: E3 split
      into E3a to E3d, inbox line of the `devices.json` stall removed (consumed here), the line of the
      33 requests sent to E3c; `sh scripts/validate.sh` green
- [x] 6. On the box, each write with the user's yes (R8.1): identify, raw boot log, backup,
      restore and its proof, `box` image twice, `sim` image; the final restore is not done: the box
      stays on the measuring image (user's test box, E3b flashes it again)
- [x] 7. `docs/hardware.md`, `docs/memory-budget.md` filled from step 6; inbox lines (R8.4)
- [x] 8. Types mirroring Sowel, API, WebSocket, interface: not applicable

## Test plan

Fakes: `esptool.py`, `espefuse.py` and `gen_esp32part.py` written by the test into a temp folder put
first on `PATH`; each logs its arguments to a file and answers from environment variables (chip
revision, flash size, eFuses, a read that differs, a write that fails). `SOLITE_TARGET_DIR` points
to a fake build (application, bootloader), `SOLITE_HARDWARE_MD` to a sheet the test writes, `SOLITE_SERIAL_CAPTURE` to a fake capture
(a command printing a canned boot log). The
prompt is answered through a pseudo-terminal (the script reads `/dev/tty`). Every test also checks the fakes' log: no write command
without a confirmed prompt, never `burn_*`.

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| box.sh identify | Revision 3, 16 MB, eFuses clear | Five lines, exit 0, only reads sent |
| box.sh identify | No serial device, or two, no `--port` | Exit 2, the devices found listed |
| box.sh identify | No answer (not in download mode, or wrong port) | Exit 4, the manual way printed |
| box.sh identify | Secure boot on (`ABS_DONE_0` true) | Exit 3, `STOP` |
| box.sh identify | Flash encryption on (`FLASH_CRYPT_CNT` not zero) | Exit 3, `STOP` |
| box.sh identify | Revision 1, or a 4 MB flash | Exit 3, the reason named |
| box.sh tools | `esptool.py` on `PATH`; none on `PATH` but one in a fake `firmware/.embuild` | The `PATH` one used; else the `.embuild` one |
| box.sh backup | MAC tail typed (upper case, then lower case), two equal reads | Named file and `.sha256` in mode 0600, the hardware row printed, second read deleted |
| box.sh backup | Answer piped on standard input, no terminal | Exit 1, no read sent |
| box.sh backup | Wrong answer at the prompt | Exit 1, `nothing done`, no read sent |
| box.sh backup | The second read differs | Exit 1, both kept, no `.sha256` |
| box.sh backup | A short read, or a uniform one | Exit 1, not valid |
| box.sh backup | No `--out`, a folder inside the repository, or the file exists | Exit 2, no read sent |
| box.sh restore | Right MAC typed | `write_flash` with the `keep` flags at 0x0, exit 0 |
| box.sh restore | Empty or wrong answer | Exit 1, `nothing written`, no write sent |
| box.sh restore | `--dry-run` | The plan printed, no prompt, no write |
| box.sh restore | SHA-256 differs from `.sha256`, the MAC of the name is another box's, or the file is not 16 MB | Exit 1 before the prompt |
| box.sh restore | The write fails | Exit 1, the two ways out printed |
| box.sh restore | The box is unfit (secure boot) | Exit 3, no prompt |
| box.sh flash | `box` image, `--backup` whose SHA-256 is in a row with a restore date, right MAC | Erases `nvs`, `otadata`, `storage` at the table's offsets; bootloader 0x1000, table 0x8000, app at `ota_0` |
| box.sh flash | No `--backup`; its SHA-256 in no row, or in a row with no restore date; its MAC another box's | Exit 1 before the prompt |
| box.sh flash | `--dry-run` | The plan printed (regions, offsets, sizes), no prompt, no write |
| box.sh flash | The box is unfit (flash encryption) | Exit 3, no prompt, no write |
| box.sh flash | `--image sim` with the QEMU image (no `written to flash`), or `box` with a `sim` build | Exit 1, the build command named |
| box.sh flash | Application missing, or its `.bin` (`elf2image --flash_mode dio --flash_size 16MB`) larger than `ota_0` | Exit 1 |
| box.sh flash | Wrong answer at the prompt | Exit 1, nothing written |
| box.sh log | `--from-file`, box image log | Exit 0, the five lines printed |
| box.sh log | `--from-file`, measuring image log, peak 312 KB | Exit 0, `engine:` and `saves:` printed |
| box.sh log | An `rst:` line after the banner, or a panic | Exit 1, last lines |
| box.sh log | Two `rst:` lines before the banner (port opening, script's reset), then a good boot | Exit 0 |
| box.sh log | `--expect-kept` with `storage: formatted`; then without it | Exit 1, the line named; exit 0 |
| box.sh log | `--raw` (fake serial capture) | The log written to `$TMPDIR/solite-box/`, no checks, exit 0 |
| box.sh log | `--replug`, the port gone then back | Waits, then captures; gone past the time allowed: exit 4 |
| box.sh log | `psram none` | Exit 1, PSRAM named |
| box.sh log | Peak above 350 KB, internal minimum below 64 KB, a device not reported, or no `saves:` | Exit 1, the figure named |
| box.sh log | A missing line (no `network:`) | Exit 1, the line named |
| qemu.sh | Both images, on the shared library | Same lines and verdict as before (validate) |
| firmware | `flash-saves` image | Builds and passes clippy (validate) |
| firmware | A file of `firmware/src/` names GPIO12 or GPIO15 | The test fails (the radio's pins are not driven) |
| Dongle Max | Identify, then backup | eFuses clear, revision 3, 16 MB; two equal reads, SHA-256 recorded |
| Dongle Max | Restore, then `--radio tcp://<address>:6638` | The radio identified again; restore date recorded |
| Dongle Max | `box` image, booted twice | First `storage: formatted`, then `--expect-kept` green; `radio: none`; memory line recorded |
| Dongle Max | Measuring image | `engine:` within the ceilings, `saves:` recorded in the budget |
| Dongle Max | Final restore | Original firmware, the radio over TCP again |
| docs | `sh scripts/check-docs.sh` | Green: three files, index row, present tense |

## Validation

- [x] Validate command green
- [x] Agent review done, blocking findings fixed
- [ ] Demo seen and approved by the user
