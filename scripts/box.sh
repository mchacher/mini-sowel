#!/bin/sh
# The Dongle Max's ESP32 over USB: identify it, save its whole flash, restore it, flash Solite's
# images, check a boot log (spec 020).
#
#   sh scripts/box.sh identify [--port <device>] [--baud <rate>]
#   sh scripts/box.sh backup --out <folder> [--dry-run]
#   sh scripts/box.sh restore <backup file> [--dry-run]
#   sh scripts/box.sh flash --image box|sim|radio [--backup <backup file>] [--keep-storage] [--dry-run]
#   sh scripts/box.sh wifi [--forget] [--port <device>]
#   sh scripts/box.sh log --image box|sim|radio [--seconds <n>] [--replug] [--expect-kept]
#   sh scripts/box.sh log --from-file <log> --image box|sim|radio [--expect-kept]   (no port)
#   sh scripts/box.sh log --raw [--seconds <n>]                              (no checks)
#
# The port is --port, or the only USB bridge found (/dev/cu.usbserial*, /dev/cu.SLAB_USBtoUART*,
# /dev/ttyUSB*); the baud rate is 921600. identify, backup, restore and flash need the box in
# download mode (esptool resets it through the USB bridge; else hold the button while plugging the
# cable); log needs a normal boot. The radio image (spec 021) is read from firmware/target/radio
# (built with the command `flash` prints when it is missing); `flash --image radio` does not reset
# the box (`--after no_reset`): the first boot, and its pairing window, is the one `log --image
# radio` captures, 300 s by default, shown live.
#
# backup, restore and flash print what they will do; backup and restore then ask for the last 6 hex
# digits of the box's MAC address on the terminal (/dev/tty, never standard input): with no terminal,
# or any other answer, nothing is done. flash asks nothing more than the command itself, and its
# --backup is optional (decision D34). --dry-run prints the plan only. No option skips the question
# of backup and restore. No command writes an eFuse. `backup`, `restore` and `flash` are run by the user, in their terminal.
#
# `flash --image radio --keep-storage` erases nvs and otadata only: the home, the network's backup
# and the Wi-Fi settings (the files of `storage`) stay. `wifi` asks on the terminal for the Wi-Fi
# network's name and password (echo off, never shown, logged or kept) and sends them to the radio
# image's serial console (spec 022 R9); `wifi --forget` removes them.
#
# Exit codes: 0 done, 1 failed or refused at the prompt, 2 wrong usage or port, 3 the box is
# unfit (secure boot, flash encryption, chip revision below 3, flash not 16 MB), 4 no answer from
# the box.
#
# Environment (the tests use them; they point at fakes): SOLITE_TARGET_DIR (the firmware's build
# folder), SOLITE_RADIO_TARGET_DIR (the radio image's build folder), SOLITE_HARDWARE_MD (the sheet whose backup table `flash` reads), SOLITE_SERIAL_CAPTURE
# (a command `<command> <port> <seconds>` printing the serial output, in place of pyserial),
# SOLITE_ESPRESSIF (ESP-IDF's tools folder), SOLITE_PORT_PATTERNS (the glob patterns of the USB
# bridges), SOLITE_REPLUG_SECONDS (the time allowed to plug the cable again, 60),
# SOLITE_SERIAL_CONSOLE (a program `<program> <port>` that reads the console line on its standard
# input and prints the answers, in place of the serial port), SOLITE_WIFI_ANSWER_SECONDS (the time
# `wifi` waits for the console's answer, 15: the console's write may wait behind an engine save),
# SOLITE_WIFI_WAIT_SECONDS (the time `wifi` waits for the connection, 60).
#
# Output stays short: a build or a long tool output goes to $TMPDIR/solite-box/, and its path is
# printed on a failure. Backups and serial logs never go into the repository.
CALLER_DIR=$(pwd -P)
cd "$(dirname "$0")/.." || exit 2
ROOT=$(pwd -P)
umask 077

box_fail() { echo "box: $1" >&2; exit 1; }
fw_fail() { box_fail "$1"; }
. "$ROOT/scripts/lib/firmware.sh"

usage() {
  echo "usage: sh scripts/box.sh identify|backup|restore|flash|log|wifi [--port <device>] [--baud <rate>] [options]" >&2
  [ -n "$1" ] && echo "box: $1" >&2
  exit 2
}

WORK=${TMPDIR:-/tmp}/solite-box
mkdir -p "$WORK" || exit 2

