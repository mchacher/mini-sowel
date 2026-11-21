#!/bin/sh
# Boots the ESP32 firmware in Espressif's QEMU and checks that it starts.
#
#   sh scripts/qemu.sh
#
# Builds the firmware (release), assembles a 16 MB flash image, starts QEMU (machine esp32, 2 MB of
# PSRAM, chip revision 3), and waits for the banner and the memory line. Exit 0 and both lines
# printed when they show; exit 1 and the last 40 lines of the serial output when the firmware
# panics, reboots, or shows nothing within 60 seconds. QEMU is always stopped, also on Ctrl-C.
# Installs QEMU into firmware/.embuild/ at the first run (a download, like ESP-IDF's).
set -e
cd "$(dirname "$0")/.."
ROOT=$(pwd)

[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
if [ -f "$HOME/export-esp.sh" ]; then
  . "$HOME/export-esp.sh"
else
  echo "qemu: the ESP32 toolchain is not set up (espup install, see README.md)." >&2
  exit 1
fi

fail() { echo "qemu: $1" >&2; exit 1; }

# The version is the workspace's, not written here.
VERSION=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' Cargo.toml)
[ -n "$VERSION" ] || fail "cannot read the workspace version from Cargo.toml."

TARGET_DIR=firmware/target/xtensa-esp32-espidf/release
ESPRESSIF=$ROOT/firmware/.embuild/espressif
TIMEOUT=60

# QEMU's system libraries on macOS (Homebrew).
if [ "$(uname -s)" = "Darwin" ]; then
  LIBS="libgcrypt glib pixman sdl2 libslirp"
  # shellcheck disable=SC2086
  if ! command -v brew >/dev/null 2>&1 || ! brew list --formula $LIBS >/dev/null 2>&1; then
    echo "qemu: QEMU needs libraries from Homebrew. Run: brew install $LIBS" >&2
    exit 1
  fi
fi

echo "== Firmware build"
(cd firmware && cargo build --release --quiet)

# ESP-IDF's tools: the Python environment and ESP-IDF itself, downloaded by the firmware build.
IDF_PATH=$(ls -d "$ESPRESSIF"/esp-idf/*/ 2>/dev/null | tail -n 1)
IDF_PATH=${IDF_PATH%/}
PY=$(ls "$ESPRESSIF"/python_env/*/bin/python 2>/dev/null | tail -n 1)
[ -n "$IDF_PATH" ] && [ -n "$PY" ] || fail "ESP-IDF was not found in firmware/.embuild (build the firmware first)."

find_qemu() { ls "$ESPRESSIF"/tools/qemu-xtensa/*/qemu/bin/qemu-system-xtensa 2>/dev/null | tail -n 1; }
QEMU=$(find_qemu)
if [ -z "$QEMU" ]; then
  echo "== Installing QEMU into firmware/.embuild (download)"
  IDF_TOOLS_PATH=$ESPRESSIF IDF_PATH=$IDF_PATH "$PY" "$IDF_PATH/tools/idf_tools.py" install qemu-xtensa
  QEMU=$(find_qemu)
  [ -n "$QEMU" ] || fail "QEMU is still missing after its installation."
fi

# The bootloader: from the esp-idf-sys build folder whose bootloader was built last (a folder
# rebuilt in place keeps its own date, so the folder's date would mislead).
BOOTLOADER=$(ls -t "$TARGET_DIR"/build/esp-idf-sys-*/out/build/bootloader/bootloader.bin 2>/dev/null | head -n 1)
[ -f "$BOOTLOADER" ] || fail "bootloader not found in $TARGET_DIR/build."

WORK=firmware/target/qemu
mkdir -p "$WORK"
LOG=$WORK/serial.log
: > "$LOG"

# The partition table: firmware/partitions.csv (the one the box is flashed with), built here; the
# application goes at the offset of `ota_0` in the built table, read from it, not written here.
PARTITIONS=$WORK/partition-table.bin
GEN="$PY $IDF_PATH/components/partition_table/gen_esp32part.py"
$GEN firmware/partitions.csv "$PARTITIONS" >/dev/null 2>&1 || fail "firmware/partitions.csv does not build."
APP_OFFSET=$($GEN "$PARTITIONS" 2>/dev/null | awk -F, '$1 == "ota_0" { gsub(/ /, "", $4); print $4 }')
[ -n "$APP_OFFSET" ] || fail "no ota_0 slot in firmware/partitions.csv."

echo "== Flash image (16 MB)"
ESPTOOL="$PY -m esptool --chip esp32"
$ESPTOOL elf2image --flash_mode dio --flash_size 16MB -o "$WORK/app.bin" "$TARGET_DIR/solite-firmware" >/dev/null
$ESPTOOL merge_bin --fill-flash-size 16MB -o "$WORK/flash.bin" \
  0x1000 "$BOOTLOADER" 0x8000 "$PARTITIONS" "$APP_OFFSET" "$WORK/app.bin" >/dev/null

# eFuses of a chip revision 3 (the values ESP-IDF's own `idf.py qemu` uses), so that the
# bootloader accepts CONFIG_ESP32_REV_MIN_3.
"$PY" -c "import binascii,sys; sys.stdout.buffer.write(binascii.unhexlify('00000000000000000000000000800000000000000000100000000000000000000000000000000000' + '0' * 168))" > "$WORK/efuse.bin"

QPID=
cleanup() {
  if [ -n "$QPID" ]; then
    kill "$QPID" 2>/dev/null || true
    wait "$QPID" 2>/dev/null || true
  fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM HUP

echo "== QEMU"
"$QEMU" -nographic -monitor none -machine esp32 -m 2M \
  -drive "file=$WORK/flash.bin,if=mtd,format=raw" \
  -drive "file=$WORK/efuse.bin,if=none,format=raw,id=efuse" \
  -global driver=nvram.esp32.efuse,property=drive,value=efuse \
  -global driver=timer.esp32.timg,property=wdt_disable,value=true \
  -serial "file:$LOG" </dev/null >/dev/null 2>"$WORK/qemu.err" &
QPID=$!

# A broken boot: a panic, or the ROM's reset line ("rst:") seen a second time.
broken() {
  grep -aqE 'Guru Meditation Error|abort\(\) was called|panicked at' "$LOG" && return 0
  [ "$(grep -ac 'rst:' "$LOG")" -ge 2 ]
}
ok() {
  grep -aqF "solite $VERSION" "$LOG" && grep -aq 'memory: internal' "$LOG"
}
show_failure() {
  echo "qemu: $1" >&2
  echo "qemu: last 40 lines of the serial output:" >&2
  tail -n 40 "$LOG" >&2
  [ -s "$WORK/qemu.err" ] && { echo "qemu: QEMU said:" >&2; cat "$WORK/qemu.err" >&2; }
  exit 1
}

elapsed=0
while [ "$elapsed" -lt "$TIMEOUT" ]; do
  broken && show_failure "the firmware panicked or rebooted."
  kill -0 "$QPID" 2>/dev/null || show_failure "QEMU stopped by itself."
  if ok; then
    # A short grace period: a reboot right after the banner must not pass.
    sleep 3
    broken && show_failure "the firmware panicked or rebooted."
    grep -aF "solite $VERSION" "$LOG"
    grep -a 'memory: internal' "$LOG"
    exit 0
  fi
  sleep 1
  elapsed=$((elapsed + 1))
done
show_failure "no banner and memory line within $TIMEOUT seconds."
