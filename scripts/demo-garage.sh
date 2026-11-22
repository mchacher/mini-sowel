#!/bin/sh
# Sets up the garage of the demos (specs 010 to 014) through the API of a running host
# (`--sim scenarios/garage.json --serve`, after `--pair` or after adding the devices from the
# interface): the zone "Garage", the paired devices placed in it (simulated TH01, MS01, or the real SNZB-02 (optional), SNZB-03PR2, ZBMINIR2),
# their equipments (climate and motion sensors, the light), and the
# motion-light recipe (spec 012) with a timeout of 20 s. Prints the ids.
# The same garage is made from the interface's Topology and Equipments pages (spec 018).
# usage: sh scripts/demo-garage.sh [port]      (port 8080 by default)
set -eu

PORT="${1:-8080}"
API="http://127.0.0.1:$PORT/api/v1"
JSON='Content-Type: application/json'

# The `id` of a JSON object read on the standard input.
# An empty or unexpected answer (a call that failed) stops the script with a clear message.
id_of() {
  python3 -c '
import sys, json
text = sys.stdin.read()
try:
    print(json.loads(text)["id"])
except (ValueError, KeyError, TypeError):
    sys.exit("demo-garage: the host did not answer with an object that has an id: " + (text.strip()[:200] or "empty answer"))'
}

ZONE=""
# Once the zone exists, any failure leaves a half-built garage: say how to remove it (the zone's
# equipments and devices stay; nothing is deleted here).
undo_hint() {
  status=$?
  if [ "$status" -ne 0 ] && [ -n "$ZONE" ]; then
    echo "demo-garage: failed after creating the zone $ZONE; to undo it:" >&2
    echo "  curl -sS -f -X DELETE $API/zones/$ZONE" >&2
  fi
}
trap undo_hint EXIT

if ! ZONES=$(curl -sS -f "$API/zones"); then
  echo "demo-garage: no host answers on port $PORT: start it with --sim scenarios/garage.json --serve" >&2
  exit 1
fi
if echo "$ZONES" | python3 -c '
import sys, json
def names(zones):
    for z in zones:
        yield z["name"]
        yield from names(z.get("children", []))
sys.exit(0 if "Garage" in names(json.load(sys.stdin)) else 1)'; then
  echo "demo-garage: a zone named Garage exists already: nothing done" >&2
  exit 1
fi

DEVICES=$(curl -sS -f "$API/devices")
# The id of the first paired device whose model is one of the arguments (empty when none is).
device_id() {
  echo "$DEVICES" | python3 -c "
import sys, json
models = sys.argv[1:]
ids = [d['id'] for d in json.load(sys.stdin) if d.get('model') in models]
print(ids[0] if ids else '')" "$@"
}
# The simulated models and the real ones: the climate sensor (TH01, SNZB-02) is optional, the
# motion sensor (MS01, SNZB-03, SNZB-03PR2) and the light (ZBMINIR2) are needed by the recipe.
CLIMATE_DEVICE=$(device_id TH01 SNZB-02)
MOTION_DEVICE=$(device_id MS01 SNZB-03 SNZB-03PR2)
LIGHT_DEVICE=$(device_id ZBMINIR2)
if [ -z "$MOTION_DEVICE" ]; then
  echo "demo-garage: no motion sensor is paired (MS01, SNZB-03 or SNZB-03PR2): pair first" >&2
  exit 1
fi
if [ -z "$LIGHT_DEVICE" ]; then
  echo "demo-garage: the device ZBMINIR2 is missing: pair first" >&2
  exit 1
fi

CREATED=$(curl -sS -f -X POST "$API/zones" -H "$JSON" -d '{"name":"Garage"}')
ZONE=$(echo "$CREATED" | id_of)
echo "zone      $ZONE"
# Places a device in the garage and binds an equipment to it; prints the equipment's id and
# leaves it in EQUIPMENT (no subshell: the line must reach the standard output).
equip() {
  DEVICE=$1; NAME=$2; TYPE=$3
  curl -sS -f -X PUT "$API/devices/$DEVICE" -H "$JSON" -d "{\"zoneId\":\"$ZONE\"}" > /dev/null
  EQUIPMENT=$(curl -sS -f -X POST "$API/equipments" -H "$JSON" \
    -d "{\"name\":\"$NAME\",\"type\":\"$TYPE\",\"zoneId\":\"$ZONE\",\"deviceIds\":[\"$DEVICE\"]}" | id_of)
  echo "equipment $EQUIPMENT  $NAME"
}
if [ -n "$CLIMATE_DEVICE" ]; then equip "$CLIMATE_DEVICE" "Garage climate" sensor; fi
equip "$MOTION_DEVICE" "Garage motion" sensor
equip "$LIGHT_DEVICE" "Garage light" light_onoff
LIGHT=$EQUIPMENT
INSTANCE=$(curl -sS -f -X POST "$API/recipe-instances" -H "$JSON" \
  -d "{\"recipeId\":\"motion-light\",\"params\":{\"zone\":\"$ZONE\",\"lights\":[\"$LIGHT\"],\"timeout\":\"20s\"}}" | id_of)
echo "instance  $INSTANCE  motion-light, timeout 20s"
echo "Stop the host (Ctrl-C) and start it again: the recipe runs from the next start."