COMMAND=$1
[ -n "$COMMAND" ] || usage
shift
PORT=
BAUD=921600
OUT=
IMAGE=
BACKUP=
DRY=false
SECONDS_=
REPLUG=false
KEPT=false
KEEP_STORAGE=false
FORGET=false
FROM_FILE=
RAW=false
FILE=
while [ $# -gt 0 ]; do
  case "$1" in
    --port) [ $# -ge 2 ] || usage "--port needs a value"; PORT=$2; shift 2 ;;
    --baud) [ $# -ge 2 ] || usage "--baud needs a value"; BAUD=$2; shift 2 ;;
    --out) [ $# -ge 2 ] || usage "--out needs a value"; OUT=$2; shift 2 ;;
    --image) [ $# -ge 2 ] || usage "--image needs a value"; IMAGE=$2; shift 2 ;;
    --backup) [ $# -ge 2 ] || usage "--backup needs a value"; BACKUP=$2; shift 2 ;;
    --seconds) [ $# -ge 2 ] || usage "--seconds needs a value"; SECONDS_=$2; shift 2 ;;
    --from-file) [ $# -ge 2 ] || usage "--from-file needs a value"; FROM_FILE=$2; shift 2 ;;
    --dry-run) DRY=true; shift ;;
    --replug) REPLUG=true; shift ;;
    --expect-kept) KEPT=true; shift ;;
    --raw) RAW=true; shift ;;
    --keep-storage) KEEP_STORAGE=true; shift ;;
    --forget) FORGET=true; shift ;;
    -*) usage "unknown option $1" ;;
    *) [ -z "$FILE" ] || usage "one file only"; FILE=$1; shift ;;
  esac
done
# A relative path is the caller's, not the repository's: made absolute before anything else.
abs_path() { case "$1" in "") ;; /*) echo "$1" ;; *) echo "$CALLER_DIR/$1" ;; esac; }
OUT=$(abs_path "$OUT")
BACKUP=$(abs_path "$BACKUP")
FROM_FILE=$(abs_path "$FROM_FILE")
FILE=$(abs_path "$FILE")
case "$BAUD" in '' | *[!0-9]*) usage "--baud takes a number" ;; esac
case "$IMAGE" in '' | box | sim | radio) ;; *) usage "--image is box, sim or radio" ;; esac

TARGET_DIR=${SOLITE_TARGET_DIR:-$ROOT/firmware/target/xtensa-esp32-espidf/release}
# The radio image is built into its own target folder, with its own ESP-IDF settings (spec 021).
RADIO_TARGET_DIR=${SOLITE_RADIO_TARGET_DIR:-$ROOT/firmware/target/radio/xtensa-esp32-espidf/release}
RADIO_BUILD='cd firmware && CARGO_TARGET_DIR=target/radio ESP_IDF_SDKCONFIG_DEFAULTS="sdkconfig.defaults;sdkconfig.radio.defaults" cargo build --release --features radio'
HARDWARE_MD=${SOLITE_HARDWARE_MD:-$ROOT/docs/hardware.md}
FLASH_BYTES=16777216

# The USB bridge: --port, or the only one found.
resolve_port() {
  [ -z "$PORT" ] || return 0
  found=
  count=0
  for p in ${SOLITE_PORT_PATTERNS:-/dev/cu.usbserial* /dev/cu.SLAB_USBtoUART* /dev/ttyUSB*}; do
    [ -e "$p" ] || continue
    found="$found $p"
    count=$((count + 1))
  done
  if [ "$count" -eq 1 ]; then
    PORT=${found# }
    return 0
  fi
  if [ "$count" -eq 0 ]; then
    echo "box: no USB serial device found; plug the box or give --port <device>" >&2
  else
    echo "box: several USB serial devices found, give --port <device>:" >&2
    for p in $found; do echo "  $p" >&2; done
  fi
  exit 2
}

# esptool on the port, at the baud rate.
esp() { $ESPTOOL --chip esp32 --port "$PORT" --baud "$BAUD" "$@"; }

sha256_of() {
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    sha256sum "$1" | awk '{print $1}'
  fi
}

no_answer() {
  echo "box: no answer from the box on $PORT." >&2
  echo "box: put it in download mode by hand (hold the button while plugging the USB cable), then run again." >&2
  echo "box: the original firmware's TCP bridge (port 6638) stops while the box is in download mode." >&2
  exit 4
}

# Reads the box (esptool flash_id, espefuse summary), prints one line each, and stops when the box
# is unfit. Sets CHIP, REVISION, MAC, MAC_NAME (lower case, no colons) and MAC_TAIL (its last 6
# digits).
identify_box() {
  resolve_port
  fw_find_tools
  esp flash_id >"$WORK/flash_id.log" 2>&1 || no_answer
  $ESPEFUSE --chip esp32 --port "$PORT" summary >"$WORK/efuse.log" 2>&1 || no_answer

  CHIP=$(sed -n 's/^Chip is \(.*\) (revision.*/\1/p' "$WORK/flash_id.log" | head -n 1)
  [ -n "$CHIP" ] || CHIP=$(sed -n 's/^Chip type: *\(.*\) (revision.*/\1/p' "$WORK/flash_id.log" | head -n 1)
  REVISION=$(grep -ao 'revision v[0-9]*\.[0-9]*' "$WORK/flash_id.log" | head -n 1 | sed 's/revision //')
  MAC=$(sed -n 's/^MAC: *\([0-9A-Fa-f:]*\).*/\1/p' "$WORK/flash_id.log" | head -n 1)
  FLASH_MB=$(sed -n 's/^Detected flash size: *\([0-9]*\)MB.*/\1/p' "$WORK/flash_id.log" | head -n 1)
  [ -n "$FLASH_MB" ] || FLASH_MB=$(sed -n 's/^Flash size: *\([0-9]*\)MB.*/\1/p' "$WORK/flash_id.log" | head -n 1)
  [ -n "$CHIP" ] && [ -n "$REVISION" ] && [ -n "$MAC" ] && [ -n "$FLASH_MB" ] \
    || box_fail "cannot read the chip, its revision, its MAC address and its flash size from esptool's answer ($WORK/flash_id.log)."
  ABS0=$(sed -n 's/^ABS_DONE_0 .*= \([A-Za-z]*\) .*/\1/p' "$WORK/efuse.log" | head -n 1)
  ABS1=$(sed -n 's/^ABS_DONE_1 .*= \([A-Za-z]*\) .*/\1/p' "$WORK/efuse.log" | head -n 1)
  CRYPT=$(sed -n 's/^FLASH_CRYPT_CNT .*= \([0-9][0-9]*\) .*/\1/p' "$WORK/efuse.log" | head -n 1)
  [ -n "$ABS0" ] && [ -n "$ABS1" ] && [ -n "$CRYPT" ] \
    || box_fail "cannot read secure boot and flash encryption from espefuse's answer ($WORK/efuse.log)."

  MAC_NAME=$(echo "$MAC" | tr -d ':' | tr 'A-F' 'a-f')
  MAC_TAIL=$(echo "$MAC_NAME" | cut -c7-12)
  SECURE=off
  [ "$ABS0" = True ] || [ "$ABS1" = True ] && SECURE=ON
  ENCRYPTION=off
  [ "$CRYPT" -ne 0 ] && ENCRYPTION=ON
  echo "chip: $CHIP, revision $REVISION"
  echo "mac: $MAC"
  echo "flash: $FLASH_MB MB"
  echo "secure boot: $SECURE"
  echo "flash encryption: $ENCRYPTION"

  unfit=
  [ "$SECURE" = ON ] && unfit="secure boot is on"
  [ -z "$unfit" ] && [ "$ENCRYPTION" = ON ] && unfit="flash encryption is on"
  major=${REVISION#v}
  major=${major%%.*}
  [ -z "$unfit" ] && [ "$major" -lt 3 ] && unfit="the chip revision is $REVISION, the image needs revision 3 or later"
  [ -z "$unfit" ] && [ "$FLASH_MB" -ne 16 ] && unfit="the flash is $FLASH_MB MB, not 16 MB"
  if [ -n "$unfit" ]; then
    echo "box: STOP, $unfit; nothing is written to this box" >&2
    exit 3
  fi
}

