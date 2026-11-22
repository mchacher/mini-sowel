# Spec 023: The box serves the API and the interface over Wi-Fi

## Context

Roadmap increment E3c (milestone E, "Solite in the box"), part 2 of 2. Since spec 021 the radio
image drives the EFR32 over the UART and runs the engine on the box, but it has no network: the box
is reached only through its USB serial log, and the API and the interface run on the Mac host only.
E3c gives the radio image a Wi-Fi link and lets it serve, itself, Sowel's HTTP API, its WebSocket
(`/ws`) and the interface, precompressed in its flash. The garage of milestone B is then set up on
the box from the Mac with `scripts/demo-garage.sh <box address>`, and the box's interface is opened
from the Mac's browser.

E3c is cut in two specs (the user's choice, 2026-11-22). **This spec depends on spec 022**
(`specs/022-serve-core-and-ui-embed/`), merged first, which put in the core and the scripts
everything testable on the Mac: the Wi-Fi settings and their parser, the link's lines, `Backoff`
and `wifi_reason`, the console's line rules (`parse_console_line`), `Reach` and the local network's
`Host` rule with the pairing route's refusal, `Slots`, the static headers and `find`, the live
session rules (`live::session`), the heap line's http stack, `crates/ui-embed`, `box.sh flash
--keep-storage`, `box.sh wifi`'s script side, `demo-garage.sh` on an address and
`scripts/measure-api.sh`; and wrote decision D33. This spec builds the firmware on them, flashes the
box with the user's yes (D6) and runs the demo and the measures on it.

The roadmap asks two measures here: the internal RAM with Wi-Fi up, and the 33 sequential requests
that the interface sends to create an equipment with 16 data and 16 order bindings (spec 019's risk,
roadmap inbox line of 2026-11-22): no 503 `Busy`, time recorded.

Decisions taken by the user before E3c's spec (not reopened here):

1. Wi-Fi only in E3c; Ethernet becomes its own increment (roadmap note below).
2. The Wi-Fi settings are a file, `/data/wifi.json`, on the FAT partition, written through
   `scripts/box.sh wifi` (spec 022) and the firmware's serial console (this spec); nothing in git.
   NVS is rejected. `network.json` keeps only `link`.
3. The API is open on the local network (no password until J4), acceptable because the radio image
   is a development image in no public release before J4 (D32), on the home's own network with no
   port forwarded (decided 2026-11-22, below). The pairing route stays 404 on the box (D28). D33
   records it.
4. The server is ESP-IDF's HTTP server through `esp-idf-svc` (`EspHttpServer`), WebSocket included,
   reusing `solite_core::api::handle_stream`, `Reply` and `LiveHub`; the core stays hardware-free,
   the adapters live in `firmware/`. Server and Wi-Fi on core 0, the engine on core 1. The limits
   stay: 4 requests at once, 16 KB bodies, 4 live clients, 2 orders at once.
5. The interface is embedded in the firmware, gzip-compressed at build time (spec 022's
   `crates/ui-embed`), hashed names, immutable cache, served from flash with no copy to RAM; it fits
   the 4 MB slot; first load at most 170 KB gzip.
6. SNTP is included (spec 021 left the clock to E3c).
7. The image is the radio image plus the network (feature `radio`). Flashing needs the user's
   explicit yes (D6); the run on the box is part of the demo.

`box.sh flash` erases `storage` unless `--keep-storage` (spec 022), which is why the settings travel
through the running firmware's console and the box is flashed with `--keep-storage` in the demo.

## Goals

1. The radio image joins the Wi-Fi network given in `/data/wifi.json`, keeps trying when the network
   is missing or lost, and sets its clock by SNTP.
2. It serves Sowel's API, `/ws` and the interface on port 80 of the local network, with the same
   routes, JSON and bounds as the Mac host, and the divergences of D33.
3. Its serial console takes the settings `scripts/box.sh wifi` sends, saves them and restarts.
4. Measured on the box: the internal RAM with Wi-Fi and the server up, and the 33 sequential requests
   of an equipment with 16 + 16 bindings.

## Out of scope

- What spec 022 built and tested on the Mac (`specs/022-serve-core-and-ui-embed/`): the core's rules,
  `crates/ui-embed`, `--keep-storage`, `box.sh wifi`'s script side, `demo-garage.sh` on an address,
  `measure-api.sh`, D33's text. This spec uses them unchanged.
- Ethernet (the IP101 PHY): its own increment, E3e (roadmap note below). In this image a setting
  `ethernet` starts no link.
- The access point `Solite-XXXX` and its page to pick the network, the fallback to it after a few
  minutes without Wi-Fi (E3d).
- A password, Sowel's bearer token, TLS (HTTPS) (J4); the pairing route on the box (404 until J4).
- mDNS (`solite-xxxxxx.local` answered by the box itself): the name is the DHCP host name only; a
  roadmap inbox line.
- The server in QEMU (Espressif's QEMU has an emulated Ethernet, `open_eth`, but no Wi-Fi): an inbox
  line; the image QEMU runs (`sim`) keeps no network.
- The box image (no feature) and the measuring image (`flash-saves`): unchanged, no link.
- The time zone and the location (J6): the box's clock is UTC, the dates ISO 8601 UTC as Sowel's.
- The save stall of `devices.json` (spec 020's inbox line) and the realistic home on the radio image.

## Requirements

### R1: The link at boot (radio image)

1. The radio image reads `network.json` as today and logs the choice with the core's
   `boot_lines_radio` (spec 022). Then:
   - choice `wifi` (the setting `wifi`, or `auto` with no cable: the image detects no cable): it
     reads `wifi.json`, parses it with the core's `wifi_settings` (spec 022) and starts the Wi-Fi
     station when the settings are valid;
   - choice `ethernet` (the setting `ethernet`): `network: ethernet (setting ethernet), not in this
     image, no link started`; no Wi-Fi either (D24: one link, the one chosen).
2. The `network:` line of the radio image becomes `network: wifi (setting <s>)` (no longer
   `no link started` when Wi-Fi starts); the box, sim and measuring images keep
   `network: wifi (setting auto), no link started` byte for byte (their checks in
   `scripts/lib/firmware.sh` and `scripts/qemu.sh` are unchanged).
3. Settings missing: `wifi: no settings (/data/wifi.json missing), no link started: sh scripts/box.sh
   wifi`. Settings refused: `wifi: settings refused (<reason>), no link started`. A read error of the
   file: `wifi: <file>: cannot read: <reason>, no link started`. In all three cases the engine runs
   as in spec 021, with no server.
4. Wi-Fi station, WPA2-Personal at least (WPA3 accepted by the access point's choice; WEP and open
   networks refused unless the password is empty, which asks for an open network and logs
   `wifi: open network, no encryption`). The DHCP host name is `solite-<6 hex digits>`, the last
   three bytes of the station's MAC address in lower case: the box's name.
5. Power save is off (`WIFI_PS_NONE`): the box is powered by USB or mains, and the modem sleep of
   the default mode delays every packet by up to a beacon interval (about 100 ms, more with a large
   DTIM), which would slow each of the 33 requests and the live batches. No software coexistence
   exists between the ESP32's Wi-Fi and the EFR32's Zigbee (two chips); the `wifi: connected` line
   shows the Wi-Fi channel and the `network:` line of the radio the Zigbee channel, so an overlap is
   seen in the log (Zigbee 15 lies between Wi-Fi channels 1 and 6, 20 between 6 and 11, 25 above 11).
6. ESP-IDF's own `wifi` log tag is set to warnings only, so its info lines (which name the SSID) do
   not reach the serial log.
7. Lines: `wifi: connecting` once at start; `wifi: connected, address <IPv4>, name <host name>,
   channel <n>, rssi <dBm> dBm` (the core's `connected_line`) at each connection; `serving:
   http://<IPv4>/` after it.

### R2: Retries, a lost link, a wrong password

1. A failed connection or a lost link is retried forever, after the core's `Backoff` (1 s doubling to
   60 s, spec 022), reset to 1 s at each connection.
