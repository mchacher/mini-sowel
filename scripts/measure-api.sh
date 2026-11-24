#!/bin/sh
# Measures the largest creation the interface sends (spec 019, spec 022 R10): an equipment with
# 16 data and 16 order bindings, that is 33 requests, one after the other, each on its own
# connection, against the API of a running host or box. It needs a device with a data and a
# device with an order (the same or two). It creates the zone "Measure 33" and the equipment
# "Measure 33" (the same data and order bound under 16 aliases: the core accepts it, and the bodies
# have the sizes of real ones), times each request, then deletes both (kept with --keep).
#
# Passes (exit 0) when the 33 answered 201, none was 503 `Busy`, and the total is at most 10 s.
# Otherwise exit 1, with the first answer that was not 201.
#
# usage: sh scripts/measure-api.sh <address>[:<port>] [--keep]
#   port 80 by default; a number alone is a port on 127.0.0.1.
#
# Run it on the Mac against the simulator (`cargo run -p solite-host -- --sim scenarios/garage.json
# --serve`, then `sh scripts/measure-api.sh 8080`), as often as needed. Against the box, at most once
# per milestone and inside `box.sh saves` (decision D39: each run rewrites the box's files in
# flash), for example: sh scripts/box.sh saves --budget 5 -- sh -c 'sh scripts/measure-api.sh
# "$SOLITE_BOX_ADDRESS"'. Never in a loop.
set -eu

KEEP=false
TARGET=
for arg in "$@"; do
  case "$arg" in
    --keep) KEEP=true ;;
    -*) echo "measure-api: unknown option $arg" >&2; exit 2 ;;
    *) [ -z "$TARGET" ] || { echo "measure-api: one address only" >&2; exit 2; }; TARGET=$arg ;;
  esac
done
if [ -z "$TARGET" ]; then
  echo "usage: sh scripts/measure-api.sh <address>[:<port>] [--keep]" >&2
  echo "  on the Mac: sh scripts/measure-api.sh 8080 (host --sim --serve); on the box: once per milestone, inside sh scripts/box.sh saves (D39)" >&2
  exit 2
fi
case "$TARGET" in
  '' | *[!0-9]*)
    case "$TARGET" in
      *:*) HOST_PART=${TARGET%%:*}; PORT=${TARGET##*:} ;;
      *) HOST_PART=$TARGET; PORT=80 ;;
    esac ;;
  *) HOST_PART=127.0.0.1; PORT=$TARGET ;;
esac
ADDRESS="$HOST_PART:$PORT"
API="http://$ADDRESS/api/v1"
JSON='Content-Type: application/json'
NAME="Measure 33"
LIMIT_SECONDS=10