# Asks for the last 6 hex digits of the MAC address, on the terminal only. Nothing is done on
# anything else.
confirm() {
  if ! (exec </dev/tty) 2>/dev/null; then
    echo "box: no terminal to confirm, nothing done" >&2
    exit 1
  fi
  printf "box: type the last 6 hex digits of the box's MAC address to go on: " >/dev/tty
  answer=
  IFS= read -r answer </dev/tty
  answer=$(printf '%s' "$answer" | tr -d '\r \t' | tr 'A-F' 'a-f')
  if [ -z "$answer" ] || [ "$answer" != "$MAC_TAIL" ]; then
    echo "box: not confirmed, nothing done" >&2
    exit 1
  fi
}

# After the plan is printed: stops on --dry-run, else asks.
go_on() {
  if $DRY; then
    echo "box: --dry-run, nothing done"
    exit 0
  fi
  # `flash` asks nothing more than the command itself (decision of the user, 2026-11-22);
  # `backup` and `restore` keep the MAC question.
  [ "$COMMAND" = flash ] || confirm
}

# The backup's checks, for restore and flash: 16 MB, its MAC (in its name) is the box's.
check_backup_file() {
  [ -f "$1" ] || box_fail "$1 is not a file."
  [ "$(wc -c < "$1" | tr -d ' ')" -eq "$FLASH_BYTES" ] || box_fail "$1 is not $FLASH_BYTES bytes (16 MB): not a backup of this box."
  name=$(basename "$1")
  case "$name" in
    "dongle-max-$MAC_NAME-"*.bin) ;;
    *) box_fail "the name of $name does not hold this box's MAC address: it is another box's backup, or renamed." ;;
  esac
}

# Not one repeated byte (an erased or an empty read): $1 the file.
uniform() {
  first=$(od -An -tu1 -N1 "$1" | tr -d ' ')
  [ -n "$first" ] || return 0
  head -c "$FLASH_BYTES" /dev/zero | tr '\000' "$(printf '\\%03o' "$first")" | cmp -s - "$1"
}

