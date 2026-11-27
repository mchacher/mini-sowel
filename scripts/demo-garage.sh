#!/bin/sh
# Sets up the garage of the demos (specs 010 to 014) through the API of a running host
# (`--sim scenarios/garage.json --serve`, after `--pair` or after adding the devices from the
# interface): the zone "Garage", the paired devices placed in it (simulated TH01, MS01, or the real SNZB-02 (optional), SNZB-03PR2, ZBMINIR2),
# their equipments (climate and motion sensors, the light), and the
# motion-light recipe (spec 012, a script recipe since spec 046: the host is started with
# `--recipe-script recipes/scripts/motion-light.js`, a box has it installed) with a timeout of 20 s.
# Prints the ids.
# The same garage is made from the interface's Topology and Equipments pages (spec 018).
# The box asks for its password (spec 029): SOLITE_PASSWORD (and SOLITE_USER, `admin` when unset);
# a box without an account is set up with it, else the script logs in. Never an argument.
# usage: SOLITE_PASSWORD=<password> sh scripts/demo-garage.sh [<port> | <address>[:<port>]]
#   a number alone is a port on 127.0.0.1 (8080 by default); anything else is an address, port 80
#   by default (the box on the home's network: sh scripts/demo-garage.sh 192.168.1.20).
set -eu

TARGET="${1:-8080}"
case "$TARGET" in
  '' | *[!0-9]*)
    case "$TARGET" in
      *:*) HOST_PART=${TARGET%%:*}; PORT=${TARGET##*:} ;;
      *) HOST_PART=$TARGET; PORT=80 ;;
    esac ;;
  *) HOST_PART=127.0.0.1; PORT=$TARGET ;;
esac
API="http://$HOST_PART:$PORT/api/v1"
JSON='Content-Type: application/json'
AUTH_DIR=$(mktemp -d "${TMPDIR:-/tmp}/solite-demo.XXXXXX")
AUTH_FILE="$AUTH_DIR/auth"
: >"$AUTH_FILE"

# The account (spec 029): the password comes from SOLITE_PASSWORD, never from an argument, and is
# never printed; the user name from SOLITE_USER (admin when unset). The box is asked whether it
# needs an account: set it up when it does, else log in. The token goes to curl in a header file.
SOLITE_USER=${SOLITE_USER:-admin}
export SOLITE_USER
if [ -z "${SOLITE_PASSWORD:-}" ]; then
  echo "demo-garage: set SOLITE_PASSWORD (the password of the box)" >&2
  exit 2
fi
export SOLITE_PASSWORD
# Prints the host's `error` text of an answer file, else the status.
refusal() {
  python3 -c '
import sys, json
try:
    print(json.load(open(sys.argv[1])).get("error", "refused"))
except (ValueError, OSError, AttributeError):
    print("refused")' "$1"
}
# Sets the account up (when the host asks for one) or logs in; writes the token header file.
sign_in() {
  if ! STATUS=$(curl -sS -f --max-time 10 "$API/auth/status" 2>/dev/null); then
    return 1
  fi
  case "$STATUS" in *'"setupRequired":true'*) ROUTE=setup ;; *) ROUTE=login ;; esac
  export ROUTE
  python3 -c '
import os, json
body = {"username": os.environ["SOLITE_USER"], "password": os.environ["SOLITE_PASSWORD"]}
if os.environ["ROUTE"] == "setup":
    body["displayName"] = os.environ["SOLITE_USER"]
print(json.dumps(body))' | curl -sS -o "$AUTH_DIR/answer" -w '%{http_code}' --max-time 30 \
    -X POST "$API/auth/$ROUTE" -H "$JSON" -d @- >"$AUTH_DIR/code" 2>/dev/null || echo 000 >"$AUTH_DIR/code"
  CODE=$(cat "$AUTH_DIR/code")
  if [ "$CODE" != 200 ] && [ "$CODE" != 201 ]; then
    echo "demo-garage: $ROUTE refused ($CODE): $(refusal "$AUTH_DIR/answer")" >&2
    exit 1
  fi
  python3 -c '
import sys, json
print("Authorization: Bearer " + json.load(open(sys.argv[1]))["accessToken"])' "$AUTH_DIR/answer" >"$AUTH_FILE"
  rm -f "$AUTH_DIR/answer" "$AUTH_DIR/code"
}

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
  rm -rf "$AUTH_DIR"
  if [ "$status" -ne 0 ] && [ -n "$ZONE" ]; then
    echo "demo-garage: failed after creating the zone $ZONE; to undo it:" >&2
    echo "  curl -sS -f -X DELETE $API/zones/$ZONE -H 'Authorization: Bearer <access token>'" >&2
  fi
}
trap undo_hint EXIT

if ! sign_in; then
  echo "demo-garage: no host answers at $HOST_PART:$PORT: start it with --sim scenarios/garage.json --serve (or give the box's address)" >&2
  exit 1
fi
ZONES=$(curl -sS -f -H "@$AUTH_FILE" "$API/zones")
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

# No recipe is built in (spec 046): the instance below needs the motion-light script.
if ! curl -sS -f -H "@$AUTH_FILE" "$API/recipes" | python3 -c '
import sys, json
sys.exit(0 if any(r.get("id") == "motion-light" for r in json.load(sys.stdin)) else 1)'; then
  echo "demo-garage: the box has no motion-light recipe: install it first (the host: --recipe-script recipes/scripts/motion-light.js)" >&2
  exit 1
fi

DEVICES=$(curl -sS -f -H "@$AUTH_FILE" "$API/devices")
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

CREATED=$(curl -sS -f -H "@$AUTH_FILE" -X POST "$API/zones" -H "$JSON" -d '{"name":"Garage"}')
ZONE=$(echo "$CREATED" | id_of)
echo "zone      $ZONE"
# Places a device in the garage and binds an equipment to it; prints the equipment's id and
# leaves it in EQUIPMENT (no subshell: the line must reach the standard output).
equip() {
  DEVICE=$1; NAME=$2; TYPE=$3
  curl -sS -f -H "@$AUTH_FILE" -X PUT "$API/devices/$DEVICE" -H "$JSON" -d "{\"zoneId\":\"$ZONE\"}" > /dev/null
  EQUIPMENT=$(curl -sS -f -H "@$AUTH_FILE" -X POST "$API/equipments" -H "$JSON" \
    -d "{\"name\":\"$NAME\",\"type\":\"$TYPE\",\"zoneId\":\"$ZONE\",\"deviceIds\":[\"$DEVICE\"]}" | id_of)
  echo "equipment $EQUIPMENT  $NAME"
}
if [ -n "$CLIMATE_DEVICE" ]; then equip "$CLIMATE_DEVICE" "Garage climate" sensor; fi
equip "$MOTION_DEVICE" "Garage motion" sensor
equip "$LIGHT_DEVICE" "Garage light" light_onoff
LIGHT=$EQUIPMENT
INSTANCE=$(curl -sS -f -H "@$AUTH_FILE" -X POST "$API/recipe-instances" -H "$JSON" \
  -d "{\"recipeId\":\"motion-light\",\"params\":{\"zone\":\"$ZONE\",\"lights\":[\"$LIGHT\"],\"timeout\":\"20s\"}}" | id_of)
echo "instance  $INSTANCE  motion-light, timeout 20s"
echo "Stop the host (Ctrl-C) and start it again: the recipe runs from the next start."