2. The reason is named by the core's `wifi_reason` from ESP-IDF's disconnection code, and the line
   is the core's `retry_line`: `wifi: not connected (<reason>), retry in <n> s` before the first
   connection, `wifi: lost (<reason>), retry in <n> s` after one. A wrong password is retried like
   the rest (an access point that rejects a right password while it restarts gives the same codes);
   the line says which it may be.
3. The engine, the radio and the recipes never wait for Wi-Fi beyond R3.2: the garage keeps running
   with no network. The server keeps running across a lost link; its clients reconnect.

### R3: The clock

1. SNTP starts with the station, servers `pool.ntp.org` and the one the DHCP server gives (option
   42, when present); ESP-IDF resyncs every hour. The clock is UTC.
2. **Only the dates wait for the clock; the radio and the pairing window do not.** The engine
   thread starts at once, as in spec 021: it loads the home, starts the radio and its network, and
   opens the pairing window of a power-on boot (spec 021 R5, its 180 s counted from its opening as
   today) without waiting for the clock. What waits is the engine's date-stamping on the flash: its
   saves are held until the first sync or `CLOCK_WAIT_MS` (20 000 ms) from the start of the station,
   whichever comes first, then the files due are saved; a normal boot thus writes no 1970 date to
   the flash. The line, once: `clock: synced, <ISO 8601 UTC>` or `clock: not synced after 20 s,
   dates count from 1970 until it is`. Without Wi-Fi settings nothing is held.
3. A later sync logs `clock: synced, <ISO 8601 UTC>` once. The dates taken before it are from
   1970 (`lastSeen`, recipe log, data timestamps, in memory during the wait, on the flash when the
   clock never synced); the core's rules apply unchanged after the jump (spec 022 R6.2 pins that the
   engine goes on; a device silent by the new clock, one paired during the wait included, turns
   offline until it speaks, as any silence).
4. Random numbers: once the Wi-Fi station has started, the ESP32's generator is fed by its radio and
   the entropy switch of spec 021 (`bootloader_random_enable`, which must not run while Wi-Fi runs)
   is skipped; with no Wi-Fi it is used as in spec 021.

### R4: The server

1. **Threads and cores.** The engine thread stays on core 1 (spec 021). ESP-IDF's Wi-Fi task, its
   HTTP server task, the API's workers, the live thread and the console thread are pinned to core 0.
   The engine thread loads the home, then hands the shared home and its live hub to `main`, which
   starts the server; the server never starts without a home (a home that cannot be read stops the
   engine as in spec 021, and no server starts: `serving: none (<reason>)`).
