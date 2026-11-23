# Shared by scripts/qemu.sh and scripts/box.sh (source it, do not run it): the ESP32 tools, the
# partition table, the application image and the checks of a boot log.
#
# The caller sets ROOT (the repository's folder) and defines `fw_fail "<message>"`, which prints
# the message its own way and exits. Variables set here: ESPRESSIF, IDF_PATH, PY (ESP-IDF's
# Python), ESPTOOL, ESPEFUSE, GEN (commands, without `--chip`), APP_OFFSET, SLOT_BYTES.
#
# Tests point SOLITE_ESPRESSIF at a fake ESP-IDF tree instead of firmware/.embuild/espressif.

# The engine's limits, in KB (spec 016 R4.4): heap peak (D27) and internal RAM minimum free.
PEAK_LIMIT_KB=350
INTERNAL_MIN_KB=64

# The version is the workspace's, not written anywhere else.
fw_version() {
  sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' "$ROOT/Cargo.toml"
}

# ESP-IDF's Python environment and ESP-IDF itself, downloaded by the firmware build.
fw_find_idf() {
  ESPRESSIF=${SOLITE_ESPRESSIF:-$ROOT/firmware/.embuild/espressif}
  IDF_PATH=$(ls -d "$ESPRESSIF"/esp-idf/*/ 2>/dev/null | tail -n 1)
  IDF_PATH=${IDF_PATH%/}
  PY=$(ls "$ESPRESSIF"/python_env/*/bin/python 2>/dev/null | tail -n 1)
  [ -n "$IDF_PATH" ] && [ -n "$PY" ] || fw_fail "ESP-IDF was not found in firmware/.embuild (build the firmware first)."
}

# The tools: esptool.py, espefuse.py and gen_esp32part.py from the PATH when there, else ESP-IDF's.
fw_find_tools() {
  if command -v esptool.py >/dev/null 2>&1 && command -v espefuse.py >/dev/null 2>&1 \
    && command -v gen_esp32part.py >/dev/null 2>&1; then
    ESPTOOL=esptool.py
    ESPEFUSE=espefuse.py
    GEN=gen_esp32part.py
    return 0
  fi
  fw_find_idf
  ESPTOOL="$PY -m esptool"
  ESPEFUSE="$PY -m espefuse"
  GEN="$PY $IDF_PATH/components/partition_table/gen_esp32part.py"
}

# The bytes of a size as the partition table prints it: 4M, 64K or 0x400000.
fw_bytes() {
  case "$1" in
    *M) echo $((${1%M} * 1048576)) ;;
    *K) echo $((${1%K} * 1024)) ;;
    *) echo $(($1)) ;;
  esac
}

# Builds the partition table from a CSV: $1 the CSV, $2 the binary. Sets APP_OFFSET and SLOT_BYTES
# (the offset and the size of `ota_0`, read from the built table, not written anywhere).
fw_build_table() {
  $GEN "$1" "$2" >/dev/null 2>&1 || fw_fail "firmware/partitions.csv does not build."
  APP_OFFSET=$(fw_part "$2" ota_0 4)
  [ -n "$APP_OFFSET" ] || fw_fail "no ota_0 slot in firmware/partitions.csv."
  SLOT_SIZE=$(fw_part "$2" ota_0 5)
  [ -n "$SLOT_SIZE" ] || fw_fail "no size for the ota_0 slot in firmware/partitions.csv."
  SLOT_BYTES=$(fw_bytes "$SLOT_SIZE")
}

# A field of a partition of a built table: $1 the table, $2 the partition, $3 the column (4 offset,
# 5 size).
fw_part() {
  $GEN "$1" 2>/dev/null | awk -F, -v n="$2" -v c="$3" '$1 == n { gsub(/ /, "", $c); print $c }'
}

# The bootloader: from the esp-idf-sys build folder whose bootloader was built last (a folder
# rebuilt in place keeps its own date, so the folder's date would mislead). $1 the target folder.
fw_find_bootloader() {
  BOOTLOADER=$(ls -t "$1"/build/esp-idf-sys-*/out/build/bootloader/bootloader.bin 2>/dev/null | head -n 1)
  [ -f "$BOOTLOADER" ] || fw_fail "bootloader not found in $1/build."
}

