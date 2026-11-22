# Spec 022: The serving rules in the core, the interface packed for the flash, the scripts on an address

## Context

Roadmap increment E3c (milestone E, "Solite in the box"), part 1 of 2. E3c gives the radio image a
Wi-Fi link and lets it serve, itself, Sowel's HTTP API, its WebSocket (`/ws`) and the interface,
precompressed in its flash. E3c is cut in two specs (the user's choice, 2026-11-22):

- **this spec, 022**: everything that can be built and checked on the Mac, with no box and nothing
  flashed. The rules the box's server needs move into the core or are added there (who may call,
  the Wi-Fi settings, the retries, the console's line rules, the slots, the static headers, the
  live session rules); a new crate packs the built interface, gzip-compressed, into a table the
  firmware can embed; the scripts learn to target an address and to keep the home across a flash;
  a new script measures the 33 requests. Everything is checked by `sh scripts/validate.sh` and the
  host program; the host's behaviour and the four firmware images are unchanged.
- **spec 023, `specs/023-box-serves-over-wifi/`**: the firmware's Wi-Fi link, SNTP, the HTTP server
  and its workers, `/ws` on the box, the serial console, the embedding of the interface in the radio
  image, the flash of the box with the user's yes (D6), the demo and the measures on the box. It
  depends on this spec.

The roadmap asks two measures in E3c: the internal RAM with Wi-Fi up, and the 33 sequential requests
that the interface sends to create an equipment with 16 data and 16 order bindings (spec 019's risk,
roadmap inbox line of 2026-11-22): no 503 `Busy`, time recorded. This spec builds the script that
drives the 33 requests and checks it against the Mac host; spec 023 runs it on the box.

Decisions taken by the user before E3c's spec (not reopened here; those that concern the firmware
are carried out by spec 023):

1. Wi-Fi only in E3c; Ethernet becomes its own increment, E3e.
2. The Wi-Fi settings are a file, `/data/wifi.json`, on the FAT partition, written through a new
   sub-command of `scripts/box.sh` that reads the password without echoing or logging it; nothing
   in git. NVS is rejected. `network.json` keeps only `link`.
3. The API is open on the local network (no password until J4), acceptable because the radio image
   is a development image in no public release before J4 (D32). The pairing route stays 404 on the
   box (D28). The `Host` rule accepts private-network addresses and the box's name. A new decision,
   D33, records it; this spec writes it, since it adds the rule and the gzip packing.
4. The server is ESP-IDF's HTTP server through `esp-idf-svc`, reusing
   `solite_core::api::handle_stream`, `Reply` and `LiveHub`; the core stays hardware-free. The
   limits stay: 4 requests at once, 16 KB bodies, 4 live clients, 2 orders at once.
5. The interface is embedded in the firmware, gzip-compressed at build time, hashed names, immutable
   cache, served from flash with no copy to RAM; first load at most 170 KB gzip.

One fact found while writing, which changes how decision 2 is carried out: `box.sh flash` erases
`storage` as well as `nvs` (spec 020, `scripts/box.sh` `cmd_flash`), so a `wifi.json` written once
would be lost at the next flash, like the paired devices and `zigbee.json`. R8 adds
`flash --image radio --keep-storage`, and R9 writes the settings through the running firmware's
serial console (built by spec 023), not through esptool, so that writing them never erases the home.

## Goals

1. The core holds every rule the box's server needs, tested on the Mac: the Wi-Fi settings, the
   link's lines and retries, the console's line rules, who may call over the local network, the
   pairing route's refusal there, the slots, the static headers, the live session rules.
2. The rules that the Mac host already had move into the core without any change of the host's
   behaviour: its existing tests pass unchanged.
3. `crates/ui-embed` turns `ui/dist/` into a sorted table of gzip files that a firmware build can
   embed, bounded and reproducible.
4. `scripts/demo-garage.sh` and a new `scripts/measure-api.sh` target an address;
   `scripts/box.sh flash --keep-storage` keeps the home; `scripts/box.sh wifi` reads the settings
   without the password ever being shown, logged or kept, and sends them to the box's console.

## Out of scope

- Everything that runs in the firmware or on the box: spec 023 (`specs/023-box-serves-over-wifi/`):
  the Wi-Fi station, SNTP and the clock wait, the server, its workers and `/ws` on the box, the
  console thread, `firmware/build.rs` embedding the table of R7, `sdkconfig.radio.defaults`, the
  `log --image radio` checks of the new lines, the flash refusing a radio build without the
  interface, the memory and the 33 requests measured on the box, the run with the user's yes (D6).