cmd_identify() {
  identify_box
}

cmd_backup() {
  [ -n "$OUT" ] || usage "backup needs --out <folder>"
  [ -d "$OUT" ] || usage "$OUT is not a folder"
  folder=$(cd "$OUT" && pwd -P)
  case "$folder" in
    "$ROOT" | "$ROOT"/*) usage "$OUT is inside the repository: a backup may hold credentials" ;;
  esac
  identify_box
  FILE_OUT=$folder/dongle-max-$MAC_NAME-$(date +%F).bin
  [ ! -e "$FILE_OUT" ] && [ ! -e "$FILE_OUT.read2" ] || usage "$FILE_OUT already exists"

  echo "box: backup, in download mode (the original firmware's TCP bridge stops while it lasts):"
  echo "  read the whole flash, 0x0 to 0x1000000 (16 MB), twice"
  echo "  to $FILE_OUT"
  echo "  and $FILE_OUT.read2, kept only when invalid; nothing is written to the box"
  go_on

  echo "box: first read..."
  esp read_flash 0x0 0x1000000 "$FILE_OUT" >"$WORK/read-1.log" 2>&1 || box_fail "the first read failed (log: $WORK/read-1.log); run again, at --baud 115200."
  echo "box: second read..."
  esp read_flash 0x0 0x1000000 "$FILE_OUT.read2" >"$WORK/read-2.log" 2>&1 || box_fail "the second read failed (log: $WORK/read-2.log); run again, at --baud 115200."
  chmod 600 "$FILE_OUT" "$FILE_OUT.read2"

  reason=
  [ "$(wc -c < "$FILE_OUT" | tr -d ' ')" -eq "$FLASH_BYTES" ] && [ "$(wc -c < "$FILE_OUT.read2" | tr -d ' ')" -eq "$FLASH_BYTES" ] \
    || reason="a read is not 16 MB long"
  [ -n "$reason" ] || cmp -s "$FILE_OUT" "$FILE_OUT.read2" || reason="the two reads differ (a bad cable or a high baud rate: run again, at --baud 115200)"
  [ -n "$reason" ] || ! uniform "$FILE_OUT" || reason="the read is one repeated byte"
  if [ -n "$reason" ]; then
    echo "box: $reason" >&2
    echo "box: both files kept for inspection: $FILE_OUT and $FILE_OUT.read2" >&2
    echo "box: backup NOT valid, nothing to record" >&2
    exit 1
  fi

  SHA=$(sha256_of "$FILE_OUT")
  printf '%s  %s\n' "$SHA" "$(basename "$FILE_OUT")" >"$FILE_OUT.sha256"
  chmod 600 "$FILE_OUT.sha256"
  rm -f "$FILE_OUT.read2"
  echo "box: backup valid: $FILE_OUT"
  echo "box: sha256 $SHA"
  echo "box: row for docs/hardware.md (add the firmware version the web console shows):"
  echo "| $(date +%F) | $REVISION | | $SHA | |"
}

cmd_restore() {
  [ -n "$FILE" ] || usage "restore needs the backup file"
  identify_box
  check_backup_file "$FILE"
  [ -f "$FILE.sha256" ] || box_fail "$FILE.sha256 is missing."
  expected=$(awk '{print $1}' "$FILE.sha256" | head -n 1)
  [ "$(sha256_of "$FILE")" = "$expected" ] || box_fail "the SHA-256 of $FILE is not the one in $FILE.sha256: the backup was altered."

  echo "box: restore, in download mode (the original firmware's TCP bridge stops while it lasts):"
  echo "  write $FILE (16 MB) at 0x0, over the whole flash"
  echo "  the box then reboots on its original firmware"
  go_on

  if ! esp write_flash --flash_mode keep --flash_freq keep --flash_size keep 0x0 "$FILE" >"$WORK/restore.log" 2>&1; then
    echo "box: the restore failed (log: $WORK/restore.log)." >&2
    echo "box: run restore again at --baud 115200, or use Sonoff's web flasher (docs/hardware.md)." >&2
    echo "box: the ROM bootloader cannot be overwritten by a flash write: download mode stays reachable." >&2
    exit 1
  fi
  echo "box: restored; the box reboots on its original firmware"
  echo "box: prove it: cargo run -p solite-host -- --radio tcp://<address>:6638, then write the date in docs/hardware.md"
}

# The backup of flash: its SHA-256 is in a row of the sheet's backup table that has a restore date.
check_backup_row() {
  [ -f "$HARDWARE_MD" ] || box_fail "$HARDWARE_MD not found."
  hash=$(sha256_of "$BACKUP")
  row=$(awk -F'|' -v h="$hash" '
    { for (i = 1; i <= NF; i++) gsub(/^ +| +$/, "", $i) }
    NF >= 6 && tolower($5) == h { print $6; found = 1 }
    END { if (!found) print "NOROW" }' "$HARDWARE_MD" | head -n 1)
  [ "$row" != NOROW ] || box_fail "the SHA-256 of $BACKUP is in no row of the backup table of docs/hardware.md."
  printf '%s' "$row" | grep -Eq '^[0-9]{4}-[0-9]{2}-[0-9]{2}$' || box_fail "its row in docs/hardware.md has no restore date (YYYY-MM-DD, found '$row'): restore the backup once and prove it first."
}

cmd_flash() {
  [ -n "$IMAGE" ] || usage "flash needs --image box|sim|radio"
  if $KEEP_STORAGE && [ "$IMAGE" != radio ]; then
    usage "--keep-storage is for the radio image: the sim image seeds /data"
  fi
  identify_box
  # The backup is optional (decision of the user, 2026-11-22): the way back to Sonoff's firmware is
  # Sonoff's own reflash. A backup that is given is still checked (16 MB, this box, proven row).
  if [ -n "$BACKUP" ]; then
    check_backup_file "$BACKUP"
    check_backup_row
  fi

  IMAGE_DIR=$TARGET_DIR
  AFTER=
  if [ "$IMAGE" = box ]; then
    BUILD="cd firmware && cargo build --release"
  elif [ "$IMAGE" = radio ]; then
    BUILD=$RADIO_BUILD
    IMAGE_DIR=$RADIO_TARGET_DIR
    # The box is not reset at the end: the first boot is `log`'s, which captures its window.
    AFTER="--after no_reset"
  else
    BUILD="cd firmware && cargo build --release --features flash-saves"
  fi
  ELF=$IMAGE_DIR/solite-firmware
  [ -f "$ELF" ] || box_fail "$ELF not found: build it with ($BUILD)."
  if [ "$IMAGE" = radio ]; then
    grep -aqF 'radio: uart ' "$ELF" || box_fail "the build is not the radio image (feature radio): build it with ($BUILD)."
    # The interface is embedded at build time from ui/dist (spec 023 R7.3).
    grep -aqF 'solite-ui ' "$ELF" || box_fail "the radio build has no embedded interface (no solite-ui marker): build it with (cd ui && npm ci && npm run build), then ($BUILD)."
    # The UART interrupt must run from internal RAM (sdkconfig.radio.defaults): a build made with
    # another folder or settings misses it, and bytes are lost while a flash write stalls the caches.
    grep -aqs '^CONFIG_UART_ISR_IN_IRAM=y' "$IMAGE_DIR"/build/esp-idf-sys-*/out/sdkconfig \
      || box_fail "the radio build was not made with sdkconfig.radio.defaults (CONFIG_UART_ISR_IN_IRAM=y is not in its sdkconfig under $IMAGE_DIR): build it with ($BUILD)."
  else
    # The radio image is never flashed as the box's or the measuring image.
    ! grep -aqF 'radio: uart ' "$ELF" || box_fail "the build is the radio image: build the $IMAGE image with ($BUILD)."
  fi
  if [ "$IMAGE" = box ]; then
    ! grep -aqF 'scenario: started' "$ELF" || box_fail "the build is a sim image, not the box's: build it with ($BUILD)."
  elif [ "$IMAGE" = sim ]; then
    grep -aqF 'written to flash' "$ELF" || box_fail "the build is not the measuring image (feature flash-saves): build it with ($BUILD)."
  fi
  fw_find_bootloader "$IMAGE_DIR"
  TABLE=$WORK/partition-table.bin
  fw_build_table "$ROOT/firmware/partitions.csv" "$TABLE"
  APP=$WORK/app.bin
  fw_elf2image "$ELF" "$APP"

  # The partitions erased: with --keep-storage (radio image), storage stays.
  ERASED="nvs otadata storage"
  $KEEP_STORAGE && ERASED="nvs otadata"
  if [ -n "$BACKUP" ]; then
    echo "box: flash the $IMAGE image (the original firmware is saved in $(basename "$BACKUP")):"
  else
    echo "box: flash the $IMAGE image (no backup given: the way back is Sonoff's own reflash):"
  fi
  for part in nvs otadata storage; do
    off=$(fw_part "$TABLE" "$part" 4)
    size=$(fw_bytes "$(fw_part "$TABLE" "$part" 5)")
    [ -n "$off" ] && [ -n "$size" ] || box_fail "no $part partition in firmware/partitions.csv."
    case " $ERASED " in *" $part "*) echo "  erase $part: $off, $size bytes" ;; esac
  done
  if $KEEP_STORAGE; then
    echo "  keep storage (the home, the network's backup and the Wi-Fi settings)"
    echo "  WARNING: keep it only over a radio image: a sim image's /data is a test home"
  fi
  echo "  write the bootloader at 0x1000 ($(wc -c < "$BOOTLOADER" | tr -d ' ') bytes)"
  echo "  write the partition table at 0x8000"
  echo "  write the application at $APP_OFFSET ($APP_SIZE bytes, slot of $SLOT_BYTES)"
  if [ "$IMAGE" = radio ]; then
    $KEEP_STORAGE || echo "  WARNING: the erase of storage destroys zigbee.json (the only copy of the network's keys once the box formed one) and the paired devices: flash the radio image once, never again over a box that holds a network (use --keep-storage to keep them)"
    echo "  the box drives this radio's network itself: never start a Mac host with this network's backup on another radio while the box runs"
  fi
  if [ "$IMAGE" = radio ]; then
    echo "  then leave the box as it is (no reset): its first boot is the one of 'log --image radio'"
  else
    echo "  then reset the box"
  fi
  go_on

  for part in $ERASED; do
    off=$(fw_part "$TABLE" "$part" 4)
    size=$(fw_bytes "$(fw_part "$TABLE" "$part" 5)")
    esp $AFTER erase_region "$off" "$(printf '0x%x' "$size")" >"$WORK/erase-$part.log" 2>&1 \
      || box_fail "the erase of $part failed (log: $WORK/erase-$part.log)."
  done
  esp $AFTER write_flash --flash_mode keep --flash_freq keep --flash_size keep \
    0x1000 "$BOOTLOADER" 0x8000 "$TABLE" "$APP_OFFSET" "$APP" >"$WORK/flash.log" 2>&1 \
    || box_fail "the flash failed (log: $WORK/flash.log); the box may not start: flash again, or restore the backup."
  if [ "$IMAGE" = radio ]; then
    echo "box: flashed; the box is not reset. Capture its first boot and its pairing window: sh scripts/box.sh log --image radio"
  else
    echo "box: flashed; the box resets. Check it: sh scripts/box.sh log --image $IMAGE"
  fi
}