# The application image: $1 the ELF, $2 the .bin; refused when larger than the `ota_0` slot.
fw_elf2image() {
  $ESPTOOL --chip esp32 elf2image --flash_mode dio --flash_size 16MB -o "$2" "$1" >/dev/null \
    || fw_fail "esptool could not make the application image of $1."
  APP_SIZE=$(wc -c < "$2" | tr -d ' ')
  [ "$APP_SIZE" -le "$SLOT_BYTES" ] || fw_fail "app.bin ($APP_SIZE bytes) is larger than the ota_0 slot ($SLOT_BYTES bytes)."
}

# A broken boot. $1 the log, $2 the way to count resets: `qemu` (the ROM's reset line, "rst:", seen
# a second time) or `box` (one after the first banner `solite <version>`: the resets before it,
# from the port opening or the script's own reset, are not counted).
fw_broken() {
  grep -aqE 'Guru Meditation Error|abort\(\) was called|panicked at' "$1" && return 0
  if [ "$2" = box ]; then
    banner=$(grep -anF -m 1 "solite $(fw_version)" "$1" | cut -d: -f1)
    [ -n "$banner" ] || return 1
    [ "$(tail -n +"$((banner + 1))" "$1" | grep -ac 'rst:')" -ge 1 ]
  else
    [ "$(grep -ac 'rst:' "$1")" -ge 2 ]
  fi
}

# The lines that must show: the banner, the memory, the storage and the network; then the engine's
# (sim image) or `radio: none` (box image). $1 the log, $2 the mode, `sim` or `box`.
fw_lines_ok() {
  grep -aqF "solite $(fw_version)" "$1" && grep -aq 'memory: internal' "$1" \
    && grep -aq 'storage: /data, ' "$1" && grep -aq 'network: .* no link started' "$1" || return 1
  if [ "$2" = sim ]; then grep -aq 'engine: ' "$1"; else grep -aq 'radio: none' "$1"; fi
}

# The first of those lines that is missing, named ($1 the log, $2 the mode); empty when all show.
fw_missing_line() {
  grep -aqF "solite $(fw_version)" "$1" || { echo "the banner (solite $(fw_version))"; return; }
  grep -aq 'memory: internal' "$1" || { echo "the memory line (memory: internal ...)"; return; }
  grep -aq 'storage: /data, ' "$1" || { echo "the storage line (storage: /data, ...)"; return; }
  # The radio image starts its Wi-Fi link (spec 023 R1.2): its line has no `no link started`.
  if [ "$2" = radio ]; then
    grep -aqE 'network: (wifi|ethernet) \(setting ' "$1" || { echo "the network line (network: ...)"; return; }
  else
    grep -aq 'network: .* no link started' "$1" || { echo "the network line (network: ...)"; return; }
  fi
  case "$2" in
    sim) grep -aq 'engine: ' "$1" || echo "the engine line (engine: ...)" ;;
    radio) fw_missing_radio_line "$1" ;;
    *) grep -aq 'radio: none' "$1" || echo "the radio line (radio: none)" ;;
  esac
}

# The first line of the radio image that is missing, named ($1 the log); empty when all show.
# The link's lines first (spec 023 R8.1): a `wifi:` line, unless the setting `ethernet` started no
# link; after a `wifi: connected` line, a `serving: http://` line.
fw_missing_radio_line() {
  if ! grep -aq 'network: ethernet (setting ' "$1"; then
    grep -aq 'wifi: ' "$1" || { echo "the Wi-Fi line (wifi: ...)"; return; }
  fi
  connected=$(grep -anF -m 1 'wifi: connected' "$1" | cut -d: -f1)
  if [ -n "$connected" ]; then
    tail -n +"$((connected + 1))" "$1" | grep -aq 'serving: http://' \
      || { echo "the serving line (serving: http://...)"; return; }
  fi
  grep -aq 'radio: uart [0-9]* baud' "$1" || { echo "the radio line (radio: uart <baud> baud)"; return; }
  grep -aq 'ezsp: ' "$1" || { echo "the ezsp line (ezsp: ...)"; return; }
  grep -aqE 'network: (formed|resumed)' "$1" || { echo "the network line (network: formed or resumed)"; return; }
  grep -aqE 'pairing: (open for|not opened)' "$1" || { echo "the pairing line (pairing: open for ... or pairing: not opened ...)"; return; }
  grep -aq 'serial: [0-9]* baud' "$1" || { echo "the serial line (serial: ... baud, due 240 s after the radio starts: run again with a larger --seconds)"; return; }
  grep -aq 'heap: internal' "$1" || echo "the heap line (heap: internal ..., due with the serial line: run again with a larger --seconds)"
}

