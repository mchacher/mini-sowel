# Hardware sheet: the Sonoff Dongle Max

For whoever wires, flashes or restores the Dongle Max (Sonoff "Dongle-M" family, the model with
the ESP32 and the Ethernet port). Every fact says where it comes from and how far to trust it.
This sheet is written from Sonoff's pages and from the community; a run of `scripts/box.sh` on the box
settles its rows (section "Measured on the box"); the rows the radio image's run settles (the UART's
speed and TX pin, the pins driven) are measured there too (run of 2026-11-22).
The unknowns have a read-only check each (section "Risks").

Trust levels:

- **confirmed**: said by Sonoff's own pages.
- **reported**: said by users (forum, pull requests), not by Sonoff.
- **unknown**: not found, or contradictory; no firmware code uses it before it is settled, except the
  pins of the radio image (below), which the run of that image settles.

Every write to the box (flash, eFuse, partition) needs the user's explicit yes (decision D6).

## The chips

| Part | What | Trust | Source |
| ---- | ---- | ----- | ------ |
| ESP32-D0WDR2-V3 | Dual-core Xtensa, chip revision 3, 2 MB PSRAM in the package, runs the Wi-Fi, the Ethernet, the radio bridge | confirmed | [Hardware specification](https://dongle.sonoff.tech/guide/dongle-m/hardware-specification-dongle-m/) |
| EFR32MG24 | Silicon Labs Zigbee/Thread radio, the chip Solite speaks EZSP to | confirmed | same page |
| 16 MB flash | The ESP32's program memory | confirmed | same page |
| IP101 | Ethernet PHY (RMII), address 1 | confirmed | [ESPHome guide](https://dongle.sonoff.tech/guide/dongle-m/running-esphome-firmware-on-dongle-m/) |
| CP2102(N) | USB-to-serial bridge | confirmed | hardware specification page |

## Pin map

GPIO numbers of the ESP32. "Level" is the active level when it is known.

| Signal | GPIO | Level | Trust | Source |
| ------ | ---- | ----- | ----- | ------ |
| EFR32 UART, ESP32 receives (RX) | 13 | | confirmed | [ESPHome guide](https://dongle.sonoff.tech/guide/dongle-m/running-esphome-firmware-on-dongle-m/) |
| EFR32 UART, ESP32 transmits (TX) | 17 | | confirmed on the box (2026-11-22): the radio image drives it as TX with the PSRAM present (2 MB, 2046 KB free), no conflict | same guide; run of the radio image |
| EFR32 UART flow control (RTS, CTS) | none listed | | unknown | same guide lists none |
| EFR32 UART baud rate | 115200, 460800 or 921600 selectable; the bootloader speaks 115200; the radio answered at 115200 baud, the first speed tried (2026-11-22); the original firmware's own value stays unread | | measured on the box for the radio (115200) | same guide; run of the radio image |
| EFR32 reset | 15 | low | confirmed; GPIO15 is also a strapping pin of the ESP32 (MTDO: held low at reset it silences the ROM's boot messages) | same guide |
| EFR32 bootloader entry | 12 | high (active high) | confirmed (same source): Sonoff's own ESPHome YAML in the guide sets `inverted: false` on GPIO12; GPIO12 is also a strapping pin | same guide |
| Ethernet MDC | 23 | | confirmed | same guide |
| Ethernet MDIO | 18 | | confirmed | same guide |
| Ethernet RMII clock, input (external 50 MHz) | 0 | | confirmed | same guide |
| Ethernet PHY power | 5 | not published | confirmed (pin), unknown (level) | same guide |
| Ethernet PHY address | 1 | | confirmed | same guide |
| RGB LED, red | 4 | not published | confirmed (pin), unknown (level); LEDC PWM in Sonoff's example | same guide |
| RGB LED, green | 14 | not published | same | same guide |
| RGB LED, blue | 2 | not published | same | same guide |
| Button | 34 | low | confirmed; GPIO34 is input-only with no internal pull-up | same guide |

The same pin table appears in the community thread
[ESPHome on Sonoff Dongle Max](https://forum.ewelink.cc/t/esphome-on-sonoff-dongle-max/209243)
(reported, consistent with Sonoff's guide). The Tasmota draft
[pull request 25301](https://github.com/arendst/Tasmota/pull/25301) adds the IP101 but no GPIO
(reported). The ESPHome device page
[pull request 1926](https://github.com/esphome/devices.esphome.io/pull/1926) discusses a race
between the "bootloader mode" and "reset" scripts of the radio (reported): the reset and the
bootloader pins must be driven in a fixed order, which the firmware's radio adapter must respect.

## The USB bridge

- The CP2102(N) reaches the ESP32: ESPHome is flashed over USB (confirmed, ESPHome guide).
- Whether it also reaches the EFR32's UART is **unknown**.
- Whether DTR and RTS reset the ESP32 and enter its download mode automatically is **unknown**.
- A user reports holding the button while plugging the USB cable to enter download mode
  ([forum thread](https://forum.ewelink.cc/t/is-the-dongle-m-cursed/207730), reported, not
  verified). Until the box is tried, the procedure below assumes this manual entry.

## The original firmware

- The radio is relayed over TCP, port 6638, one client at a time; a web console runs at
  `http://Dongle-M.local` (confirmed:
  [FAQ](https://dongle.sonoff.tech/guide/dongle-m/donglem_faq/),
  [web console](https://dongle.sonoff.tech/guide/dongle-m/web_console/)). The console also has
  "Upload & Flash Firmware".
- Release notes:
  [ESP32 firmware release notes](https://dongle.sonoff.tech/guide/dongle-m/donglem_esp32_firmware_release_notes/).
- Sonoff documents two ways back to its firmware: the web tool
  [SONOFF Dongle Flasher for Web](https://dongle.sonoff.tech/sonoff-dongle-flasher) and the Home
  Assistant add-on
  [hassio-ihost-addon](https://github.com/iHost-Open-Source-Project/hassio-ihost-addon).
- Sonoff publishes OTA application images of the ESP32 firmware (`donglem_esp32_*_ota.bin`) and
  the radio's firmware (`zigbee_ncp_uart_hw_donglem_*.gbl`) in the release notes of its eWeLink
  forum ("Dongle-M firmware Release Notes"). An OTA image is the application only: no bootloader,
  no partition table, no NVS (the box's own settings, Wi-Fi included). The only complete copy is
  the one read from the user's own box (procedure below).
- Secure boot and flash encryption are not documented: the eFuses of the box say (check below).

## Risks, and the read-only check of each

| Risk | Why | Check (read-only) |
| ---- | --- | ----------------- |
| GPIO17 and the PSRAM | The EFR32 UART's TX is on GPIO17, while the in-package PSRAM of the ESP32-D0WDR2-V3 may use GPIO16 and GPIO17. If it does, the UART TX pin of the table is wrong, or the PSRAM and the radio cannot both work. | `esptool.py chip_id` and the original firmware's boot log (serial): they say whether the PSRAM is found, and on which pins. Evidence against the conflict: Sonoff's ESPHome YAML in the guide runs `psram: mode: quad` together with `tx_pin: GPIO17`; the row stays unknown until the check. |
| GPIO12 at boot | GPIO12 is the flash-voltage strapping pin of the ESP32: driving it at the wrong level during boot can stop the box from starting, and burning the wrong eFuse can brick it. The EFR32 bootloader line is on it. | Read the strapping state in the boot log; the radio image drives GPIO12 only from its `engine` thread, after the boot is over, the level (low) written before the pin becomes an output. |
| Secure boot, flash encryption | If enabled, a flashed image must be signed or encrypted, and a backup read from flash is encrypted. | `espefuse.py summary` (reads eFuses only). |
| Radio baud rate | The original firmware's UART speed is not published; a wrong speed gives no answer from the radio. | The original firmware's boot log, or its web console; the host program already speaks to the radio through it. |
| LED and PHY power levels | Active levels are not published. | Read from the original firmware's behaviour, or try with the LED only (an LED cannot be damaged by the wrong level). |
| The Wi-Fi antenna | Not described by Sonoff's pages found. | Not needed before Wi-Fi is built. |

Only the radio image (feature `radio`, spec 021) drives pins: GPIO12 (the bootloader line, never set
active), GPIO15 (reset) and the UART's GPIO13 and GPIO17, all named in `firmware/src/radio_uart.rs`
and nowhere else (a test checks it). The box and measuring images drive none. The run of the radio
image settles those rows (section "Measured on the box").

## Procedures

All of them run through `sh scripts/box.sh` (spec 020). Every step that writes needs the user's yes:
the script asks, on the terminal, for the last 6 hex digits of the box's MAC address, and no option
skips the question. Entering the download mode **reboots the ESP32 and stops the original firmware's
radio bridge on TCP 6638** for as long as it lasts. No command writes an eFuse.

1. **Identify** (read-only): `sh scripts/box.sh identify`. It prints the chip and its revision, the
   MAC address, the flash size, secure boot and flash encryption, and stops (exit 3) on a box it cannot
   flash: secure boot on, flash encryption on, a chip revision below 3 or a flash that is not 16 MB.
   `esptool.py` resets the box through the USB bridge; when it gets no answer (exit 4), enter the
   download mode by hand: hold the button while plugging the USB cable (reported, see "The USB bridge").
2. **Capture the original firmware's boot log** (read-only): `sh scripts/box.sh log --raw` (use
   `--replug` when the bridge does not reset the chip). It settles the PSRAM, the strapping state and the
   original firmware's version.
3. **Back up the whole flash first, before any other write** (the user runs it, in their terminal):
   `sh scripts/box.sh backup --out <folder outside the repository>`. It reads the 16 MB twice, and keeps
   `dongle-max-<mac>-<date>.bin` (mode 0600, with its `.sha256`) when the two reads are equal and
   not one repeated byte; it prints the row to paste below. When a read is invalid, both files are
   kept (`.bin` and `.bin.read2`) for inspection and no row is printed. A read takes about 3
   minutes at 921600 baud (about 6 minutes for the two) and about 24 minutes at 115200
   (`--baud 115200`; about 50 minutes for the two). The file may hold the box's Wi-Fi credentials: it stays outside
   the repository, and no MAC address, serial number or captured log goes into it.

   | Date | Chip revision | Firmware version | SHA-256 of the backup | Restore proven |
   | ---- | ------------- | ---------------- | --------------------- | -------------- |
   | 2026-11-22 | v3.1 | not read (the web console was not reachable) | 35aea91858ab0c4eb66ea776d7d1887c8113990b48828ef27684be190cee5e87 | 2026-11-22 |

   The date is the backup's (YYYY-MM-DD), the revision the one `esptool.py` reads (`v3.x`), the version
   the one the web console shows, the SHA-256 the 64 hex digits in lower case; the last column holds
   the date of the proof of step 4, empty until then. `flash` reads the fourth and fifth columns.
4. **Restore, once, and prove it** (needs the user's yes): `sh scripts/box.sh restore <backup file>`
   writes the backup back; the box runs its original firmware again. The way back is proven when the host
   identifies the radio over TCP again: `cargo run -p solite-host -- --radio tcp://<address>:6638`. Write
   the date in the backup's row. The second way back is Sonoff's own web flasher, or its Home Assistant
   add-on (see "The original firmware"); a failed restore is run again at `--baud 115200`.
5. **Flash Solite** (needs the user's yes, once the backup's row has its restore date): build the
   image (`cd firmware && cargo build --release`, or `--features flash-saves` for the measuring image,
   or the radio image with the command in the README), then
   `sh scripts/box.sh flash --image box|sim|radio --backup <backup file>`. It refuses without the backup,
   when the build is not the image asked for or does not fit the `ota_0` slot, erases `nvs`, `otadata`
   and `storage` (unless `--keep-storage`, radio image only; the original firmware's settings, which may hold Wi-Fi credentials, are in the backup),
   writes the bootloader, the table of `firmware/partitions.csv` (decision D25) and the application, then
   resets the box; the radio image is read from `firmware/target/radio` and leaves the box unreset
   (`--after no_reset`), so that its first boot, and its pairing window, are the ones of `log`.
6. **Check the boot**: `sh scripts/box.sh log --image box|sim|radio` checks the serial log as
   `scripts/qemu.sh` checks QEMU's (the radio image's 300 seconds are shown as they come); `--expect-kept` on the second boot of the box's image checks that the partition was
   kept. A boot loop or a panic fails with the last lines: restore the backup, or flash the other image.
7. **Restore the original firmware** (optional, on the user's request), so the Mac's TCP development
   path works; the box can end on the radio image.

## Measured on the box

A box ran `scripts/box.sh` on 2026-11-22 (identify, backup read twice, restore and its proof, the box's
image and the measuring image booted). The memory and save-stall figures are in
[memory-budget.md](memory-budget.md). The original firmware's version stays unread: the box was not on
the network, so its web console was not reachable.

| Fact | Value | Date |
| ---- | ----- | ---- |
| Chip revision, PSRAM, flash size, eFuses | ESP32-D0WDR2-V3 revision v3.1, 2 MB PSRAM (2045 KB free at boot), 16 MB flash, secure boot off, flash encryption off | 2026-11-22 |
| Does the USB bridge reset the chip (DTR and RTS)? | Yes: `esptool` enters download mode and resets the box by itself, at 115200 baud (921600 and 460800 fail: invalid packet head) | 2026-11-22 |
| Original firmware version | not read (the box was not on the network) | 2026-11-22 |
| PSRAM and GPIO16/17 (the risk above) | PSRAM found and usable with the firmware as built: no conflict seen | 2026-11-22 |
| PSRAM with the UART's TX on GPIO17 (the radio image) | PSRAM found (2 MB, 2046 KB free at boot) with GPIO17 driven as TX: no conflict | 2026-11-22 |
| The EFR32 UART's speed (115200, 460800 or 921600) | 115200 baud, answered at the first speed tried (the probe needed no fallback) | 2026-11-22 |
| The radio's firmware | EZSP protocol 13, stack type 2, EmberZNet 7.4.5.0 | 2026-11-22 |
| The pins the radio image drives (GPIO12 low, GPIO15 reset, GPIO13 and GPIO17 UART) | driven in the fixed order (GPIO12 low before GPIO15); the radio answered, not wedged | 2026-11-22 |