# The serial output of a normal boot: $1 the port, $2 the seconds, on standard output. The box is
# reset the way esptool does it through the USB bridge (RTS pulls EN low, DTR keeps GPIO0 high).
capture() {
  if [ -n "$SOLITE_SERIAL_CAPTURE" ]; then
    $SOLITE_SERIAL_CAPTURE "$1" "$2"
    return
  fi
  fw_find_idf
  "$PY" -c '
import sys, time, serial
port, seconds = sys.argv[1], float(sys.argv[2])
s = serial.Serial()
s.port = port
s.baudrate = 115200
s.timeout = 0.5
s.dtr = False
s.rts = False
s.open()
s.dtr = False
s.rts = True
time.sleep(0.1)
s.rts = False
end = time.time() + seconds
out = sys.stdout.buffer
while time.time() < end:
    data = s.read(4096)
    if data:
        out.write(data)
        out.flush()
' "$1" "$2"
}

# Waits for the user to plug the cable again: the port goes, then comes back.
wait_replug() {
  limit=${SOLITE_REPLUG_SECONDS:-60}
  echo "box: unplug the USB cable and plug it again (waiting up to $limit s)..."
  gone=false
  waited=0
  while [ "$waited" -lt "$limit" ]; do
    if [ -e "$PORT" ]; then
      $gone && return 0
    else
      gone=true
    fi
    sleep 1
    waited=$((waited + 1))
  done
  echo "box: the port $PORT did not come back within $limit s." >&2
  exit 4
}