- Ethernet (E3e); the access point `Solite-XXXX` (E3d); a password, Sowel's bearer token, TLS (J4);
  mDNS; the server in QEMU; the time zone (J6).
- Any change of the host's behaviour: it keeps `Reach::Loopback`, its routes, headers and bounds.

## Requirements

### R1: The Wi-Fi settings file

1. `/data/wifi.json` holds `{"ssid": "<name>", "password": "<passphrase>"}`. The core parses it
   (`solite_core::network::wifi_settings`), bounded and strict:
   - the text: at most `MAX_WIFI_BYTES` (256 bytes), else refused `too long`;
   - `ssid`: a string of 1 to 32 bytes (UTF-8), else `invalid ssid`;
   - `password`: a string, empty (an open network) or 8 to 63 printable ASCII characters, or
     exactly 64 hexadecimal digits (a raw WPA key), else `invalid password`;
   - any other field, or a text that is not a JSON object: `invalid`;
   - an empty or blank text: `empty`.
2. The parsed settings never show the password: their `Debug` form and every line built from them
   print `password: <n> characters` at most (an empty one: `open network`), never the password,
   never the SSID (the SSID is the user's network name: personal data, kept out of logs that may be
   pasted into the repository).
3. `/data/network.json` keeps `{"link": ...}` only (D24, unchanged); a `wifi` field in it is ignored
   as any other field.

### R2: The link's lines and retries, in the core

The firmware of spec 023 prints these lines; the core builds them, so they are tested on the Mac.

1. `boot_lines_radio(text, cable)`: the radio image's `network:` line. Choice `wifi` (the setting
   `wifi`, or `auto` with no cable): `network: wifi (setting <s>)`, with no `no link started`.
   Choice `ethernet` (the setting `ethernet`): `network: ethernet (setting ethernet), not in this
   image, no link started`. `boot_lines` (the box, sim and measuring images) is unchanged byte for
   byte.
2. The settings lines: missing file `wifi: no settings (/data/wifi.json missing), no link started:
   sh scripts/box.sh wifi`; refused `wifi: settings refused (<reason>), no link started`.
3. `Backoff`: 1, 2, 4, 8, 16, 32 then 60 s (doubling, capped at `WIFI_RETRY_MAX_MS` = 60 000), reset
   to 1 s by `reset`.
4. `wifi_reason(code)` names ESP-IDF's disconnection code: `network not found` (201 `NO_AP_FOUND`),
   `wrong password or refused` (15 `4WAY_HANDSHAKE_TIMEOUT`, 202 `AUTH_FAIL`, 204
   `HANDSHAKE_TIMEOUT`, 2 `AUTH_EXPIRE`), `beacon lost` (200), `reason <n>` otherwise.
5. `retry_line`: `wifi: not connected (<reason>), retry in <n> s` before the first connection,
   `wifi: lost (<reason>), retry in <n> s` after one. `connected_line`: `wifi: connected, address
   <IPv4>, name <host name>, channel <n>, rssi <dBm> dBm`.

### R3: The console's line rules, in the core

`solite_core::network::parse_console_line(line: &[u8]) -> ConsoleCommand`, which spec 023's console
thread calls on each line read from the USB serial port:

