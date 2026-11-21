# Hardware sheet: the Sonoff Dongle Max

For whoever wires, flashes or restores the Dongle Max (Sonoff "Dongle-M" family, the model with
the ESP32 and the Ethernet port). Every fact says where it comes from and how far to trust it.
Nothing was measured on a box for this sheet: it is written from Sonoff's pages and from the
community, and the unknowns have a read-only check each (section "Risks").

Trust levels:

- **confirmed**: said by Sonoff's own pages.
- **reported**: said by users (forum, pull requests), not by Sonoff.
- **unknown**: not found, or contradictory; no firmware code uses it before it is settled.

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
| EFR32 UART, ESP32 transmits (TX) | 17 | | unknown until `chip_id` and the boot log settle the risk "GPIO17 and the PSRAM"; evidence against the conflict: the same YAML runs `psram: mode: quad` together with `tx_pin: GPIO17` | same guide; community thread below |
| EFR32 UART flow control (RTS, CTS) | none listed | | unknown | same guide lists none |
| EFR32 UART baud rate | 115200, 460800 or 921600 selectable; the bootloader speaks 115200; the original firmware's value is not published | | unknown for the original firmware | same guide |
| EFR32 reset | 15 | low | confirmed | same guide |
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
- No partition table and no stand-alone image of the original firmware are published. The only
  complete copy is the one read from the user's own box (procedure below).
- Secure boot and flash encryption are not documented: the eFuses of the box say (check below).

## Risks, and the read-only check of each

| Risk | Why | Check (read-only) |
| ---- | --- | ----------------- |
| GPIO17 and the PSRAM | The EFR32 UART's TX is on GPIO17, while the in-package PSRAM of the ESP32-D0WDR2-V3 may use GPIO16 and GPIO17. If it does, the UART TX pin of the table is wrong, or the PSRAM and the radio cannot both work. | `esptool.py chip_id` and the original firmware's boot log (serial): they say whether the PSRAM is found, and on which pins. Evidence against the conflict: Sonoff's ESPHome YAML in the guide runs `psram: mode: quad` together with `tx_pin: GPIO17`; the row stays unknown until the check. |
| GPIO12 at boot | GPIO12 is the flash-voltage strapping pin of the ESP32: driving it at the wrong level during boot can stop the box from starting, and burning the wrong eFuse can brick it. The EFR32 bootloader line is on it. | Read the strapping state in the boot log; the firmware does not drive GPIO12 before the boot is over. |
| Secure boot, flash encryption | If enabled, a flashed image must be signed or encrypted, and a backup read from flash is encrypted. | `espefuse.py summary` (reads eFuses only). |
| Radio baud rate | The original firmware's UART speed is not published; a wrong speed gives no answer from the radio. | The original firmware's boot log, or its web console; the host program already speaks to the radio through it. |
| LED and PHY power levels | Active levels are not published. | Read from the original firmware's behaviour, or try with the LED only (an LED cannot be damaged by the wrong level). |
| The Wi-Fi antenna | Not described by Sonoff's pages found. | Not needed before Wi-Fi is built. |

The firmware codes **no pin** until the line it concerns is settled in this table.

## Procedures

Every step that writes says "needs the user's yes". Entering the download mode **reboots the
ESP32 and stops the original firmware's radio bridge on TCP 6638** for as long as it lasts.

1. **Enter the download mode**: with the pin table not yet proven, hold the button while
   plugging the USB cable (reported, see "The USB bridge"); `esptool.py` then reaches the ROM
   bootloader.
2. **Back up the whole flash first, before any other step** (needs the user's yes, because it
   reboots the box into the bootloader):
   `esptool.py -b 921600 read_flash 0x0 0x1000000 <file>`: a few minutes at 921600 baud, about 20
   minutes at 115200.
3. **Keep the file outside the repository**: it may hold the box's Wi-Fi credentials and its
   identifiers. Write its SHA-256 here, with the date, the chip's MAC address and the original
   firmware version shown by the web console:

   | Date | Box | Firmware version | SHA-256 of the backup |
   | ---- | --- | ---------------- | --------------------- |
   | none taken | | | |

4. **Read the unknowns** (read-only): `esptool.py chip_id`, `espefuse.py summary`, and the
   original firmware's boot log on the serial port. Write the results in this sheet.
5. **Restore** (needs the user's yes): `esptool.py write_flash 0x0 <file>` writes the backup
   back, the box then runs its original firmware again. The second way back is Sonoff's own web
   flasher, or its Home Assistant add-on (see "The original firmware").
6. **Flash Solite** (needs the user's yes, once the backup's SHA-256 is recorded in step 3 and the
   restore is proven): from the repository root, after `cd firmware && cargo build --release`,
   `espflash flash --partition-table firmware/partitions.csv --bootloader <bootloader.bin>
   --erase-parts otadata,nvs firmware/target/xtensa-esp32-espidf/release/solite-firmware`, where
   `<bootloader.bin>` is the newest
   `firmware/target/xtensa-esp32-espidf/release/build/esp-idf-sys-*/out/build/bootloader/bootloader.bin`
   (the one `scripts/qemu.sh` uses). The table is the one of decision D25. `--erase-parts
   otadata,nvs` erases the old firmware's boot selection and its NVS, which may hold the box's
   Wi-Fi credentials; it is done only after the backup is recorded.

## Open checks

The checks of steps 1 to 4 need the user's yes and a box on USB. Until they are done, the
"unknown" rows above stay unknown.