2. **Port and limits.** Port 80, IPv4. At most 12 open connections (`max_open_sockets`; ESP-IDF
   closes the least recently used one beyond it), in an lwIP socket table of 24
   (`CONFIG_LWIP_MAX_SOCKETS`: the 12, the listener, the server's control socket, DNS, SNTP, DHCP's,
   and room left so that the closing of the least recently used connection is reached long before
   the table is full and never meets an internal socket), request head at most 1024 bytes
   (`CONFIG_HTTPD_MAX_REQ_HDR_LEN`; a longer one is answered 431 by ESP-IDF), receive and send
   timeouts 5 s, TCP keep-alive on every connection (idle 30 s, interval 5 s, 3 probes).
3. **The API.** Every request other than `/ws`, `/` and `/assets/*` goes through
   `solite_core::api::handle_stream`, exactly as on the Mac, with the request's `reach` set to
   `Reach::Lan { name }` (spec 022 R4). It is answered by a pool of 4 worker threads (ESP-IDF's
   asynchronous requests, `httpd_req_async_handler_begin`), so a request waiting for an order (up to
   12 s) never holds the server's task; a 5th request in progress is answered 503
   `{"error":"Busy"}` by the server's task. The counting is the core's `Slots` (spec 022).
4. **Bodies.** Each worker owns one body buffer of `MAX_BODY_BYTES` + 1 bytes (16 KB + 1,
   `BODY_BUFFER_BYTES`), allocated once in PSRAM when the server starts (`heap_caps_malloc` with
   `MALLOC_CAP_SPIRAM`) and kept for the server's life: 4 × 16 KB = 64 KB of PSRAM, never on a
   worker's stack and never in internal RAM. A body is read into it up to 16 KB + 1: a longer one, or a
   `Content-Length` above 16 KB, is answered 413 `{"error":"Body too large"}` without reading the
   rest, and the connection is closed. A body sent with `Transfer-Encoding: chunked` (no length) is
   answered 411 `{"error":"Length required"}`.
5. **Lists and orders.** `Reply::List` is sent in HTTP chunks, the home taken for each chunk and
   released while it is written (D15). `Reply::Order` releases the home, waits for the radio loop's
   answer and sends `order_response`, as the host does. At most 2 orders from the API at once
   (`solite_core::orders::MAX_ORDERS` = 2, `crates/core/src/orders/queue.rs`; not
   `devices::MAX_ORDERS`, the 4 orders a device may hold), 503 `Busy` beyond, unchanged.
6. **The interface.** `/` and `/assets/<name>` are answered by the core's `static_files::route`, then
   from the table embedded in the firmware (R7) through the core's `find`: the file's gzip bytes are
   sent from flash with `Content-Encoding: gzip`, `Vary: Accept-Encoding`, its content type, the
   core's `cache_control` and `STATIC_HEADERS` (spec 022). A name not in the table: 404
   `{"error":"Not found"}` with the security headers. These are served by the server's task (short,
   from flash), not by the workers.
7. **The pairing route.** On the box, `GET` and `POST /api/v1/integrations/solite/permit-join`
   answer 404 `{"error":"Pairing from the API is off on the box"}` (D28), decided in the core under
   `Reach::Lan` (spec 022 R4.3). The boot window of spec 021 is unchanged.
8. **Security headers on API answers**: unchanged (none, as on the Mac).

### R5: Live events on the box