1. a line ends at `\n`; one `\r` before it is dropped; its length is counted without them;
2. at most `MAX_CONSOLE_LINE` (512) bytes; a longer line is `TooLong` and never parsed (the
   firmware's reader keeps at most 513 bytes of a line and hands them over);
3. the command is the text up to the first space: `wifi-set` followed by one space and the JSON
   (`WifiSet(WifiSettings)` or `Refused(WifiRefusal)`, by R1's parser; `wifi-set` with nothing
   after it: `Refused(Empty)`); `wifi-forget` alone (`WifiForget`; any text after it: `Unknown`);
   an empty line: `Empty`; anything else, or bytes that are not UTF-8: `Unknown`;
4. the `Debug` form of every `ConsoleCommand` holds neither the password nor the SSID (R1.2).

### R4: Who may call: `Reach` and the local network's `Host` rule

1. `solite_core::api::Request` gains `reach`: `Reach::Loopback` (the Mac host, unchanged: `Host` is
   `127.0.0.1:<port>` or `localhost:<port>`) or `Reach::Lan { name }` (the box, spec 023).
2. With `Reach::Lan`, the `Host` header is accepted, without case, when its host part is: an IPv4
   address in `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16` or `127.0.0.0/8`; `localhost`; the
   box's name, alone or followed by `.local`, `.lan`, `.home` or `.home.arpa`. Its port, when given,
   is the server's (80): `Host: 192.168.1.20` and `Host: 192.168.1.20:80` both pass. Anything else
   (a public address, an IPv6 literal, another name, a name ending in the box's name with another
   suffix, a missing header) is 403 `{"error":"Forbidden host"}`. A DNS name that an attacker points
   at the box is refused this way (DNS rebinding).
3. Under `Reach::Lan`, `GET` and `POST /api/v1/integrations/solite/permit-join` answer 404
   `{"error":"Pairing from the API is off on the box"}` (D28), before the pairing route, which is
   never reached; under `Reach::Loopback` they answer as spec 017.
4. Under `Reach::Lan` every other answer (routes, bodies, statuses, the 16 KB body bound) is the
   same as under `Reach::Loopback`, byte for byte.
5. The existing guards hold for both: a POST or PUT without `application/json` is 415, so a page of
   another site cannot send one without a CORS preflight, which neither server answers with CORS
   headers.

### R5: The host's rules moved into the core, behaviour unchanged

1. **Live sessions.** `solite_core::live::session` (new) holds, moved from `crates/host/src/ws.rs`
   unchanged: the `Origin` rule (`origin_allowed`: absent, or `http(s)://<the Host header>`, or the
   interface's development server `http://localhost:5173`, `http://127.0.0.1:5173`), `DEV_ORIGINS`,
   the subprotocol rule (`pick_subprotocol`), `TICK_MS`, `PING_SECONDS`, `WRITE_TIMEOUT_SECONDS`,
   `SILENCE_SECONDS`, `MAX_INCOMING_BYTES` and `close_reason(code)`. The host's `ws.rs` uses them.
2. **Slots.** `solite_core::api::Slots` (new, `api/slots.rs`): a counter of requests in progress,
   `try_take() -> Option<SlotGuard>`, released when the guard drops (a panic unwinding included),
   `MAX_WORKERS` = 4 (moved from `crates/host/src/serve.rs`). The host's `http_loop` uses it: a 5th
   request at once is still 503 `{"error":"Busy"}`.
3. **Static headers.** `solite_core::api::static_files` gains `STATIC_HEADERS` (`X-Frame-Options:
   DENY`, `Content-Security-Policy: frame-ancestors 'none'`, `X-Content-Type-Options: nosniff`),
   `cache_control(file)` (`no-cache` for `index.html`, `public, max-age=31536000, immutable` for
   `assets/`), both moved from `crates/host/src/static_files.rs`, and `find(table, path)`: a binary
   search in a table of `(path, bytes)` sorted by path. The route is unchanged; the host uses the
   core's headers.

### R6: The heap line and the clock's jump

1. `HeapStats` gains an optional `http_stack` (unused, total); `heap_line` adds `, http stack <s> KB
   unused of <t> KB` when present (spec 023 fills it with the workers' smallest high-water mark);
   without it the line is that of spec 021, byte for byte.
2. The engine goes on when its wall clock jumps forward by decades while it runs (the box's clock
   set by SNTP after a boot in 1970, spec 023): no error, the next save written, a device that
   reports after the jump online. This is the core's behaviour today; a test pins it.

### R7: `crates/ui-embed`: the interface packed for the flash

1. A new Mac-workspace crate, `crates/ui-embed` (a library, no hardware, tested on the Mac, used by
   spec 023 as a build dependency of the firmware), reads `ui/dist/` (or the folder in
   `SOLITE_UI_DIST`): `index.html` and the files of `assets/` (no sub-folder, at most `MAX_UI_FILES`
   = 32 files, each at most 1 MB before compression, names checked as the core's
   `static_files::route` checks them), gzips each one (level 9, through the `flate2` crate, with no
   time in the gzip header) and writes `ui_files.rs` into a given folder (the build's `OUT_DIR`): a
   table of `(path, gzip bytes)` sorted by path, whose bytes come from `include_bytes!`, so they
   stay in the application's flash.
2. It fails when `index.html` is missing, with `interface: ui/dist not built: cd ui && npm ci &&
   npm run build`, or when a bound is passed (named, with the file).
3. The table carries the marker `solite-ui <n> files, <gzip KB> KB` (a string in the image), which
   spec 023 logs at boot and checks before a flash.
4. Two runs on the same folder write the same `ui_files.rs` and the same gzip bytes.
5. The first load stays under 170 KB gzip (`ui/scripts/check-bundle-budget.mjs`, unchanged); a test
   on the real `ui/dist` (skipped when it is absent) checks the packed total under 170 KB.

### R8: `scripts/box.sh flash --keep-storage`, and one owner of a radio network

1. `sh scripts/box.sh flash --image radio --keep-storage --backup <file>` erases `nvs` and `otadata`
   only and keeps `storage` (the home, `zigbee.json`, `wifi.json`); its plan says `keep storage
   (the home, the network's backup and the Wi-Fi settings)` and the radio image's warning about the
   erase of `zigbee.json` is not printed. Without `--keep-storage` the plan is as today, and the
   warning adds `(use --keep-storage to keep them)`. `--keep-storage` with `--image box|sim` is
   refused (`--keep-storage is for the radio image: the sim image seeds /data`), exit 2.
2. The network the box resumed may be the one a Mac host formed earlier through the original
   firmware (spec 021's inbox line): `box.sh flash --image radio` prints, in its plan, `the box drives
   this radio's network itself: never start a Mac host with this network's backup on another radio
   while the box runs`. The living spec states the rule: one network, one coordinator at a time.

### R9: `scripts/box.sh wifi`: the settings sent to the box's console

1. `sh scripts/box.sh wifi [--port <device>]` asks on the terminal (`/dev/tty`, never standard
   input; no terminal: `box: no terminal to ask for the Wi-Fi settings, nothing done`, exit 1) for the
   network's name (shown as typed) and its password (terminal echo off, restored on any exit, Ctrl-C
   included, nothing sent then). It sends one line, `wifi-set <JSON>`, to the firmware's serial
   console over the USB bridge, without resetting the box (DTR and RTS low, as `capture`), and reads
   the answer:
   - `wifi: settings saved`: the script waits up to 60 s (shortened by an environment variable in
     its tests) for `wifi: connected, address <IPv4>` and prints `box: Wi-Fi connected, address
     <IPv4>, name <host name>; open http://<IPv4>/` (exit 0), or the last `wifi:` line and exit 1
     (`not connected (wrong password or refused)`, for example);
   - `wifi: settings refused (<reason>)`: printed, exit 1;
   - no answer within 5 s: `box: no answer from Solite's console on <port>: is the radio image
     running?`, exit 4. An image without the console (the box's original firmware, or a radio image
     before spec 023) answers nothing: exit 4, nothing changed on the box.
2. The JSON is built by the script's Python from the two lines it reads on its standard input (never
   from arguments, environment variables or files): the password is never in a process list, a
   file of `$TMPDIR/solite-box/`, a log or the script's output. The SSID is not printed either.
3. `sh scripts/box.sh wifi --forget` sends `wifi-forget` and reads `wifi: settings removed` (exit 0);
   no answer within 5 s: exit 4 as above.
4. In tests the console is faked through `SOLITE_SERIAL_CONSOLE` (a program that reads the line on
   its standard input and prints the answers), as `box.sh`'s other fakes.

### R10: The demo scripts on an address, and the 33 requests

1. `scripts/demo-garage.sh [<port> | <address>[:<port>]]`: a number alone is a port on `127.0.0.1`
   (unchanged); anything else is an address, port 80 by default (`sh scripts/demo-garage.sh
   192.168.1.20`). Its messages name the address; its checks are unchanged. Nothing listening:
   `demo-garage: no host answers at <address>:<port> ...`, exit 1.
2. `scripts/measure-api.sh <address>[:<port>] [--keep]` (port 80 by default; a number alone is a port
   on `127.0.0.1`) needs a device with at least one data and a device with at least one order (the
   same or two); none with an order: `no device with an order: pair a relay first`, exit 1. It
   refuses when a zone `Measure 33` exists (exit 1, nothing created). It creates that zone (not
   counted), then sends, one after the other, each on its own connection, the 33 requests of spec
   019's largest creation: `POST /api/v1/equipments` `{"name":"Measure 33",
   "type":"switch","zoneId":...}`, 16 `POST .../data-bindings` `{"deviceDataId":...,"alias":"m1"}`
   to `"m16"` and 16 `POST .../order-bindings` `{"deviceOrderId":...,"alias":"o1"}` to `"o16"` (the
   same data and order bound under 16 aliases: the core accepts it, and the bodies have the sizes of
   real ones). It times each (`curl -w`), then deletes the equipment and the zone (kept with
   `--keep`). No API answering: `measure-api: no API answers at <address>`, exit 1.
3. It prints `measure: 33 requests, <k> answered 201, <b> Busy, total <s> s, slowest <t> s (request
   <i>)` and exits 0 only when the 33 answered 201, none 503, and the total is at most 10 s (the
   interface's "Create" stays usable: about 300 ms a request at most). Otherwise exit 1, with the
   first answer that was not 201 (status and body); the equipment and the zone are deleted all the
   same (unless `--keep`).

## Living spec delta

### docs/living-spec/README.md

- Changed, "Flashes the box and brings it back": `flash --image radio --keep-storage` (keeps
  `storage`: the home, the network's backup and the Wi-Fi settings), refused with `box` and `sim`;
  the warning's `(use --keep-storage to keep them)`; the plan's ownership line and the rule "one
  network, one coordinator at a time". New: `wifi` and `wifi --forget`: the prompts on the terminal
  (the password never echoed, shown, logged or kept, the SSID not printed), the one line sent to the
  box's serial console, the answers read and the exit codes; an image without the console answers
  nothing (exit 4).
- Changed, "Runs without the dongle" (where `demo-garage.sh` is described) or the commands list:
  `demo-garage.sh` takes a port or an address (port 80 by default); new `scripts/measure-api.sh
  <address>[:<port>] [--keep]`: the 33 requests of an equipment with 16 + 16 bindings, its pass
  rule (33 × 201, no 503, at most 10 s), the zone and equipment removed after it.
- History: `| 022 | The rules for serving on the local network in the core, the interface packed
  for the flash, the scripts on an address, the 33 requests measured |`.

### Other docs (applied in the same change)

- `docs/architecture.md`: the Devices and API row (`Reach` and the local network's `Host` rule,
  `Slots`, the static headers), the Live events row (`live/session.rs` holds the origin rule and the
  constants; the host's `ws.rs` uses them), the core's network line (the Wi-Fi settings, the link's
  lines and retries, the console's line rules), a new row for `crates/ui-embed`.
- `docs/decisions.md`: D33 (below), the decision the user took before E3c's spec, with the conditions
  answered on 2026-11-22 (the home's own network, no port forwarded).
- `docs/roadmap.md`: E3c's line says it is cut into specs 022 and 023.
- `CLAUDE.md`: the demo line becomes `sh scripts/demo-garage.sh [port | address[:port]]`, and a
  line for `sh scripts/measure-api.sh <address>`.

### No longer true

Found by searching `erases`, `storage`, `ws.rs`, `serve.rs`, `127.0.0.1`, `demo-garage`:

- `docs/living-spec/README.md`, "Flashes the box and brings it back": "erases `nvs`, `otadata` and
  `storage`" → "...and `storage`, unless `--keep-storage` (radio image)".
- `docs/architecture.md`, Live events row: "in the host: `front.rs` ... and `ws.rs`" → the origin
  rule and the constants are the core's (`live/session.rs`).
- `docs/living-spec/README.md`, `README.md` and `CLAUDE.md`, wherever `demo-garage.sh [port]` is
  written: the argument is a port or an address.
- `docs/hardware.md`, step 5 of the flashing procedure: "erases `nvs`, `otadata` and `storage`" →
  "...unless `--keep-storage`, radio image only".
- `README.md` and `CLAUDE.md`, the `box.sh` command lines: `flash [--keep-storage]` and `wifi` are
  named.

The sentences about the box with no network ("starts no link", "no network and no HTTP server",
"the box does not serve it") stay true after this spec: spec 023 rewrites them.

## Acceptance criteria

- [x] `sh scripts/validate.sh` green: the core tests (Wi-Fi settings, link lines, backoff, reasons,
      console lines, `Host` rule, pairing refusal, slots, session rules, headers, heap line, clock
      jump), the `ui-embed` tests, the script tests below; the host's existing tests pass unchanged;
      the four firmware images build and QEMU passes, unchanged.
- [x] With the host started by `cargo run -p solite-host -- --sim scenarios/garage.json --serve
      --speed 10`, `sh scripts/demo-garage.sh 127.0.0.1:8080` sets the garage up, as with `8080`.
- [x] Against the same host, `sh scripts/measure-api.sh 8080` prints 33 answered 201, 0 Busy and its
      total time, and leaves no zone `Measure 33` behind.
- [x] `sh scripts/box.sh flash --image radio --keep-storage --backup <file> --dry-run` prints a plan
      that erases `nvs` and `otadata` only, says `keep storage (...)` and shows the ownership line.
- [x] `sh scripts/box.sh wifi` never echoes the password typed; against a box without the console it
      ends with `box: no answer from Solite's console ...`, exit 4.
- [x] The interface served by the host at `http://127.0.0.1:8080/` works as before (rooms, live
      state): the moves changed nothing.

## Edge cases

- `wifi.json` empty, over 256 bytes, not JSON, an extra field, SSID empty or over 32 bytes, password
  of 7 or 64 non-hex characters: refused with the reason (R1.1).
- A console line over 512 bytes, an unknown command: `TooLong` / `Unknown`, never parsed (R3).
- A console line with `\r\n`, an empty line, `wifi-forget now`, bytes that are not UTF-8: the `\r`
  dropped, `Empty`, `Unknown`, `Unknown` (R3).
- `Host` of a public address, of an IPv6 literal, of `evil.example`, of `solite-xxxxxx.evil.example`,
  of another box's name, with a port other than 80, missing, under `Reach::Lan`: 403 (R4.2).
- `POST /api/v1/integrations/solite/permit-join` under `Reach::Lan`: 404 with its message, the
  pairing state untouched (R4.3).
- A guard of `Slots` dropped while a panic unwinds: the slot is released (R5.2).
- An asset name not in the table, `..`, `%2e`: `find` gives `None`, the route 404 with the security
  headers (R5.3).
- The wall clock jumping forward by 56 years: the engine goes on (R6.2).
- `ui/dist` missing, an interface over the bounds (33 files, a 2 MB file), a sub-folder, a name
  starting with `.` or holding `%`: `ui-embed` fails, naming the bound or the file (R7.2).
- `flash --keep-storage` with `--image sim` or `--image box`: refused, exit 2 (R8.1).
- `box.sh wifi` with no terminal, or with no console answering: refused / exit 4; Ctrl-C at the
  password prompt: echo restored, nothing sent (R9.1).
- `measure-api.sh` with a zone `Measure 33`, with no device with an order, against an API answering
  503 to one request, slower than 10 s in all, with nothing listening: exit 1 each, nothing left
  behind (R10.2-3).
- `demo-garage.sh 127.0.0.1` with nothing on port 80: exit 1 naming the address (R10.1).

## Sowel contract

Nothing new in Sowel's model: the routes, bodies, answers and events are those of specs 008 to 019
(`src/shared/types.ts`, `docs/technical/api-reference.md` of the Sowel repository), unchanged byte for
byte under both `Reach` values. The one divergence the core decides under `Reach::Lan`, the pairing
route answering 404 (D28), is reached only on the box (spec 023) and is recorded in D33.

## Decided by the user (2026-11-22)

1. **Two specs.** E3c is cut into this spec (the Mac-testable part, merged first) and spec 023 (the
   firmware and the run on the box), which depends on it.
2. **The open API on the home's network.** An API with no password on the local network is accepted
   while the same box is also the family's annex controller, before J4 brings the password, on two
   conditions written in the living spec by spec 023 and in D33: the box stays on the home's own
   network (not a guest network shared with visitors), and no port of the router is forwarded to it
   (the `Host` rule refuses public names and addresses, but a forwarded port reached by a private
   address would pass). The pairing route stays off (404), so nobody on the network can add a device.

## Decided in autonomy

- **The settings travel through the firmware's serial console**, not an esptool write: `flash`
  erases `storage` (found while writing), and writing a FAT file from outside means rebuilding the
  wear-levelled partition, which would erase the home. The console needs no flash, keeps the home,
  and lets the box confirm the connection. No MAC prompt: it writes no flash through esptool (D6 is
  about flashing); the terminal prompts are the confirmation.
- `flash --keep-storage` for the radio image, so that a new image does not cost the paired devices,
  the network's backup and the Wi-Fi settings at each flash of E3c.
- The interface packed by a Mac-tested crate (`crates/ui-embed`, gzip through `flate2`, build-time
  only, never in the box's code) into a table read with `include_bytes!`; a partition of its own was
  rejected (a second flash write for each change, two parts that can disagree, and J5's update
  carries one image).
- Gzip always, with no plain copy: half the flash and one path; every browser sends `gzip`.
- The `Host` rule: private IPv4 ranges and the box's name with four usual home suffixes; IPv6 and
  public addresses refused; `Reach` carried by the request, so the core decides and tests it.
- The console's line rules in the core (`parse_console_line`), so the 512-byte bound, the whole-line
  drop and the commands are tested on the Mac.
- The measure of the 33 requests is a script on the Mac (`scripts/measure-api.sh`), tested against the
  Mac host; pass: 33 × 201, no 503, at most 10 s in all.
- **The cut between the two specs**: `box.sh wifi`'s script side is here, since its tests fake the
  console; `box.sh log --image radio`'s new checks and `flash`'s refusal of a radio build without
  the interface are in spec 023, since they would fail or mean nothing on today's radio image;
  `firmware/build.rs` calling `ui-embed` is in spec 023, so that no image changes in this spec.
- **D33 is written by this spec**, which adds the `Host` rule of the local network, the pairing
  route's refusal and the gzip packing; spec 023 builds the rest of what it records and checks its
  text against what is built.

### Decision D33 (text written to `docs/decisions.md` in Phase 3)

> **D33: The radio image serves the API on the local network, over Wi-Fi**
>
> - **Date**: 2026-11-22
> - **Choice**: the radio image (feature `radio`) joins the Wi-Fi network of `/data/wifi.json`
>   (written by `box.sh wifi` through the firmware's serial console; never NVS, never git) and serves
>   Sowel's API, `/ws` and the interface on port 80, with no authentication. A `Host` is
>   accepted when it is a private IPv4 address (10/8, 172.16/12, 192.168/16, 127/8), `localhost` or
>   the box's name `solite-<6 hex>` (alone or with `.local`, `.lan`, `.home`, `.home.arpa`); the
>   pairing route answers 404 (D28). ESP-IDF's HTTP server through `esp-idf-svc`, 4 workers by its
>   asynchronous requests (their stacks and 16 KB body buffers in PSRAM), at most 12 connections in
>   a socket table of 24, TCP keep-alive; on `/ws` the refusals are close
>   codes after the upgrade (4003, 1013), only `bearer.none` is echoed, and a dead client is found by
>   TCP keep-alive instead of 60 s of silence; a client that cannot be written to holds the server's
>   task up to 5 s. The interface is embedded at build time, gzip only, from flash. Wi-Fi power save
>   off; SNTP with `pool.ntp.org`; the engine's saves wait at most 20 s for the clock, the radio and
>   the pairing window of a power-on do not. `flate2` is in the firmware's build (never its code).
>   The setting `ethernet` starts no link in this image. Built in two specs: the rules in the core,
>   the packing and the scripts (spec 022), the firmware (spec 023).
> - **Why**: the true north's box is reached from the home's network; the radio image is a
>   development image, in no public release (D32), so an open API on the local network is
>   accepted, also while the box is the family's annex controller (the user, 2026-11-22), on two
>   conditions: the box stays on the home's own network (not a guest network), and no router port is
>   forwarded to it. The guards are the Mac host's (Host check against DNS rebinding, JSON-only
>   bodies against forms, Origin on `/ws`), and the pairing route stays off. The settings file
>   survives a flash with `--keep-storage`, which NVS would not.

## How the user will check it

On the Mac, no box needed (step 5 optionally with the box on USB, running spec 021's image):

1. `sh scripts/validate.sh`: green.
2. `cargo run -p solite-host -- --sim scenarios/garage.json --serve --speed 10`, then open
   `http://127.0.0.1:8080/`: the interface works as before.
3. In a second terminal: `sh scripts/demo-garage.sh 127.0.0.1:8080`: the garage is set up; then
   `sh scripts/measure-api.sh 8080`: `measure: 33 requests, 33 answered 201, 0 Busy, total ... s`.
4. `sh scripts/box.sh flash --image radio --keep-storage --backup <file> --dry-run`: the plan keeps
   `storage` and shows the ownership line; with `--image sim`: refused, exit 2.
5. `sh scripts/box.sh wifi`: type a name and a password: the password is not echoed; with no
   console on the box, `box: no answer from Solite's console ...`, exit 4.