# The radio image's log against its checks ($1 the log). Prints the reason and returns 1 when one
# fails; returns 0 when all hold. The banner counts a reboot even when the ROM's `rst:` lines are
# silenced (GPIO15 held low at reset).
fw_check_radio() {
  grep -aq 'no answer at any baud rate' "$1" && { echo "the radio answers at no speed (115200, 460800, 921600): see the failure ladder of spec 021 R8.3."; return 1; }
  grep -aq 'radio: stopped after' "$1" && { echo "the radio was restarted too often: $(grep -a 'radio: stopped after' "$1" | tail -n 1)"; return 1; }
  failed=$(grep -a 'pairing: failed:' "$1" | tail -n 1)
  [ -z "$failed" ] || { echo "the radio refused to open joining: $failed"; return 1; }
  grep -aq 'task_wdt' "$1" && { echo "a task watchdog fired (task_wdt)."; return 1; }
  refused=$(grep -a 'wifi: settings refused' "$1" | tail -n 1)
  [ -z "$refused" ] || { echo "the Wi-Fi settings were refused: $refused"; return 1; }
  [ "$(grep -acF "solite $(fw_version)" "$1")" -le 1 ] || { echo "the box rebooted (the banner shows more than once)."; return 1; }
  heap=$(grep -a 'heap: internal' "$1" | tail -n 1)
  [ -n "$heap" ] || return 0
  min=$(echo "$heap" | sed -n 's/.*heap: internal [0-9]* KB free (min \([0-9]*\) KB).*/\1/p')
  [ -n "$min" ] || { echo "cannot read the internal minimum from the heap line: $heap"; return 1; }
  [ "$min" -ge "$INTERNAL_MIN_KB" ] || { echo "the internal RAM minimum free is $min KB, below the $INTERNAL_MIN_KB KB reserve of decision D23."; return 1; }
  return 0
}

# The engine's line against its limits ($1 the log). Prints the reason and returns 1 when a limit
# is crossed; returns 0 and sets ENGINE when all hold.
fw_check_engine() {
  ENGINE=$(grep -a 'engine: ' "$1" | tail -n 1)
  PEAK=$(echo "$ENGINE" | sed -n 's/.*(peak \([0-9]*\) KB).*/\1/p')
  INTERNAL_MIN=$(echo "$ENGINE" | sed -n 's/.*internal [0-9]* KB free (min \([0-9]*\) KB).*/\1/p')
  [ -n "$PEAK" ] && [ -n "$INTERNAL_MIN" ] || { echo "cannot read the peak and the internal minimum from the engine line."; return 1; }
  [ "$PEAK" -le "$PEAK_LIMIT_KB" ] || { echo "the engine's heap peak is $PEAK KB, above the $PEAK_LIMIT_KB KB of decision D27."; return 1; }
  DEVICES=$(echo "$ENGINE" | sed -n 's/^.*engine: \([0-9]*\) devices (\([0-9]*\) reported).*/\1/p')
  REPORTED=$(echo "$ENGINE" | sed -n 's/^.*engine: \([0-9]*\) devices (\([0-9]*\) reported).*/\2/p')
  [ -n "$DEVICES" ] && [ -n "$REPORTED" ] || { echo "cannot read the reported devices from the engine line."; return 1; }
  [ "$REPORTED" -eq "$DEVICES" ] || { echo "only $REPORTED of the $DEVICES devices got a measure into the engine."; return 1; }
  grep -aq 'scenario: started' "$1" || { echo "the scenario never started."; return 1; }
  DROPPED=$(sed -n '/scenario: started/,$p' "$1" | grep -ac 'callback queue full' || true)
  [ "$DROPPED" -eq 0 ] || { echo "the radio dropped $DROPPED callbacks while the scenario played."; return 1; }
  [ "$INTERNAL_MIN" -ge "$INTERNAL_MIN_KB" ] || { echo "the internal RAM minimum free is $INTERNAL_MIN KB, below the $INTERNAL_MIN_KB KB kept for the network."; return 1; }
  return 0
}