log_fail() {
  echo "box: $1" >&2
  echo "box: last 40 lines of the serial output ($LOGF):" >&2
  tail -n 40 "$LOGF" >&2
  exit 1
}

cmd_log() {
  if ! $RAW; then [ -n "$IMAGE" ] || usage "log needs --image box|sim|radio (or --raw)"; fi
  if [ -n "$FROM_FILE" ]; then
    [ -f "$FROM_FILE" ] || box_fail "$FROM_FILE is not a file."
    LOGF=$FROM_FILE
  else
    resolve_port
    if [ -z "$SECONDS_" ]; then
      case "$IMAGE" in sim) SECONDS_=240 ;; radio) SECONDS_=300 ;; *) SECONDS_=60 ;; esac
    fi
    case "$SECONDS_" in '' | *[!0-9]*) usage "--seconds takes a number" ;; esac
    $REPLUG && wait_replug
    LOGF=$WORK/serial-$(date +%Y%m%d-%H%M%S).log
    echo "box: capturing $SECONDS_ s of $PORT at 115200 baud..."
    if [ "$IMAGE" = radio ]; then
      # Shown as it comes (the user sees the pairing window open), and kept in the log file.
      rm -f "$WORK/capture-failed"
      { capture "$PORT" "$SECONDS_" || : >"$WORK/capture-failed"; } | tee "$LOGF" >&2
      [ ! -e "$WORK/capture-failed" ] || { echo "box: cannot read $PORT (log: $LOGF)." >&2; exit 4; }
    else
      capture "$PORT" "$SECONDS_" >"$LOGF" || { echo "box: cannot read $PORT (log: $LOGF)." >&2; exit 4; }
    fi
    echo "box: serial log: $LOGF"
  fi
  if $RAW; then
    exit 0
  fi

  VERSION=$(fw_version)
  [ -n "$VERSION" ] || box_fail "cannot read the workspace version from Cargo.toml."
  fw_broken "$LOGF" box && log_fail "the firmware panicked or boot-looped (a reset after the banner)."
  if grep -a 'memory: internal' "$LOGF" | grep -aq 'psram none'; then
    log_fail "PSRAM not found (the memory line says psram none): the box has 2 MB in its package."
  fi
  # The radio's own failures first: a radio that answers at no speed also lacks the lines after it.
  if [ "$IMAGE" = radio ]; then
    reason=$(fw_check_radio "$LOGF") || log_fail "$reason"
  fi
  missing=$(fw_missing_line "$LOGF" "$IMAGE")
  [ -z "$missing" ] || log_fail "the log is missing $missing."
  if $KEPT && grep -aq 'storage: formatted' "$LOGF"; then
    log_fail "storage: formatted at the second boot: the partition was not kept."
  fi
  if [ "$IMAGE" = sim ]; then
    reason=$(fw_check_engine "$LOGF") || log_fail "$reason"
    grep -aq 'saves: [0-9]* written to flash' "$LOGF" || log_fail "the log has no saves: line (is this the measuring image, feature flash-saves?)."
  fi

  grep -aF "solite $VERSION" "$LOGF" | head -n 1
  grep -a 'memory: internal' "$LOGF" | head -n 1
  grep -a 'storage: ' "$LOGF" | tail -n 1
  if [ "$IMAGE" = radio ]; then
    # The link chosen (the radio's own `network:` line is printed below).
    grep -aE 'network: (wifi|ethernet) \(setting ' "$LOGF" | tail -n 1
  else
    grep -a 'network: ' "$LOGF" | tail -n 1
  fi
  if [ "$IMAGE" = box ]; then
    grep -a 'radio: none' "$LOGF" | tail -n 1
  elif [ "$IMAGE" = radio ]; then
    grep -a 'radio: uart ' "$LOGF" | tail -n 1
    grep -a 'ezsp: ' "$LOGF" | tail -n 1
    grep -a 'radio address: ' "$LOGF" | tail -n 1
    grep -aE 'network: (formed|resumed)' "$LOGF" | tail -n 1
    grep -aE 'pairing: (open for|not opened)' "$LOGF" | head -n 1
    grep -a 'serial: ' "$LOGF" | tail -n 1
    grep -a 'heap: ' "$LOGF" | tail -n 1
    # The link's lines (spec 023 R8.1), when they show.
    grep -a 'wifi: ' "$LOGF" | tail -n 1
    grep -a 'clock: ' "$LOGF" | tail -n 1
    grep -a 'serving: ' "$LOGF" | tail -n 1
    grep -a 'memory-network: ' "$LOGF" | tail -n 1
  else
    grep -a 'engine: ' "$LOGF" | tail -n 1
    grep -a 'saves: [0-9]* written to flash' "$LOGF" | tail -n 1
  fi
}