WORK=$(mktemp -d "${TMPDIR:-/tmp}/solite-measure.XXXXXX")
ZONE=
EQUIPMENT=
cleanup() {
  status=$?
  if [ -n "$EQUIPMENT" ] && ! $KEEP; then
    curl -sS -f -X DELETE "$API/equipments/$EQUIPMENT" >/dev/null 2>&1 || echo "measure-api: could not delete the equipment $EQUIPMENT" >&2
  fi
  if [ -n "$ZONE" ] && ! $KEEP; then
    curl -sS -f -X DELETE "$API/zones/$ZONE" >/dev/null 2>&1 || echo "measure-api: could not delete the zone $ZONE" >&2
  fi
  rm -rf "$WORK"
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT TERM HUP

if ! ZONES=$(curl -sS -f --max-time 10 "$API/zones" 2>/dev/null); then
  echo "measure-api: no API answers at $ADDRESS" >&2
  exit 1
fi
if echo "$ZONES" | python3 -c '
import sys, json
def names(zones):
    for z in zones:
        yield z["name"]
        yield from names(z.get("children", []))
sys.exit(0 if sys.argv[1] in names(json.load(sys.stdin)) else 1)' "$NAME"; then
  echo "measure-api: a zone named $NAME exists already: nothing done" >&2
  exit 1
fi

DEVICES=$(curl -sS -f --max-time 10 "$API/devices")
# The id of the first data (or order) of the first device that has one; empty when none.
first_id() {
  echo "$DEVICES" | python3 -c '
import sys, json
for d in json.load(sys.stdin):
    if d.get(sys.argv[1]):
        print(d[sys.argv[1]][0]["id"])
        break' "$1"
}
DATA=$(first_id data)
ORDER=$(first_id orders)
if [ -z "$ORDER" ]; then
  echo "measure-api: no device with an order: pair a relay first" >&2
  exit 1
fi
if [ -z "$DATA" ]; then
  echo "measure-api: no device with a data: pair a sensor or a relay first" >&2
  exit 1
fi

# The `id` of a JSON object read on the standard input; empty when there is none.
id_of() {
  python3 -c '
import sys, json
try:
    print(json.loads(sys.stdin.read())["id"])
except (ValueError, KeyError, TypeError):
    pass'
}

ZONE=$(curl -sS -f --max-time 10 -X POST "$API/zones" -H "$JSON" -d "{\"name\":\"$NAME\"}" | id_of)
if [ -z "$ZONE" ]; then
  echo "measure-api: could not create the zone $NAME" >&2
  exit 1
fi

# One request of the 33: $1 its number, $2 the method, $3 the path, $4 the body. Writes the answer
# to $WORK/body, and `<status> <seconds>` to $WORK/times at the end of the file.
send() {
  result=$(curl -sS -o "$WORK/body" --max-time 30 -w '%{http_code} %{time_total}' -X "$2" "$API$3" -H "$JSON" -d "$4" 2>/dev/null) || result="000 0"
  echo "$1 $result" >>"$WORK/times"
  status=${result%% *}
  if [ "$status" != 201 ] && [ ! -s "$WORK/first-bad" ]; then
    echo "request $1: status $status, $(head -c 200 "$WORK/body" 2>/dev/null)" >"$WORK/first-bad"
  fi
  [ "$status" = 201 ]
}

: >"$WORK/times"
: >"$WORK/first-bad"
if send 1 POST /equipments "{\"name\":\"$NAME\",\"type\":\"switch\",\"zoneId\":\"$ZONE\"}"; then
  EQUIPMENT=$(id_of <"$WORK/body")
fi
if [ -n "$EQUIPMENT" ]; then
  i=1
  while [ "$i" -le 16 ]; do
    send $((i + 1)) POST "/equipments/$EQUIPMENT/data-bindings" "{\"deviceDataId\":\"$DATA\",\"alias\":\"m$i\"}" || :
    i=$((i + 1))
  done
  i=1
  while [ "$i" -le 16 ]; do
    send $((i + 17)) POST "/equipments/$EQUIPMENT/order-bindings" "{\"deviceOrderId\":\"$ORDER\",\"alias\":\"o$i\"}" || :
    i=$((i + 1))
  done
fi

# The summary, from the times: how many 201, how many 503, the total, the slowest.
SUMMARY=$(awk '
  { n++; if ($2 == 201) ok++; if ($2 == 503) busy++; total += $3; if ($3 > slow) { slow = $3; at = $1 } }
  END { printf "%d %d %d %.3f %.3f %d", n, ok, busy, total, slow, at }' "$WORK/times")
set -- $SUMMARY
SENT=$1; OK=$2; BUSY=$3; TOTAL=$4; SLOWEST=$5; AT=$6
echo "measure: 33 requests, $OK answered 201, $BUSY Busy, total $TOTAL s, slowest $SLOWEST s (request $AT)"

if [ "$OK" -ne 33 ] || [ "$BUSY" -ne 0 ]; then
  echo "measure: first answer that was not 201: $(cat "$WORK/first-bad")" >&2
  [ "$SENT" -eq 33 ] || echo "measure: only $SENT requests were sent" >&2
  exit 1
fi
if awk -v t="$TOTAL" -v m="$LIMIT_SECONDS" 'BEGIN { exit !(t > m) }'; then
  echo "measure: the total, $TOTAL s, is over $LIMIT_SECONDS s: the interface's Create would feel slow" >&2
  exit 1
fi