1. `GET /ws` is registered as ESP-IDF's WebSocket handler with the subprotocol `bearer.none` (the
   one the interface sends, `ui/src/store/useLive.ts`). ESP-IDF answers the handshake itself; then
   the firmware checks, in this order: the `Host` (spec 022 R4.2), else closes with 4003 `Forbidden
   host`; the `Origin` (the core's `live::session::origin_allowed`), else 4003 `Origin not
   allowed`; a slot in the hub (`LiveHub::connect`), else 1013 `Try again later` (the 5th client).
   Then it sends the welcome message of spec 013.
2. The live thread (core 0) every 200 ms (`TICK_MS`) sends each client its batch as one text
   message in fragments (`[`, each event, `]`) taken with `begin_batch` and `pop`, no batch string
   built, through `esp-idf-svc`'s detached sender (which hands each frame to the server's task); a
   close asked by the hub (1013 on a full queue) is sent; a ping every 30 s.
3. A client is dropped when a frame cannot be written within 5 s (the server's send timeout) or when
   TCP keep-alive finds it gone (R4.2, about 45 s). The 60 s silence rule of the host is not used on
   the box: ESP-IDF answers the pings and pongs itself and does not hand them to the firmware.
4. A message over 1 KB (`MAX_INCOMING_BYTES`) closes with 1009; a text or binary message is a
   subscribe (`LiveHub::subscribe`), as on the Mac.
5. The constants and the subprotocol rule are the core's `live::session` (spec 022), shared with
   the host.

### R6: The serial console

1. A thread `console` (core 0, 4 KB of stack, internal RAM: it writes the flash) reads bytes from
   the USB serial port (UART0, ESP-IDF's console, with its driver installed so reads block) and logs
   `console: ready` once at boot. It keeps at most 513 bytes of a line (`MAX_CONSOLE_LINE` + 1),
   discards the rest up to the next `\n`, and hands each line to the core's `parse_console_line`
   (spec 022 R3). `TooLong` is logged `console: line over 512 bytes, ignored`; `Unknown` is logged
   `console: unknown command`; `Empty` is ignored with no log. The line itself is never logged.
2. `WifiSet`: written to `/data/wifi.json` through the storage's save (temporary file, sync,
   rename), then `wifi: settings saved`. `Refused(<reason>)`: `wifi: settings refused (<reason>)`,
   nothing changed. `WifiForget`: removes the file, `wifi: settings removed`.
3. After a save or a removal, the engine is asked to stop (its stop flag), saves its files (forced
   save, as the host's Ctrl-C does), and the ESP32 restarts by software (no pairing window: not a
   power-on, spec 021); if the engine has not stopped within 10 s, it restarts anyway. After a
   removal it then starts no link (R1.3).
4. With this console, `sh scripts/box.sh wifi` (spec 022 R9) ends as it describes: the settings
   saved, the box restarted, its address printed; `wifi --forget` removes them.

### R7: The interface in the firmware

1. `firmware/build.rs` calls spec 022's `crates/ui-embed` (a build dependency) only with the feature
   `radio`, from `../ui/dist` (or `SOLITE_UI_DIST`), writing `ui_files.rs` into `OUT_DIR`; the other
   images embed nothing and build without `ui/dist`. `cargo:rerun-if-changed` covers `ui/dist`. A
   missing `ui/dist` fails the build with `ui-embed`'s message (`interface: ui/dist not built: cd ui
   && npm ci && npm run build`).
2. The radio image logs the table's marker `solite-ui <n> files, <gzip KB> KB` at boot.
3. `box.sh flash --image radio` refuses a radio build without the embedded interface (its ELF lacks
   the marker `solite-ui `), with the build command, nothing written.
4. `validate.sh --only firmware` builds `ui/dist` first when missing (`ui/node_modules` needed,
   `cd ui && npm ci` named otherwise), checks the radio ELF holds `solite-ui ` and the name of
   `ui/dist`'s hashed script (the embed is the current build), and that `CONFIG_HTTPD_WS_SUPPORT=y`
   is in the radio build's `sdkconfig` and in no other. CI's firmware job installs Node and `ui/`'s
   packages.
5. The radio application, interface included, fits the 4 MB `ota_0` slot (4096 KB,
   `firmware/partitions.csv`; `fw_elf2image` refuses a larger `app.bin` in `validate.sh` and `box.sh
   flash`). Today's radio image (spec 021, no network) loads about 930 KB into flash (952 585 bytes
   of loaded sections in its release ELF of 2026-11-22, read with `xtensa-esp32-elf-size`; `app.bin`
   adds only headers and padding): about 3.1 MB of margin. Wi-Fi, lwIP, the HTTP server, the
   WebSocket and SNTP add about 500 to 700 KB (**estimate**, the usual size of ESP-IDF's Wi-Fi
   station examples), the interface about 54 KB of gzip: about 1.5 to 1.7 MB in all, under half the
   slot. The size of the built `app.bin` is written in the result of the run. If it ever exceeds the
   slot, **the work stops and the figures are reported to the user**: the partition table (D25) is
   not changed and nothing is removed from the image on the quiet.

### R8: `scripts/box.sh` on a radio image with Wi-Fi

1. `log --image radio` checks the lines of spec 021, plus: a `wifi:` line; when a `wifi: connected`
   line shows, a `serving: http://` line after it; and prints the last `wifi:`, `clock:`,
   `serving:` and `memory-network:` lines. It fails on `wifi: settings refused` (named). A box with
   no settings passes (its `wifi: no settings` line is printed). The box and sim images' checks are
   unchanged (`no link started` still required).
2. `backup` warns, as today, that a backup may hold Wi-Fi credentials: once `wifi.json` exists it
   does (in plain text, as `zigbee.json`'s keys).

### R9: What is measured on the box

1. At boot the `memory:` line is unchanged (before Wi-Fi). Ten seconds after the first `serving:
   http://` line the image logs once `memory-network: internal <n> KB free (largest block <m> KB),
   psram <p> KB free of 2048 KB`: the cost of Wi-Fi and the server is the difference.
2. The `heap:` line every 240 s (spec 021) gains `, http stack <s> KB unused of <t> KB` (the core's
   `heap_line`, spec 022 R6.1, fed with the smallest high-water mark among the 4 workers,
   `WORKER_STACK_BYTES` = 10 KB each, in PSRAM: R9.4).
3. Pass criteria, written with the figures in `docs/memory-budget.md` ("On the box"):
   - the `heap:` internal minimum stays at or above 64 KB (the reserve, D23) through the whole run:
     boot, Wi-Fi up, the interface open with one live client for 2 minutes, the garage set up, the 33
     requests (`box.sh log --image radio` fails below it);
   - the projection for the realistic home, written beside: the measuring image's internal minimum
     on the box (107 KB, spec 020) minus the cost of R9.1. When it is below 64 KB, or the run's
     minimum is, **the work stops and the figures are reported to the user**: the limit is not
     lowered, no further setting is turned on the quiet. The knobs left, in the order they would be
     proposed, as a decision of the user: `CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL` from 4096 down to
     2048 or 512 (more of the engine in PSRAM, slower; spec 016 measured its effect), then streaming
     the home's loading (roadmap inbox). The first knob, the workers' stacks in PSRAM, is on from the
     start (R9.4);
   - the 33 requests: R10.
4. **Expected outcome, and the workers' stacks in PSRAM from the start.** The budget's figures give
   the projection before any run: 107 KB (the measuring image's internal minimum) minus the 70 KB the
   budget sets for Wi-Fi leaves about **37 KB, under the 64 KB floor**, before any stack of the
   server is counted. The first knob is therefore on from the start, not left to the run:
   `CONFIG_SPIRAM_ALLOW_STACK_EXTERNAL_MEMORY=y` in `sdkconfig.radio.defaults`, and the 4 workers'
   stacks (4 × 10 KB) allocated in PSRAM (`esp_pthread_cfg_t.stack_alloc_caps` =
   `MALLOC_CAP_SPIRAM`); the workers never write the flash, which a task with its stack in PSRAM must
   not do. Their 16 KB body buffers are in PSRAM too (R4.4). What stays in internal RAM: the
   server's task (8 KB), the console (4 KB, it writes the flash), the live thread (4 KB), lwIP's
   sockets (R9.5) and Wi-Fi's own internal part. The projection becomes 107 − 70 − 16 = about
   **21 KB** with the budget's 70 KB, still under the floor; it reaches 64 KB only if Wi-Fi's real
   internal cost is about 27 KB or less, which `CONFIG_SPIRAM_TRY_ALLOCATE_WIFI_LWIP` (Wi-Fi's and
   lwIP's dynamic buffers in PSRAM) makes possible but not certain. The likely first run: the run's
   own minimum passes with room (the radio image held 234 KB at its lowest with one device, spec 021,
   so about 150 to 190 KB with Wi-Fi up), and the projection for the realistic home falls under
   64 KB, which stops the work and reports the figures with the next knob (R9.3). The subtraction is
   pessimistic: Wi-Fi's internal allocations draw partly from the 64 KB reserve, counted twice, as
   `docs/memory-budget.md` says of its own table.
5. **The lwIP socket table.** `CONFIG_LWIP_MAX_SOCKETS` is 24 rather than 16 (R4.2). Its cost, an
   **estimate** from lwIP's structures on ESP-IDF 5: about 40 bytes of table per slot whether used
   or not (8 more slots: about 0.3 KB, static), and about 0.4 KB per socket while it is open (its
   `netconn`, its TCP control block and its mailboxes, from lwIP's heap in internal RAM); the 8 extra
   slots all open at once: about 3.5 KB at most. Counted inside Wi-Fi's figure of R9.1, not apart.
6. **The realistic home stays a projection.** The "Internal RAM with Wi-Fi up" row of
   `docs/memory-budget.md` is measured for the home the run holds (one or two devices); for the
   realistic home it stays an **estimate**, a subtraction (R9.3), marked E. The realistic home is
   measured on the radio image with Wi-Fi up later, by a roadmap inbox line (roadmap notes): the
   radio image seeded with the realistic home of `firmware/fixtures/` (as the measuring image is),
   Wi-Fi and the server up, its `heap:` minimum read.

### R10: The 33 requests on the box

1. `sh scripts/measure-api.sh <box address>` (spec 022 R10.2-3) is run against the box, with the
   SNZB-03PR2 and the ZBMINIR2 paired; pass: 33 answered 201, none 503, total at most 10 s.
2. The time is written in `docs/memory-budget.md` whatever the verdict; a failure stops the work and
   reports the figures (as R9.3).

### R11: No TCP relay of the radio on the box

1. The box's image has no TCP relay of the radio (the original firmware's port 6638): a Mac host
   started with `--radio tcp://<box>:6638` gets `connection refused` and says so, as today for a
   wrong address. The rule "one network, one coordinator at a time" and the flash plan's ownership
   line are spec 022's (R8.2).

## Living spec delta

### docs/living-spec/README.md

- Changed, "Starts and shows its name", the ESP32 bullet: the radio image, after the `network:`
  line, reads `/data/wifi.json`, starts the Wi-Fi station (or logs why not), starts SNTP, and
  starts the engine thread on core 1 as before (radio, network and the power-on pairing window at
  once; the engine's saves held until the clock is set, 20 s at most); once the home is
  loaded it serves Sowel's API, `/ws` and the interface on port 80 (see "Serves Sowel's API from the
  box"). New lines: `solite-ui <n> files, <KB> KB`, `console: ready`, `wifi: ...`, `clock: ...`,
  `serving: ...`, `memory-network: ...`; the `heap:` line gains the workers' stack. "The radio image
  has no network and no HTTP server; it is a development image." → "The radio image is a
  development image: its API is open on the local network (decision D33)."
- Changed, "Chooses its network": the radio image starts the Wi-Fi link when the choice is Wi-Fi and
  `/data/wifi.json` is valid; the choice `ethernet` starts no link in this image; the other images
  start no link. The Wi-Fi settings, their bounds, the retries (1 s doubling to 60 s, forever) and the
  reasons named.
- New section, "Serves Sowel's API from the box": port 80 on the local network; the box kept on the
  home's own network (not a guest network) with no router port forwarded to it (decided by the user,
  2026-11-22, D33); the `Host` rule (private IPv4 addresses, the box's name and its suffixes), the
  same routes and errors as the Mac host except: the pairing route 404 `Pairing from the API is off
  on the box`; on `/ws` the refusals come after the upgrade as close codes (4003 host, 4003 origin,
  1013 for a 5th client), only `bearer.none` is echoed, a dead client is found by TCP keep-alive
  instead of 60 s of silence; 411 for a body without length; 431 for a head over 1024 bytes; at most
  12 open connections, the least recently used closed beyond; 503 `Busy` beyond 4 requests at once.
  The interface is served from the firmware, gzip always (`Content-Encoding: gzip`), cache headers as
  on the Mac.
- Changed, "Shows the home in the browser": the interface also opens at `http://<box address>/` or
  `http://solite-xxxxxx/` (when the router resolves DHCP names), from the copy embedded in the radio
  image at build time.
- Changed, "Flashes the box and brings it back": `wifi` ends with the box saving the settings,
  restarting and its address printed, `wifi --forget` removes them (spec 022's sentence "an image
  without the console answers nothing" kept for the original firmware and the other images); the
  radio build refused without the embedded interface; `log --image radio`'s new lines and checks.
- Changed, "Stays within a memory budget": the radio image logs `memory-network:` and the workers'
  stack; `docs/memory-budget.md` holds the internal RAM with Wi-Fi up, the projection for the
  realistic home and the 33 requests' time on the box.
- Changed, the closing paragraph: "the firmware's radio image does the same over the serial link,
  with no network and no interface" → "the firmware's radio image does the same over the serial link
  and serves the API, the live events and the interface over Wi-Fi, without the pairing route".
- History: `| 023 | The radio image joins Wi-Fi and serves Sowel's API, its live events and the
  interface from its flash |`.

### Other docs (applied in the same change)

- `docs/architecture.md`: the Firmware row (Wi-Fi, SNTP, the server, the live thread, the console,
  the embedded interface), the Live events row (the box's sessions in `firmware/src/live_ws.rs`),
  Builds (`build.rs` embeds `ui/dist` for `radio` through `crates/ui-embed`, the new settings of
  `sdkconfig.radio.defaults`, the UI built before the radio image), Data (`/data/wifi.json`).
- `docs/memory-budget.md`: "On the box" rows "Internal RAM with Wi-Fi up" and "The 33 requests" filled;
  the internal RAM table gains the measured network cost beside its estimate; the transport row
  (no tungstenite buffers on the box); the PSRAM rows of the workers' stacks and body buffers (added
  as estimates with E3c's spec) checked against the `heap:` line's PSRAM figures.
- `docs/hardware.md`: the Wi-Fi antenna risk row: settled by the run (RSSI and channel seen; the box's
  position not recorded).
- `docs/decisions.md`: D33 (written by spec 022) checked against what is built; any difference is
  raised to the user, not rewritten on the quiet.
- `docs/roadmap.md` (roadmap notes): E3c's status after the run; E3c's line reads "Wi-Fi" only
  (Ethernet left to E3e); a new line after E3d: **E3e** "Ethernet (the IP101, RMII, pins of
  `docs/hardware.md`): a cable at boot under `auto` means Ethernet, the same server; the Ethernet
  memory figure measured" | "The box's interface opened over a cable, Wi-Fi settings absent" (the
  Ethernet line of E1's network choice, moved out of E3c). E3d's line is rewritten: "(settings can
  also be written to flash when flashing)" becomes "(settings also written through the serial
  console, `box.sh wifi`, specs 022 and 023)": the console route is the one chosen, a write at flash
  time is not (it would erase the home). Inbox lines: mDNS on the box; the server in QEMU through
  `open_eth`; Sowel's bearer token on the box's `/ws` (only `bearer.none` is echoed until J4); the
  `wifi` sub-command reboots by software only (no pairing window), so pairing a device still needs a
  power-on; the realistic home measured on the radio image with Wi-Fi and the server up (the radio
  image seeded from `firmware/fixtures/`), so the "Internal RAM with Wi-Fi up" row stops being a
  subtraction for it (R9.6).
- `README.md`: the radio image's build needs `ui/dist` first (`cd ui && npm run build`); the box line
  lists `wifi`.
- `CLAUDE.md`: the box line becomes `sh scripts/box.sh identify|log|wifi`.
- `.github/workflows/ci.yml`: the firmware job installs Node and `ui/` packages (the radio image
  embeds the built interface).

### No longer true

Found by searching `no link started`, `starts no link`, `no network`, `no HTTP server`, `does not
serve`, `with no network and no interface`, `127.0.0.1 only`, `box will`, `the box's HTTP server`,
`needs Wi-Fi (E3c)`, `Not needed before Wi-Fi is built`, `answers nothing`:

- `docs/living-spec/README.md` l.39-40: "logs its choice (`network: wifi (setting auto), no link
  started`), starts no link" → true for the box, sim and measuring images; the radio image logs
  `network: wifi (setting auto)` and starts the link.
- `docs/living-spec/README.md` l.55: "The radio image has no network and no HTTP server; it is a
  development image." → rewritten (above).
- `docs/living-spec/README.md` l.82: "the firmware starts no link." → "the radio image starts the
  Wi-Fi link; the other images start none."
- `docs/living-spec/README.md` l.248-251, "Serves Sowel's device API": "serves on `127.0.0.1` only"
  → true of the host; the box is in its own section.
- `docs/living-spec/README.md` l.545-547: "the firmware's radio image does the same over the serial
  link, with no network and no interface" → rewritten (above).
- `docs/living-spec/README.md`, "Flashes the box and brings it back": spec 022's "a radio image
  before spec 023 answers nothing" (or its wording) → the radio image answers.
- `docs/architecture.md` l.31: "which the host's `--serve` serves; the box does not serve it." →
  "which the host's `--serve` serves from disk and the radio image embeds, compressed, in its flash."
- `docs/architecture.md`, Firmware row: "the network setting read from `/data/network.json`" → adds
  the Wi-Fi link, SNTP, the server and the console of the radio image.
- `docs/memory-budget.md`: "the network rows need Wi-Fi (E3c)" and the two "needs Wi-Fi (E3c)" cells
  → the figures of the run.
- `docs/memory-budget.md`, QEMU section: "the margin that decides is the box's, with the network up
  (needs Wi-Fi, E3c)" → the measured figure.
- `docs/hardware.md`, risks: "The Wi-Fi antenna | Not described by Sonoff's pages found. | Not needed
  before Wi-Fi is built." → settled by the run.
- `docs/decisions.md` D18: "The box will use its own server (E)" stays as the record of 2026-11-21
  (decisions are dated history, not rewritten); D33 says what the box uses.
- `crates/core/src/network.rs` doc comment of `boot_lines`: "(the firmware starts no link and detects
  no cable ...)" → for the images without a link.
- `firmware/src/main.rs` module comment: "reads its network setting" → adds the Wi-Fi and the server
  of the radio image.

## Acceptance criteria

- [ ] `sh scripts/box.sh wifi` on the box: the password typed is never shown; the script prints the
      box's address; `/data/wifi.json` exists on the box (the next `log` shows `wifi: connected`).
- [ ] `http://<box address>/` opens the interface from the Mac's browser, with the rooms and the
      live state (one live client), served by the box (the Mac host not running).
- [ ] `sh scripts/demo-garage.sh <box address>` sets the garage up on the box; the motion before the
      sensor switches the ZBMINIR2, and the interface shows it live.
- [ ] `sh scripts/measure-api.sh <box address>` prints 33 answered 201, 0 Busy, and its total time;
      the figure is in `docs/memory-budget.md`.
- [ ] The `memory-network:` line, the `heap:` minimum through the run and the projection for the
      realistic home are in `docs/memory-budget.md`; the minimum is at or above 64 KB, or the run
      stopped and the figures were reported (R9.3).
- [ ] With the access point switched off, the log shows `wifi: lost (...)`, then retries; back on,
      `wifi: connected` again and the interface reconnects by itself; the garage kept working.
- [ ] `sh scripts/validate.sh` green: the script tests below; the four firmware images built (the
      radio one with the interface embedded); QEMU unchanged.

## Edge cases

- `wifi.json` missing: `wifi: no settings ...`, no link, no server, the engine runs (R1.3).
- `wifi.json` refused (any reason of spec 022 R1.1): `wifi: settings refused (<reason>), no link
  started`, no server (R1.3).
- Wrong password: `wifi: not connected (wrong password or refused), retry in ...` forever, doubling to
  60 s; `box.sh wifi` reports it and exits 1 (R2, spec 022 R9.1).
- Network absent at boot: `not connected (network not found)`, retried; the engine runs from the
  start, its saves held 20 s, then written with 1970 dates (R2, R3.2).
- Link lost while running: `wifi: lost (...)`, retried; the live clients drop and reconnect; the
  engine and recipes go on (R2.3).
- The setting `ethernet`: no link in this image (R1.1).
- The clock never synced (no Internet behind the Wi-Fi): dates from 1970, the line says so, the engine
  works (R3.2); synced late: one `clock: synced` line, statuses follow the core's silence rules (R3.3).
- A 5th live client: closed 1013 after the upgrade, the 4 others unaffected (R5.1).
- A 5th request at once: 503 `Busy` (R4.3); a 3rd order at once: 503 `Busy` (R4.5).
- A body over 16 KB (by length or by bytes): 413, connection closed (R4.4); chunked body: 411.
- A head over 1024 bytes: 431 by ESP-IDF (R4.2).
- A 13th connection: ESP-IDF closes the least recently used one (it may be a live client, which
  reconnects) (R4.2).
- `Host` of `evil.example` on the box: 403; on `/ws`: closed 4003 (R5.1). `Origin` of another site on
  `/ws`: closed 4003.
- `POST /api/v1/integrations/solite/permit-join` on the box: 404 with its message (R4.7).
- A client that sends no `Accept-Encoding: gzip`: still gets gzip (no plain copy is kept; every browser
  accepts it; `curl --compressed` reads it).
- A client that stops reading (a sleeping laptop): dropped after a 5 s blocked write, or by keep-alive
  (R5.3); during that write the server's task is held up to 5 s (a bounded stall, D33).
- A flash save running (the 3.8 s of `devices.json`, or an `equipments.json` written after each
  binding): requests wait for the home lock only while a chunk is built; answers come late, none is
  lost; live batches are late, not dropped (the hub's bounds hold).
- `ui/dist` missing when building the radio image: the build fails with the command to run (R7.1); a
  radio build without the marker: `box.sh flash` refuses (R7.3).
- A console line over 512 bytes, an unknown command: ignored, never logged (R6.1).
- `wifi-set` while the engine is stuck: restart after 10 s anyway (R6.3).
- A power-on with the access point on: the pairing window opens before `clock: synced`; a device
  paired during the clock wait turns offline at the clock's jump until it speaks again (R3.2, R3.3).
- The built `app.bin` larger than the `ota_0` slot: refused by the build checks; the work stops and
  the figures are reported (R7.5).
- A Mac host pointed at the box's port 6638: connection refused (R11.1).
- The internal RAM minimum below 64 KB: `box.sh log` fails, the run stops, figures reported (R9.3).
- An order sent while the radio restarts (spec 021's restart ladder): answered `device did not answer`
  after 12 s, as on the Mac.

## Sowel contract

Nothing new in Sowel's model: the routes, bodies, answers and events are those of specs 008 to 019
(`src/shared/types.ts`, `docs/technical/api-reference.md` of the Sowel repository), unchanged byte for
byte. Divergences on the box only, recorded in D33: the pairing route 404 (D28); on `/ws`, the
refusals as close codes after the upgrade (Sowel and the host answer 403 and 503 before it), only
`bearer.none` echoed, TCP keep-alive in place of the 60 s silence; gzip always for the interface; 411
and 431 from ESP-IDF.

## Decided by the user (2026-11-22)

1. **Two specs.** E3c is cut into spec 022 (the Mac-testable part, merged first) and this spec (the
   firmware and the run on the box), which depends on it.
2. **The open API on the home's network.** An API with no password on the local network is accepted
   while the same box is also the family's annex controller, before J4 brings the password, on two
   conditions written in the living spec's section "Serves Sowel's API from the box" and in D33: the
   box stays on the home's own network (not a guest network shared with visitors), and no port of
   the router is forwarded to it (the `Host` rule refuses public names and addresses, but a
   forwarded port reached by a private address would pass). The pairing route stays off (404), so
   nobody on the network can add a device.

## Points to decide, from spec 022's review

- Setting `auto` with an Ethernet cable or PoE plugged: the radio image starts no Wi-Fi (`network:
  ethernet (setting auto), not in this image, no link started`), so the box is unreachable. This
  is decided on purpose, and a test pins the decision.
- The live session's `DEV_ORIGINS` (`http://localhost:5173`, `http://127.0.0.1:5173`) are accepted
  by the box once it uses `origin_allowed`: a development page on any machine of the network can
  open `/ws`. D33 decides it.
- The rows of `docs/memory-budget.md` for the HTTP buffers and the worker stacks are estimates
  (marked E): they are re-checked against the real figures on the box.

## Decided in autonomy

- ESP-IDF's asynchronous requests and a pool of 4 workers: one server task would be held 12 s by an
  order, stopping the interface and the live events.
- WebSocket refusals after the upgrade (ESP-IDF answers the handshake itself; its pre-handshake hook
  is a Kconfig option `esp-idf-svc` does not expose), TCP keep-alive in place of the silence rule.
- Power save off; ESP-IDF's `wifi` tag at warnings (its info lines name the SSID).
- `CONFIG_SPIRAM_TRY_ALLOCATE_WIFI_LWIP=y`: Wi-Fi's and lwIP's dynamic buffers go to PSRAM when they
  can (ESP-IDF's setting for a board with PSRAM), to spare the internal RAM, the tight resource.
- SNTP: `pool.ntp.org` and the DHCP server's; the engine's saves wait at most 20 s for it, so the
  dates written by a normal boot are right from the first save, and a box without Internet still
  starts. The radio, its network and the pairing window of a power-on do not wait: a 20 s delay
  would eat into the 180 s window of spec 021.
- The workers' stacks (4 × 10 KB) and their body buffers (4 × 16 KB) in PSRAM from the start: the
  budget's own figures put the projection under 64 KB without it (R9.4).
- `CONFIG_LWIP_MAX_SOCKETS` 24: room above the 12 connections so the least recently used close
  never meets an internal socket (DNS, SNTP, DHCP, the server's control socket); about 0.3 KB static
  (R9.5).
- The memory plan: the run's minimum against 64 KB, plus a projection for the realistic home from the
  measured network cost; below it, stop and report, the knobs proposed in order.
- What QEMU cannot run (Wi-Fi, the server) is covered by the Mac tests of every piece of logic
  (spec 022's core, `ui-embed` and script tests, and this spec's script tests), by building and
  linting the radio image in `validate.sh`, and by the run on the box.
- D33 was written by spec 022; this spec checks it against what is built.

## How the user will check it

The demo, on the Dongle Max on USB, with the SNZB-03PR2 and the ZBMINIR2 paired (or paired again in
the window of a power-on boot), the Mac host not running:

1. Build the interface and the radio image; flash it with the user's yes and MAC:
   `sh scripts/box.sh flash --image radio --keep-storage --backup <file>`.
2. `sh scripts/box.sh wifi`: type the network's name and password; the script prints the address.
3. In a second terminal, `sh scripts/box.sh log --image radio --seconds 900` (the box restarts; its
   `wifi:`, `clock:`, `serving:`, `memory-network:` and `heap:` lines show).
4. Open `http://<address>/` in the Mac's browser; `sh scripts/demo-garage.sh <address>`; move before
   the sensor: the light switches and the interface shows it.
5. `sh scripts/measure-api.sh <address>`.
6. Switch the access point off for a minute, then on: the log shows `wifi: lost`, the retries, then
   `wifi: connected`; the interface reconnects.

Result of the run (filled after the run on the box):

- [ ] `box.sh wifi`, the address printed, the password never shown: ...
- [ ] the interface served by the box, live: ...
- [ ] the garage set up through the API, the motion switching the light: ...
- [ ] the 33 requests: ... answered 201, ... Busy, total ... s
- [ ] the internal RAM: `memory-network:` ..., `heap:` minimum ..., projection for the realistic
      home ...
- [ ] the lost link and its return: ...