# Talks to the radio image's serial console (spec 022 R9). Standard input: the network's name and
# its password, one per line (`set`), nothing (`forget`). The JSON is built here, from those two
# lines: the password is in no argument, no environment variable and no file. Opening the port
# resets the box (the USB bridge), so the line is sent once the console says `console: ready`.
# Exit 0 done, 1 refused or not connected, 4 no answer. $1 the port, $2 `set` or `forget`.
console_exchange() {
  [ -n "$SOLITE_SERIAL_CONSOLE" ] || fw_find_idf
  ${PY:-python3} -c '
import json, os, re, select, subprocess, sys, time

port, mode = sys.argv[1], sys.argv[2]
fake = os.environ.get("SOLITE_SERIAL_CONSOLE")
wait = float(os.environ.get("SOLITE_WIFI_WAIT_SECONDS") or 60)
answer_wait = float(os.environ.get("SOLITE_WIFI_ANSWER_SECONDS") or 15)
secrets = []
if mode == "set":
    ssid = sys.stdin.readline().rstrip("\r\n")
    password = sys.stdin.readline().rstrip("\r\n")
    secrets = [ssid] + ([password] if password else [])
    line = "wifi-set " + json.dumps({"ssid": ssid, "password": password})
else:
    line = "wifi-forget"

def clean(text):
    for secret in secrets:
        text = text.replace(secret, "<hidden>")
    return text

if fake:
    import atexit
    proc = subprocess.Popen([fake, port], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
    atexit.register(lambda: proc.poll() is None and proc.kill())
    fd = proc.stdout.fileno()
    def send(data):
        proc.stdin.write(data)
        proc.stdin.flush()
    def read(timeout):
        ready, _, _ = select.select([fd], [], [], max(timeout, 0))
        return os.read(fd, 4096) if ready else b""
else:
    import serial
    s = serial.Serial()
    s.port = port
    s.baudrate = 115200
    s.timeout = 0.2
    s.dtr = False
    s.rts = False
    s.open()
    s.dtr = False
    s.rts = False
    def send(data):
        s.write(data)
        s.flush()
    def read(timeout):
        s.timeout = max(min(timeout, 0.5), 0.01)
        return s.read(4096)

buffer = b""
def lines_until(deadline):
    # The complete lines that arrive before the deadline, one at a time.
    global buffer
    while True:
        while b"\n" in buffer:
            one, buffer = buffer.split(b"\n", 1)
            text = one.decode("utf-8", "replace").strip()
            # The firmware answers through its log: its prefix (level, time, module) and colours
            # are dropped.
            logged = LOG_LINE.match(text)
            yield logged.group(1) if logged else text
        left = deadline - time.time()
        if left <= 0:
            return
        data = read(left)
        if not data and fake and proc.poll() is not None:
            return
        buffer += data

LOG_LINE = re.compile(r"^(?:\x1b\[[0-9;]*m)?[EWIDV] \(\d+\) \S+: (.*?)(?:\x1b\[0m)?$")

def no_answer():
    print("box: no answer from Solite\u0027s console on %s: is the radio image running?" % port)
    sys.exit(4)

if not fake:
    # Opening the serial port may reset the box (the DTR and RTS lines of the USB-serial chip): the console
    # exists again about 1.5 s into the boot, and a line sent before is lost. Wait for its marker;
    # a box that did not reset never prints it, so the wait is bounded.
    ready_by = time.time() + 12
    for text in lines_until(ready_by):
        if text.endswith("console: ready"):
            break
send((line + "\n").encode())
want = "wifi: settings saved" if mode == "set" else "wifi: settings removed"
answer = None
for text in lines_until(time.time() + answer_wait):
    if text.startswith("wifi: settings"):
        answer = text
        break
if answer is None:
    no_answer()
if answer.startswith("wifi: settings refused"):
    print("box: " + clean(answer))
    sys.exit(1)
if mode == "forget":
    print("box: Wi-Fi settings removed")
    sys.exit(0)
last = answer
for text in lines_until(time.time() + wait):
    if not text.startswith("wifi:"):
        continue
    last = text
    found = re.match(r"wifi: connected, address (\d+\.\d+\.\d+\.\d+)(?:, name ([^,\s]+))?", text)
    if found:
        address, name = found.group(1), found.group(2) or "?"
        print("box: Wi-Fi connected, address %s, name %s; open http://%s/" % (address, name, address))
        sys.exit(0)
print("box: " + clean(last))
sys.exit(1)
' "$1" "$2"
}

# Restores the terminal's echo, whatever way the script ends.
TTY_STATE=
restore_tty() {
  [ -z "$TTY_STATE" ] || stty "$TTY_STATE" </dev/tty 2>/dev/null
  TTY_STATE=
}

cmd_wifi() {
  if $FORGET; then
    resolve_port
    code=0
    : | console_exchange "$PORT" forget || code=$?
    exit "$code"
  fi
  if ! (exec </dev/tty) 2>/dev/null; then
    echo "box: no terminal to ask for the Wi-Fi settings, nothing done" >&2
    exit 1
  fi
  resolve_port
  name=
  pass=
  printf "box: Wi-Fi network name: " >/dev/tty
  IFS= read -r name </dev/tty
  name=$(printf '%s' "$name" | tr -d '\r')
  [ -n "$name" ] || { echo "box: no network name given, nothing sent" >&2; exit 1; }
  # The password is read with the echo off, restored on any exit (Ctrl-C included).
  TTY_STATE=$(stty -g </dev/tty 2>/dev/null)
  trap 'restore_tty; exit 130' INT TERM HUP
  trap restore_tty EXIT
  stty -echo </dev/tty 2>/dev/null
  printf "box: Wi-Fi password (empty for an open network, not shown): " >/dev/tty
  IFS= read -r pass </dev/tty
  restore_tty
  printf '\n' >/dev/tty
  pass=$(printf '%s' "$pass" | tr -d '\r')
  code=0
  # `printf` is a built-in of the shell: the two lines never appear in a process list.
  printf '%s\n%s\n' "$name" "$pass" | console_exchange "$PORT" set || code=$?
  name=
  pass=
  exit "$code"
}

case "$COMMAND" in
  identify) cmd_identify ;;
  backup) cmd_backup ;;
  restore) cmd_restore ;;
  flash) cmd_flash ;;
  log) cmd_log ;;
  wifi) cmd_wifi ;;
  *) usage "unknown command $COMMAND" ;;
esac
