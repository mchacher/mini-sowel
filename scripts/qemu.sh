#!/bin/sh
# Boots the ESP32 firmware in Espressif's QEMU and checks the engine's memory.
#
#   sh scripts/qemu.sh          the image QEMU runs (feature `sim`): the engine on the simulated
#                               radio with the realistic home of firmware/fixtures/
#   sh scripts/qemu.sh --box    the image of the box (no `sim`): storage, network, `radio: none`
#
# Builds the firmware (release), assembles a 16 MB flash image (bootloader, partition table,
# application; the data partition stays blank, so every run formats it), starts QEMU (machine
# esp32, 2 MB of PSRAM, chip revision 3), and waits for the banner, the memory line, the `storage:`
# and `network:` lines and, for the sim image, the `engine:` line, which it prints. Exit 0 when they
# show and the engine's heap peak is at most 300 KB (decision D20) and its internal RAM minimum
# free at least 64 KB (spec 015's reserve); exit 1 and the last 40 lines of the serial output when
# a limit is crossed, the firmware panics, reboots, or a line is missing within the time allowed.
# QEMU is always stopped, also on Ctrl-C.
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
# The engine's limits, in KB (spec 016 R4.4): heap peak (D20) and internal RAM minimum free.
PEAK_LIMIT_KB=300
INTERNAL_MIN_KB=64

MODE=sim
case "$1" in
  "") ;;
  --box) MODE=box ;;
  *) fail "usage: sh scripts/qemu.sh [--box]" ;;
esac
# Time allowed: the blank partition is formatted, then 12 s of scenario time on a slow emulator.
if [ "$MODE" = sim ]; then TIMEOUT=240; else TIMEOUT=60; fi

# QEMU's system libraries on macOS (Homebrew).
if [ "$(uname -s)" = "Darwin" ]; then
  LIBS="libgcrypt glib pixman sdl2 libslirp"
  # shellcheck disable=SC2086
  if ! command -v brew >/dev/null 2>&1 || ! brew list --formula $LIBS >/dev/null 2>&1; then
    echo "qemu: QEMU needs libraries from Homebrew. Run: brew install $LIBS" >&2
    exit 1
  fi
fi

echo "== Firmware build ($MODE image)"
if [ "$MODE" = sim ]; then
  (cd firmware && cargo build --release --features sim --quiet)
else
  (cd firmware && cargo build --release --quiet)
fi

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
SLOT_SIZE=$($GEN "$PARTITIONS" 2>/dev/null | awk -F, '$1 == "ota_0" { gsub(/ /, "", $5); print $5 }')
APP_SIZE=$(wc -c < "$WORK/app.bin" | tr -d ' ')
[ -n "$SLOT_SIZE" ] || fail "no size for the ota_0 slot in firmware/partitions.csv."
# The table prints sizes as 4M, 64K or 0x400000.
case "$SLOT_SIZE" in
  *M) SLOT_BYTES=$((${SLOT_SIZE%M} * 1048576)) ;;
  *K) SLOT_BYTES=$((${SLOT_SIZE%K} * 1024)) ;;
  *) SLOT_BYTES=$((SLOT_SIZE)) ;;
esac
[ "$APP_SIZE" -le "$SLOT_BYTES" ] || fail "app.bin ($APP_SIZE bytes) is larger than the ota_0 slot ($SLOT_BYTES bytes)."
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
# The lines that must show: the banner, the memory, the storage and the network; then the engine's
# (sim image) or `radio: none` (box image).
ok() {
  grep -aqF "solite $VERSION" "$LOG" && grep -aq 'memory: internal' "$LOG" \
    && grep -aq 'storage: /data, ' "$LOG" && grep -aq 'network: .* no link started' "$LOG" || return 1
  if [ "$MODE" = sim ]; then grep -aq 'engine: ' "$LOG"; else grep -aq 'radio: none' "$LOG"; fi
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
    grep -a 'storage: ' "$LOG"
    grep -a 'network: ' "$LOG"
    if [ "$MODE" = box ]; then
      grep -a 'radio: none' "$LOG"
      exit 0
    fi
    ENGINE=$(grep -a 'engine: ' "$LOG" | tail -n 1)
    echo "$ENGINE"
    PEAK=$(echo "$ENGINE" | sed -n 's/.*(peak \([0-9]*\) KB).*/\1/p')
    INTERNAL_MIN=$(echo "$ENGINE" | sed -n 's/.*internal [0-9]* KB free (min \([0-9]*\) KB).*/\1/p')
    [ -n "$PEAK" ] && [ -n "$INTERNAL_MIN" ] || show_failure "cannot read the peak and the internal minimum from the engine line."
    [ "$PEAK" -le "$PEAK_LIMIT_KB" ] || show_failure "the engine's heap peak is $PEAK KB, above the $PEAK_LIMIT_KB KB of decision D20."
    [ "$INTERNAL_MIN" -ge "$INTERNAL_MIN_KB" ] || show_failure "the internal RAM minimum free is $INTERNAL_MIN KB, below the $INTERNAL_MIN_KB KB kept for the network."
    exit 0
  fi
  sleep 1
  elapsed=$((elapsed + 1))
done
show_failure "the lines of the $MODE image did not all show within $TIMEOUT seconds."
