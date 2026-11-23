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
box with the user's yes (D6) and runs the demo and the measures on it. It also adds what the
firmware needs from the core and the scripts and spec 022 did not give, each tested on the Mac:
in the core, `api::static_reply` (R4.6), the saver's gate (R3.2) and `WifiSettings::to_json`
(R6.2); in the scripts, the radio image's `network:` line in `scripts/lib/firmware.sh` (R1.2) and
the 15 s answer window of `box.sh wifi` (R6.2).

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
  `measure-api.sh`, D33's text. This spec uses them unchanged, but for the additions named in the
  context (three to the core, two to the scripts) and one sentence added to D33 (the development
  origins, decided by the user).
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
   - choice `wifi` (the setting `wifi`, or `auto`): this image detects no cable, so it passes
     `cable` = `false` to `boot_lines_radio` and `auto` always means Wi-Fi (a plugged cable is
     ignored until E3e). It reads `wifi.json`, parses it with the core's `wifi_settings` (spec 022)
     and starts the Wi-Fi station when the settings are valid;
   - choice `ethernet` (the setting `ethernet`): `network: ethernet (setting ethernet), not in this
     image, no link started`; no Wi-Fi either (D24: one link, the one chosen).
2. The `network:` line of the radio image becomes `network: wifi (setting <s>)` (the core's
   `boot_lines_radio`: no `no link started`, Wi-Fi settings present or not); the box, sim and
   measuring images keep `network: wifi (setting auto), no link started` byte for byte. Today
   `fw_missing_line` (`scripts/lib/firmware.sh` l.111) requires `network: .* no link started` for
   every image, the radio image included; it keeps that rule for the box and sim images and, for
   the radio image, accepts `network: wifi (setting <s>)` or `network: ethernet (setting <s>), not
   in this image, no link started` (the pattern `network: (wifi|ethernet) \(setting `).
   `fw_lines_ok` (l.100-104, used by `scripts/qemu.sh` only) and `scripts/qemu.sh` are unchanged.
3. Settings missing: `wifi: no settings (/data/wifi.json missing), no link started: sh scripts/box.sh
   wifi`. Settings refused: `wifi: settings refused (<reason>), no link started`. A read error of the
   file: `wifi: <file>: cannot read: <reason>, no link started`. In all three cases the engine runs
   as in spec 021, with no server.
4. Wi-Fi station, WPA2-Personal at least (WPA3 accepted by the access point's choice; WEP and open
   networks refused unless the password is empty, which asks for an open network and logs
   `wifi: open network, no encryption`). The DHCP host name is `solite-<6 hex digits>`, the last
   three bytes of the station's MAC address in lower case: the box's name. `EspWifi` is built with
   no NVS partition (`None`, so ESP-IDF's `wifi_init_config_t.nvs_enable` is 0, `esp-idf-svc`
   0.53 `wifi.rs` l.546-566) and the firmware also calls `esp_wifi_set_storage(WIFI_STORAGE_RAM)`
   before setting the configuration: the SSID and the password are held in RAM only and never
   written to NVS (D33: "never NVS"); the file `/data/wifi.json` is their only copy.
5. Power save is off (`WIFI_PS_NONE`): the box is powered by USB or mains, and the modem sleep of
   the default mode delays every packet by up to a beacon interval (about 100 ms, more with a large
   DTIM), which would slow each of the 33 requests and the live batches. No software coexistence
   exists between the ESP32's Wi-Fi and the EFR32's Zigbee (two chips); the `wifi: connected` line
   shows the Wi-Fi channel and the `network:` line of the radio the Zigbee channel, so an overlap is
   seen in the log (Zigbee 15 lies between Wi-Fi channels 1 and 6, 20 between 6 and 11, 25 above 11).
6. ESP-IDF's own `wifi` log tag is set to warnings only, so its info lines (which name the SSID) do
   not reach the serial log. On the Rust side, `esp-idf-svc` 0.53 logs
   `debug!("Setting STA configuration: {conf:?}")` (`wifi.rs` l.1257), which prints the password:
   the `esp_idf_svc::wifi` log target stays at info or above (the firmware's log level is ESP-IDF's
   default, info; no `set_target_level` lowers it), and a comment beside the logger's start says
   why.
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
   **The hold is in the core**, because `Saver::tick` runs inside the core's `engine::run`
   (`crates/core/src/engine/mod.rs` l.151) and a `Storage` that failed on purpose would push the
   zones, equipments and recipes back by `RETRY_MS` (60 s) and count the devices' write as failed
   (`crates/core/src/engine/saver.rs` l.57-104). The `Saver` gains a gate,
   `Saver::with_gate(gate: impl Fn() -> bool + 'static)`: while the gate answers `false` and the
   engine is not stopping, `tick` writes nothing and changes nothing (every file stays due, no
   failure is shown or counted, no `RETRY_MS` delay, the devices' policy untouched); at the first
   tick after it answers `true`, the files due are written as usual. A stop writes whatever the gate
   answers (the forced save of R6.3; its dates may then be 1970). A `Saver` without a gate behaves
   as today. The firmware's gate reads an `AtomicBool` set by the SNTP callback or by the 20 s
   timer, whichever comes first (set at once without Wi-Fi settings). Tested on the Mac (plan).
3. A later sync logs `clock: synced, <ISO 8601 UTC>` once. The dates taken before it are from
   1970 (`lastSeen`, recipe log, data timestamps, in memory during the wait, on the flash when the
   clock never synced); the core's rules apply unchanged after the jump (spec 022 R6.2 pins that the
   engine goes on; a device silent by the new clock, one paired during the wait included, turns
   offline until it speaks, as any silence).
4. Random numbers: once the Wi-Fi station has started, the ESP32's generator is fed by its radio and
   the entropy switch of spec 021 (`bootloader_random_enable`, which must not run while Wi-Fi runs)
   is skipped; with no Wi-Fi it is used as in spec 021.

### R4: The server

1. **Threads and cores.** The engine thread stays on core 1 (spec 021). ESP-IDF's Wi-Fi task (on
   core 0 by ESP-IDF's default, `CONFIG_ESP_WIFI_TASK_PINNED_TO_CORE_0`), lwIP's tcpip task (no
   core by default: `CONFIG_LWIP_TCPIP_TASK_AFFINITY_CPU0=y` is added to
   `sdkconfig.radio.defaults`), the HTTP server task (`esp-idf-svc`'s `Configuration.core`), the
   API's workers, the live thread and the console thread are pinned to core 0.
   The engine thread loads the home, then hands the shared home and its live hub to `main`, which
   starts the server; the server never starts without a home (a home that cannot be read stops the
   engine as in spec 021, and no server starts: `serving: none (<reason>)`).
2. **Port and limits.** Port 80, IPv4. At most 12 open connections (`max_open_sockets` 12, with
   `lru_purge_enable` true, `esp-idf-svc` 0.53's default kept and written out in the configuration:
   when 12 are open, ESP-IDF closes the least recently used one before it accepts the new one,
   `httpd_main.c` l.52-75), in an lwIP socket table of 16 (`CONFIG_LWIP_MAX_SOCKETS`, 10 by default
   in ESP-IDF 5.5). The size is ESP-IDF's own rule: its server uses `max_open_sockets` + 3 sockets
   (the listener and its two UDP control sockets) and `httpd_start` refuses a smaller table
   (`httpd_main.c` l.495-507), so 15, plus one spare; DHCP, DNS and SNTP use lwIP's raw API, no
   socket. The check is ESP-IDF's refusal at the server's start, which the box would log as
   `serving: none (<reason>)`. Request head at most 1024 bytes
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
6. **The interface.** `/` and `/assets/*` are registered for GET, POST, PUT and DELETE, so that
   the core decides every answer. They are answered by a new core function,
   `solite_core::api::static_reply(request: &Request) -> Option<Reply>`: `None` when the path is
   not the interface's; else, in the order of the Mac's `dispatch` (`crates/core/src/api/mod.rs`
   l.231-244), the `Host` check (`host_allowed`, l.152; 403 `{"error":"Forbidden host"}`), then
   `static_files::route` (`static_files.rs` l.67, which stays `pub(super)`: 405 for a method other
   than GET, 404 for an unsafe name, else the file to send). `dispatch` itself is unchanged; the
   firmware cannot call `route`, and calling it alone would skip the `Host` check and serve `/`
   to any `Host`. A file to send is looked up in the table embedded in the firmware (R7) with the
   core's `find` (`static_files.rs` l.42): its gzip bytes are sent from flash with
   `Content-Encoding: gzip`, `Vary: Accept-Encoding`, its content type, the core's `cache_control`
   and `STATIC_HEADERS` (spec 022). A name not in the table: 404 `{"error":"Not found"}` with the
   security headers. These are served by the server's task (short, from flash), not by the
   workers.
7. **The pairing route.** On the box, `GET` and `POST /api/v1/integrations/solite/permit-join`
   answer 404 `{"error":"Pairing from the API is off on the box"}` (D28), decided in the core under
   `Reach::Lan` (spec 022 R4.3). The boot window of spec 021 is unchanged.
8. **Security headers on API answers**: unchanged (none, as on the Mac).

### R5: Live events on the box

1. `GET /ws` is registered directly with ESP-IDF (`httpd_register_uri_handler`, `is_websocket`),
   with the subprotocol `bearer.none` (the one the interface sends, `ui/src/store/useLive.ts`) and a
   post-handshake callback (`CONFIG_HTTPD_WS_POST_HANDSHAKE_CB_SUPPORT=y`, decision D37). ESP-IDF
   answers the handshake itself, then calls the callback on the handshake's request, where the
   firmware checks, in this order: the `Host` (spec 022 R4.2), else closes with 4003 `Forbidden
   host`; the `Origin` (the core's `live::session::origin_allowed`, its development origins
   included, decided by the user), else 4003 `Origin not allowed`; a slot in the hub
   (`LiveHub::connect`), else 1013 `Try again later` (the 5th client). Then it sets the request's
   session context, which ESP-IDF keeps with the socket and releases when it deletes it (the slot is
   freed there), and sends the welcome message of spec 013. `Host` and `Origin` are read from the
   raw `httpd_req_t` with ESP-IDF's `httpd_req_get_hdr_value_len` / `httpd_req_get_hdr_value_str`,
   in one small `unsafe` function with its `SAFETY` comment (D11), each header bounded by the
   1024-byte head of R4.2. (`esp-idf-svc`'s `ws_handler` is not used: with ESP-IDF 5.5 the
   handshake never reaches a WebSocket URI handler, so its session was never made, D37.)
2. The live thread (core 0) every 200 ms (`TICK_MS`) sends each client its batch as one text
   message in fragments (`[`, each event, `]`) taken with `begin_batch` and `pop`, no batch string
   built: each frame is handed to the server's task (`httpd_queue_work`, then
   `httpd_ws_send_frame_async` there) and the thread waits for its result; a frame for a session
   already closed is not sent (its socket number may belong to a new connection). A close asked by
   the hub (1013 on a full queue) is sent; a ping every 30 s. The live thread closes a client (a
   failed send, a close asked by the hub) through the server's task too: there, a session still
   open has its socket shut down, and ESP-IDF closes it (from the code review, 2026-11-22).
3. A client is dropped when a frame cannot be written within 5 s (the server's send timeout) or when
   TCP keep-alive finds it gone (R4.2, about 45 s). The 60 s silence rule of the host is not used on
   the box: ESP-IDF answers the pings itself; the pongs it hands over are ignored.
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
2. `WifiSet`: the settings are written to `/data/wifi.json` through the storage's save (temporary
   file, sync, rename), then `wifi: settings saved`. The text written is the core's
   `WifiSettings::to_json()` (new, `crates/core/src/network.rs`): the object
   `{"ssid":<string>,"password":<string>}` serialized by `serde_json`, which `wifi_settings` reads
   back to the same settings (a round-trip test, plan); the received line is not stored as is
   (`ConsoleCommand::WifiSet` holds the parsed settings, not the text). `Refused(<reason>)`:
   `wifi: settings refused (<reason>)`, nothing changed. `WifiForget`: removes the file,
   `wifi: settings removed`.
   The FAT partition is shared with the engine, whose save of a file may be running: the console's
   write then waits for it, at most one save (the longest measured on the box: 3945 ms,
   `docs/memory-budget.md` l.151), so `wifi: settings saved` may come about 4 s after the line.
   `box.sh wifi` waited 5 s for that answer (`scripts/box.sh` l.628); the window becomes 15 s
   (`SOLITE_WIFI_ANSWER_SECONDS`, 15 by default, for the tests), and the living spec's "no answer
   within 5 s" becomes 15 s.
3. After a save or a removal, the engine is asked to stop (its stop flag), saves its files (forced
   save, as the host's Ctrl-C does), and the ESP32 restarts by software (no pairing window: not a
   power-on, spec 021; the `pair` line of R6.5 opens one); if the engine has not stopped within 10
   s, it restarts anyway. After a removal it then starts no link (R1.3).
4. With this console, `sh scripts/box.sh wifi` (spec 022 R9) ends as it describes: the settings
   saved, the box restarted, its address printed; `wifi --forget` removes them.
5. `Pair` (added after the run on the box, 2026-11-22, decision D36): the line `pair` alone
   (`ConsoleCommand::Pair`) logs `pairing: asked from the console` and sets a flag that a small
   thread, `pair-watch` (4 KB of stack, every 500 ms), turns into the home's pairing request for
   `PAIR_AT_BOOT_SECONDS` (180 s), whatever the boot and the marker; the engine then logs
   `pairing: open for 180 s` or `pairing: failed: <reason>`. `sh scripts/box.sh pair` sends it and
   reads that answer within 15 s (exit 0 open, 1 failed, 4 no answer). Nothing is written to the
   flash. The pairing route stays 404 on the network (R4.7).

### R7: The interface in the firmware

1. `firmware/build.rs` calls spec 022's `crates/ui-embed` (a build dependency) only with the feature
   `radio`, from `../ui/dist` (or `SOLITE_UI_DIST`), writing `ui_files.rs` into `OUT_DIR`; the other
   images embed nothing and build without `ui/dist`. `cargo:rerun-if-changed` covers `ui/dist`, and
   `cargo:rerun-if-env-changed=SOLITE_UI_DIST` a change of the folder. A
   missing `ui/dist` fails the build with `ui-embed`'s message (`interface: ui/dist not built: cd ui
   && npm ci && npm run build`).
2. The radio image logs the table's marker `solite-ui <n> files, <gzip KB> KB` at boot.
3. `box.sh flash --image radio` refuses a radio build without the embedded interface (its ELF lacks
   the marker `solite-ui `), with the build command, nothing written.
4. `validate.sh --only firmware` always builds `ui/dist` first (`npm run build`, even when a
   `ui/dist` exists: an old one would be embedded and still pass the checks; `ui/node_modules`
   needed, `cd ui && npm ci` named otherwise), checks the radio ELF holds `solite-ui ` and the name
   of `ui/dist`'s hashed script (the embed is the current build), and that `CONFIG_HTTPD_WS_SUPPORT=y`
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

1. `log --image radio` checks the lines of spec 021, with the radio image's `network:` line of
   R1.2 in place of `no link started`, plus: a `wifi:` line (none is required when the network
   line is `network: ethernet ...`, which starts no link, R1.1); when a `wifi: connected` line shows, a
   `serving: http://` line after it; and prints the last `wifi:`, `clock:`, `serving:` and
   `memory-network:` lines. It fails on `wifi: settings refused` (named). A box with no settings
   passes (its `wifi: no settings` line is printed). The box and sim images' checks are unchanged
   (`no link started` still required).
2. `backup` warns, as today, that a backup may hold Wi-Fi credentials: once `wifi.json` exists it
   does (in plain text, as `zigbee.json`'s keys).

### R9: What is measured on the box

1. At boot the `memory:` line is unchanged (before Wi-Fi). Ten seconds after the first `serving:
   http://` line the image logs once `memory-network: internal <n> KB free (largest block <m> KB),
   psram <p> KB free of 2048 KB`: a snapshot with the server idle, printed for information. It is
   not the network's cost: between the boot `memory:` line and it, the engine's stack, the UART
   rings, the loaded home and the server's stacks are allocated too (the engine's 32 KB stack and
   the UART rings are not Wi-Fi's), and an idle snapshot misses the peaks.
   **The network's cost** is a difference of two minima taken the same way: spec 021's `heap:`
   internal minimum with one device and no Wi-Fi, **234 KB** (`docs/memory-budget.md` l.155, the
   radio image after 4 minutes), minus this run's internal minimum on its first `heap:` line (240 s
   after the radio starts: the home of the devices paired, no equipment yet, Wi-Fi and the server
   up). The engine's stack, the UART rings and the loaded home are in both and cancel; what is left
   is Wi-Fi, lwIP, the server's internal stacks, the live thread and the console. This run's home
   holds two devices (the SNZB-03PR2 and the ZBMINIR2) where spec 021's held one: the second device
   is counted in the cost, pessimistic by about 1 to 2 KB (**estimate**).
2. The `heap:` line every 240 s (spec 021) gains `, http stack <s> KB unused of <t> KB` (the core's
   `heap_line`, spec 022 R6.1, fed with the smallest high-water mark among the 4 workers,
   `WORKER_STACK_BYTES` = 10 KB each, in PSRAM: R9.4).
3. Pass criteria, written with the figures in `docs/memory-budget.md` ("On the box"):
   - the `heap:` internal minimum stays at or above 64 KB (the reserve, D23) through the whole run:
     boot, Wi-Fi up, the interface open with one live client for 2 minutes, the garage set up, the 33
     requests, the lost link (`box.sh log --image radio` fails below it);
   - the projection for the realistic home, written beside, marked E: the measuring image's
     internal minimum on the box (107 KB, spec 020, `docs/memory-budget.md` l.149) minus the
     network's cost of R9.1;
   - **when the run's minimum or the projection is below 64 KB**, the next knob is applied, approved
     in advance by the user (2026-11-22): `CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL` from 4096 to 2048 in
     `firmware/sdkconfig.radio.defaults` only (the other images keep `sdkconfig.defaults`' 4096 and
     their measured figures); the radio image is rebuilt, flashed again with the user's yes (D6),
     and the run's minimum and the projection are measured again the same way. The 234 KB of R9.1
     was read at 4096, so the setting's saving on the small home lowers the measured cost; the
     realistic home's larger saving is not credited, so the projection stays pessimistic. **Only if
     a figure is still below 64 KB, the work stops and the figures are reported to the user**: the
     limit is not lowered, no further setting is turned on the quiet. The knobs left, in the order
     they would be proposed, as a decision of the user: `CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL` 512
     (spec 016 measured its effect), then streaming the home's loading (roadmap inbox). The
     workers' stacks in PSRAM are on from the start (R9.4);
   - the 33 requests: R10.
4. **Expected outcome, and the workers' stacks in PSRAM from the start.** The budget's figures give
   the projection before any run. The network's cost by the budget: 70 KB for Wi-Fi
   (`docs/memory-budget.md` l.57) plus the stacks that stay in internal RAM, 16 KB: about 86 KB, so
   107 − 86 = about **21 KB, under the 64 KB floor**. Without the workers' stacks in PSRAM it would
   be 40 KB lower still; they are therefore in PSRAM from the start, not left to the run:
   `CONFIG_SPIRAM_ALLOW_STACK_EXTERNAL_MEMORY=y` in `sdkconfig.radio.defaults`, and the 4 workers'
   stacks (4 × 10 KB) allocated in PSRAM (`esp_pthread_cfg_t.stack_alloc_caps` =
   `MALLOC_CAP_SPIRAM`); the workers never write the flash, which a task with its stack in PSRAM must
   not do. Their 16 KB body buffers are in PSRAM too (R4.4). What stays in internal RAM: the
   server's task (8 KB), the console (4 KB, it writes the flash), the live thread (4 KB), lwIP's
   sockets (R9.5) and Wi-Fi's own internal part. The projection reaches 64 KB only if the measured
   cost is about 43 KB or less (Wi-Fi's own part about 27 KB), which
   `CONFIG_SPIRAM_TRY_ALLOCATE_WIFI_LWIP` (Wi-Fi's and lwIP's dynamic buffers in PSRAM) makes
   possible but not certain. The likely first run: the run's own minimum passes with room (about
   234 − 86 = 150 KB), the projection falls under 64 KB, the knob of R9.3 is applied and measured
   again, and the work stops with the figures only if the projection is still under.
5. **The lwIP socket table.** `CONFIG_LWIP_MAX_SOCKETS` is 16 rather than ESP-IDF's 10 (R4.2). Its
   cost, an **estimate** from lwIP's structures on ESP-IDF 5: about 40 bytes of table per slot
   whether used or not (6 more slots: about 0.25 KB, static), and about 0.4 KB per socket while it
   is open (its `netconn`, its TCP control block and its mailboxes, from lwIP's heap in internal
   RAM). Counted inside the network's cost of R9.1, not apart.
6. **The realistic home stays a projection.** The "Internal RAM with Wi-Fi up" row of
   `docs/memory-budget.md` is measured for the home the run holds (two devices); for the realistic
   home it stays an **estimate**, a subtraction (R9.3), marked E. The realistic home is measured on
   the radio image with Wi-Fi up later, by a roadmap inbox line (roadmap notes): the radio image
   seeded with the realistic home of `firmware/fixtures/` (as the measuring image is), Wi-Fi and
   the server up, its `heap:` minimum read.

### R10: The 33 requests on the box

1. `sh scripts/measure-api.sh <box address>` (spec 022 R10.2-3) is run against the box, with the
   SNZB-03PR2 and the ZBMINIR2 paired; pass: 33 answered 201, none 503, total at most 10 s.
2. The time is written in `docs/memory-budget.md` whatever the verdict; a failure stops the work and
   reports the figures to the user.

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
  1013 for a 5th client), the development origins (`http://localhost:5173`,
  `http://127.0.0.1:5173`) accepted as on the Mac, only `bearer.none` is echoed, a dead client is found by TCP keep-alive
  instead of 60 s of silence; 411 for a body without length; 431 for a head over 1024 bytes; at most
  12 open connections, the least recently used closed beyond; 503 `Busy` beyond 4 requests at once.
  The interface is served from the firmware, gzip always (`Content-Encoding: gzip`), cache headers as
  on the Mac.
- Changed, "Shows the home in the browser": the interface also opens at `http://<box address>/` or
  `http://solite-xxxxxx/` (when the router resolves DHCP names), from the copy embedded in the radio
  image at build time.
- Changed, "Flashes the box and brings it back": `wifi` ends with the box saving the settings,
  restarting and its address printed, `wifi --forget` removes them (spec 022's sentence "an image
  without the console answers nothing" kept for the original firmware and the other images); "no
  answer within 5 s" becomes "no answer within 15 s" (R6.2); the radio build refused without the
  embedded interface; `log --image radio`'s new lines and checks (its `network:` line without
  `no link started`).
- Changed, "Stays within a memory budget": the radio image logs `memory-network:` (a snapshot, for
  information) and the workers' stack; `docs/memory-budget.md` holds the internal RAM with Wi-Fi
  up, the network's cost as a difference of two `heap:` minima (R9.1), the projection for the
  realistic home and the 33 requests' time on the box.
- Changed, the closing paragraph (l.582-583): "the firmware's radio image does the same over the serial link,
  with no network and no interface" → "the firmware's radio image does the same over the serial link
  and serves the API, the live events and the interface over Wi-Fi, without the pairing route".
- History: `| 023 | The radio image joins Wi-Fi and serves Sowel's API, its live events and the
  interface from its flash |`.

### Other docs (applied in the same change)

- `docs/architecture.md`: the diagram (l.24), the Firmware row (Wi-Fi, SNTP, the server, the live
  thread, the console, the embedded interface), the Live events row (the box's sessions in
  `firmware/src/live_ws.rs`), the API row (`static_reply`), Builds (`build.rs` embeds `ui/dist` for
  `radio` through `crates/ui-embed`, the new settings of `sdkconfig.radio.defaults`, the UI built
  before the radio image), the Interface line, Data (`/data/wifi.json`).
- `docs/memory-budget.md`: "On the box" rows "Internal RAM with Wi-Fi up" and "The 33 requests" filled;
  the internal RAM table gains the measured network cost (R9.1's difference) beside its estimate;
  if the knob of R9.3 was applied, the radio image's `SPIRAM_MALLOC_ALWAYSINTERNAL` and the figures
  before and after it; the transport row
  (no tungstenite buffers on the box); the PSRAM rows of the workers' stacks and body buffers (added
  as estimates with E3c's spec) checked against the `heap:` line's PSRAM figures.
- `docs/hardware.md`: the Wi-Fi antenna risk row: settled by the run (RSSI and channel seen; the box's
  position not recorded).
- `docs/decisions.md` D33: two sentences changed in this change. Added, decided by the user
  (2026-11-22): "On `/ws` the box accepts, as the host does, the development origins of
  `live::session::origin_allowed` (`http://localhost:5173`, `http://127.0.0.1:5173`): a
  development page on a machine of the home's network can open the live events." Changed, found by
  this spec's review and shown to the user with the spec: "a socket table of 24" becomes "a socket
  table of 16" (R4.2). If the knob of R9.3 is applied, D33 records it. D33 is otherwise checked
  against what is built; any other difference is raised to the user, not rewritten on the quiet.
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
  (l.62) already lists `wifi` and is unchanged. `CLAUDE.md` is unchanged: its `identify|log` line
  is the read-only one, and `wifi` is already among the commands the user runs.
- `.github/workflows/ci.yml`: the firmware job installs Node and `ui/` packages (the radio image
  embeds the built interface).

### Added after the run on the box (2026-11-22)

- `docs/living-spec/README.md`: "Starts and shows its name" (the console's `pair` line also opens
  pairing for 180 s); "Pairs a device" (the window opens for 180 s on the console's `pair`, whatever
  the boot and the marker; opening the serial port resets the box, so within 10 minutes of the
  previous boot the marker refuses that boot's window and `pair` opens it; pairing never opens from
  the network on the box, D36); "Serves Sowel's API from the box" (pairing opens at a power-on boot
  or from the console's `pair`, over the cable only); the zone aggregation (`motionSince` is none
  while the clock is not set, before 2020-01-01); the `box.sh` list (the `pair` bullet).
- `docs/decisions.md`: D36 (the pairing window from the serial console, never from the network) and
  D37 (`/ws` registered directly with ESP-IDF).
- `docs/architecture.md`: the Live events row (`/ws` registered directly, the post-handshake
  callback, `httpd_queue_work`), the Firmware row (`pair`, `pair-watch`), the radio settings line.
- `CLAUDE.md` and `README.md`: `sh scripts/box.sh pair`. `docs/hardware.md`: `box.sh pair` also
  resets the box. `docs/memory-budget.md`: the 33 requests measured.
- `docs/roadmap.md`: the inbox lines on the power-on needed to pair removed, the J4 pairing line
  rewritten, the 33 requests and a `lastSeen` of 1970 added, the pending checks narrowed; journal.

### No longer true

Found by searching `no link started`, `starts no link`, `no network`, `no HTTP server`, `does not
serve`, `with no network and no interface`, `127.0.0.1 only`, `box will`, `the box's HTTP server`,
`needs Wi-Fi (E3c)`, `need Wi-Fi (E3c)`, `Not needed before Wi-Fi is built`, `answers nothing`,
`within 5 s`, `UART interrupt in internal RAM`, `which the host serves`, `no link is started`
(line numbers read on `main` at c9373df):

- `docs/living-spec/README.md` l.39-40: "logs its choice (`network: wifi (setting auto), no link
  started`), starts no link" → true for the box, sim and measuring images; the radio image logs
  `network: wifi (setting auto)` and starts the link.
- `docs/living-spec/README.md` l.46-48: "(`firmware/sdkconfig.radio.defaults`, which puts the UART
  interrupt in internal RAM)" → which also turns on the WebSocket, the server's limits, the socket
  table, the PSRAM stacks and buffers, lwIP's core and SNTP's settings.
- `docs/living-spec/README.md` l.55: "The radio image has no network and no HTTP server; it is a
  development image." → rewritten (above).
- `docs/living-spec/README.md` l.82: "the firmware starts no link." → "the radio image starts the
  Wi-Fi link; the other images start none."
- `docs/living-spec/README.md` l.250, "Serves Sowel's device API": "serves on `127.0.0.1` only" →
  true of the host; the box is in its own section.
- `docs/living-spec/README.md` l.525: "no answer within 5 s" → 15 s (R6.2). The sentence after it
  (l.526-527, "an image without the console, such as the box's original firmware, answers
  nothing") names the original firmware and stays true.
- `docs/living-spec/README.md` l.582-583: "the firmware's radio image does the same over the serial
  link, with no network and no interface" → rewritten (above).
- `docs/architecture.md` l.24 (diagram): "served from ui/dist by the host's --serve" → also from
  the radio image's flash.
- `docs/architecture.md` l.31: "which the host's `--serve` serves; the box does not serve it." →
  "which the host's `--serve` serves from disk and the radio image embeds, compressed, in its flash."
- `docs/architecture.md` l.45, Firmware row: "the network setting read from `/data/network.json`"
  → adds the Wi-Fi link, SNTP, the server and the console of the radio image.
- `docs/architecture.md` l.64-65: "`firmware/sdkconfig.radio.defaults` read after
  `sdkconfig.defaults` (the UART interrupt in internal RAM)" → adds the network's settings.
- `docs/architecture.md` l.79: "Vite builds `ui/dist/`, which the host serves" → "which the host
  serves and the radio image embeds".
- `docs/memory-budget.md` l.73: "the network rows need Wi-Fi (E3c)" and l.152-153, the two
  "needs Wi-Fi (E3c)" cells → the figures of the run.
- `docs/memory-budget.md` l.108-110, QEMU section: "the margin that decides is the box's, with the
  network up (needs Wi-Fi, E3c)" → the measured figure.
- `docs/hardware.md` l.98, risks: "The Wi-Fi antenna | Not described by Sonoff's pages found. | Not
  needed before Wi-Fi is built." → settled by the run.
- `docs/decisions.md` D18 (l.273): "The box will use its own server" stays as the record of
  2026-11-21 (decisions are dated history, not rewritten); D33 says what the box uses.
- `crates/core/src/network.rs` l.114-116, doc comment of `boot_lines`: "(the firmware starts no
  link and detects no cable ...)" → for the images without a link.
- From decision D35 (the backup optional for `flash`, no MAC question for `flash`):
  `docs/living-spec/README.md`, "Flashes the box and brings it back" (the `flash` bullet "refuses
  without a backup", the prompt bullet "`backup`, `restore` and `flash` ... ask", the `--keep-storage`
  bullet's `--backup <file>`, and the `wifi` bullet's "without resetting the box": opening the port
  resets it); `docs/hardware.md`, Procedures (the opening paragraph, step 3 "before any other
  write", step 5 "once the backup's row has its restore date" and "refuses without the backup", step
  6 "restore the backup"); `README.md` (the flashing paragraph); `scripts/box.sh` (the header
  comment and the comment of `console_exchange`: "DTR and RTS stay low, so that opening the port
  does not reset the box"); D31's "refuses to flash without a backup that was restored once" stays
  as dated history, D35 replaces it. `CLAUDE.md` (its `box.sh` line: the user runs `flash`) and
  `.claude/` (no mention of the backup found) stay true.
- `firmware/src/main.rs` l.1-2, module comment: "reads its network setting" → adds the Wi-Fi and
  the server of the radio image; l.41, doc comment of `log_network`: "no link is started" → for
  the images without a link (the radio image's path starts Wi-Fi).
- After the run on the box (2026-11-22), searched with `ws_handler`, `detached`, `EspHttpWsConnection`,
  `power-on`, `pairing window` and `box.sh wifi`: this spec's R5.1 and R5.2 (`esp-idf-svc`'s
  `ws_handler`, `EspHttpWsConnection::New`, the detached sender) and `architecture.md` (the diagram,
  the Live sessions row, the `CONFIG_HTTPD_WS_SUPPORT` row, the `live_ws.rs` row, the risk on the
  `New` variant), rewritten for D37; `firmware/sdkconfig.radio.defaults` (its comment on
  `esp-idf-svc`'s handler); `docs/architecture.md` l.42 ("on ESP-IDF's WebSocket handler"); the
  living spec's "the pairing window of a power-on boot is unchanged" (the box's pairing route); the
  roadmap inbox lines "pairing a device still needs a power-on" and "pairing the ZBMINIR2 needs a
  power-on after 10 min", removed, and "Replace the pairing window opened at boot (... no input
  path)", rewritten; this spec's last sentence of the first run's result. D28, D32 and D33 stay as
  dated history; D36 and D37 say what changed.

## Acceptance criteria

- [ ] `sh scripts/box.sh wifi` on the box: the password typed is never shown; the script prints the
      box's address; `/data/wifi.json` exists on the box (the next `log` shows `wifi: connected`).
      Partly seen (2026-11-22): `wifi: connected, address 192.168.0.46, ...` and `serving:` in the
      log after `box.sh wifi`; the script's own output was not recorded.
- [ ] The SSID is in no serial log of the run: the user runs `grep -c '<SSID>' <log file>` on the
      Mac on each log file `box.sh log` printed (the SSID is never written in the repository or
      in a report), and it prints 0. NOT RUN.
- [ ] `http://<box address>/` opens the interface from the Mac's browser, with the rooms and the
      live state (one live client), served by the box (the Mac host not running). Seen in the
      second run (2026-11-22): the interface live in Chromium, the welcome and the events, 7
      reloads out of 7.
- [x] `sh scripts/demo-garage.sh <box address>` sets the garage up on the box; the motion before the
      sensor switches the ZBMINIR2, and the interface shows it live. Seen in the second run
      (2026-11-22).
- [ ] `sh scripts/measure-api.sh <box address>` prints 33 answered 201, 0 Busy, and its total time;
      the figure is in `docs/memory-budget.md`. Second run (2026-11-22): never `Busy`, but over
      10 s in seven runs of eight and one request with no answer in two; the figures are in
      `docs/memory-budget.md`. FAILED (roadmap inbox).
- [ ] The `memory-network:` line, the `heap:` minimum through the run, the network's cost (234 KB
      minus the first `heap:` minimum, R9.1) and the projection for the realistic home are in
      `docs/memory-budget.md`; the minimum and the projection are at or above 64 KB, with or
      without the knob of R9.3 (said which), or the run stopped after the knob and the figures were
      reported. Partly: the `memory-network:` line is in `docs/memory-budget.md`; the `heap:`
      minimum through a run, R9.1's difference and the projection are NOT computed.
- [ ] With the access point switched off: the log shows `wifi: lost (...)`, then retries; a motion
      before the sensor while it is off still switches the ZBMINIR2 (seen on the relay and in the
      log); back on, `wifi: connected` again and the interface reconnects by itself. NOT RUN.
- [ ] No 1970 date written to the flash by a boot with the access point on: after that boot has
      saved (a `clock: synced` line, then a pairing or an edit), the box is unplugged and plugged
      back with the access point off; once the access point is on again and the box answers,
      `GET /api/v1/devices` before any device has spoken since that boot (no measure line of it in
      the log): every `lastSeen` is of the day, none in 1970. NOT RUN (the `clock:` line was seen
      in the second run; a device that spoke before it kept a 1970 `lastSeen`: roadmap inbox).
- [ ] After `sh scripts/box.sh wifi --forget` and the box's restart, the log shows `wifi: no
      settings ...`, no `wifi: connected` and no `serving:`: the settings were in the file only,
      none kept in NVS. NOT RUN.
- [x] `sh scripts/validate.sh` green: the core and script tests below; the four firmware images built
      (the radio one with the interface embedded); QEMU unchanged.

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
- The setting `ethernet`: no link in this image (R1.1). The setting `auto` with a cable or PoE
  plugged: Wi-Fi, as with no cable (this image detects no cable, R1.1).
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
- The internal RAM minimum or the projection below 64 KB: `box.sh log` fails on the minimum; the
  knob of R9.3 (`SPIRAM_MALLOC_ALWAYSINTERNAL` 2048, radio image only) is applied and measured
  again; still below, the run stops and the figures are reported (R9.3).
- `Host` of `evil.example` on `/`: 403 `Forbidden host`; `POST /`: 405 (R4.6, the core's
  `static_reply`).
- `wifi-set` while the engine is saving a file: `wifi: settings saved` up to about 4 s later, within
  `box.sh wifi`'s 15 s (R6.2).
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
3. **The development origins on `/ws`.** The box accepts, as the host does, the development origins
   of `live::session::origin_allowed` (`DEV_ORIGINS`: `http://localhost:5173`,
   `http://127.0.0.1:5173`) on `/ws` (R5.1): a development page on a machine of the home's network
   can open the live events, within the open API of item 2. D33's text gets that sentence in this
   change (other docs delta).
4. **The next memory knob, approved in advance.** When the run's internal minimum or the
   projection falls under the 64 KB floor, `CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL` goes from 4096 to
   2048 in `sdkconfig.radio.defaults` only, and both are measured again; only if a figure is still
   under, the work stops and the figures are reported (R9.3). The flash it needs still asks the
   user's yes (D6).
5. **The backup of the original firmware is optional for `box.sh flash`** (2026-11-22, decision
   D35): `flash` no longer requires `--backup` (a backup given is still checked: 16 MB, this box,
   a proven row in `docs/hardware.md`) and asks no MAC question any more; `backup` and `restore`
   keep theirs. The way back to the original firmware is Sonoff's own reflash. Claude still never
   flashes without the user's yes (`CLAUDE.md`).
6. **The spec closed with a partial box run** (2026-11-22): the user chose to move on to features;
   the box checks not run are listed in "Result of the run" and in the roadmap's inbox.

## Decided in autonomy

- ESP-IDF's asynchronous requests and a pool of 4 workers: one server task would be held 12 s by an
  order, stopping the interface and the live events.
- `cable` is `false` in this image: it has no cable detection, so `auto` always means Wi-Fi, and a
  box with a cable or PoE plugged still starts Wi-Fi (R1.1). Cable detection and Ethernet are E3e.
- Three small core additions, tested on the Mac, where the review found the firmware could not
  reach or hold the core's rules: `api::static_reply` (the `Host` check and the 405 before the
  interface's files, as `dispatch` does), the saver's gate (the clock wait without fake storage
  failures), `WifiSettings::to_json` (the text written to `wifi.json`). `dispatch`, the host and
  the saver without a gate behave as before.
- `box.sh wifi`'s answer window 15 s instead of 5 s: the console's write can wait behind an engine
  save (3.9 s measured).
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
- `CONFIG_LWIP_MAX_SOCKETS` 16: ESP-IDF's server needs the 12 connections plus its 3 own sockets
  and refuses to start with fewer (ESP-IDF's default is 10); one spare; DHCP, DNS and SNTP take no
  socket; about 0.25 KB static (R4.2, R9.5). D33's "24" is changed with it.
- `CONFIG_LWIP_TCPIP_TASK_AFFINITY_CPU0`: lwIP's task on core 0 with Wi-Fi and the server, off the
  engine's core (R4.1).
- The memory plan: the run's minimum against 64 KB, plus a projection for the realistic home from
  the network's cost measured as a difference of two `heap:` minima (R9.1); below it, the knob
  approved by the user, then stop and report.
- What QEMU cannot run (Wi-Fi, the server) is covered by the Mac tests of every piece of logic
  (spec 022's core, `ui-embed` and script tests, and this spec's core and script tests), by building and
  linting the radio image in `validate.sh`, and by the run on the box.
- D33 was written by spec 022; this spec changes two of its sentences (the development origins,
  the socket table of 16) and checks the rest against what is built.
- From the code review (2026-11-22): the SNTP callback runs on lwIP's task, whose stack is small, so
  it only stores the sync's time and sets the flags; `wifi::run`'s loop logs `clock: synced, <ISO>`.
  The console thread's stack (4 KB, it writes the flash) has no measured margin: its high-water
  mark is logged on the `console: restarting` line and read on the box (plan, "Checked on the box
  only").
- A fragmented incoming `/ws` message is accepted as no concern: ESP-IDF's server never sets
  `fragmented` on a received frame, and a client's message is a small `subscribe` (1009 past 1 KB).
- A synchronous handler that answers without reading a declared body (`/`, 503 `Busy`, a failed
  asynchronous copy) returns `ESP_FAIL`, so ESP-IDF closes the connection rather than read and
  discard the body on the server's task.

Decided during the run on the box (2026-11-22):

- The console thread reads UART0 with a 100 ms wait in a loop (`READ_WAIT_MS` in
  `firmware/src/console.rs`): `uart_read_bytes(..., TickType_t::MAX)` never returned (the driver's
  deadline arithmetic overflows), so `box.sh wifi` got no answer.
- The time of the clock's sync is an `AtomicU32` of seconds (`CLOCK_SYNCED_SECS` in
  `firmware/src/wifi.rs`): the ESP32 has no `AtomicU64`, and `wifi.rs` did not compile for it.
- `box.sh wifi` waits for `console: ready` (12 s at most) before it sends its line: opening the
  serial port resets the box (the DTR and RTS lines of the USB-serial chip), and a line sent before
  the console exists is lost.
- Lesson: the build agent's "validate green" came before the last edit of `wifi.rs` and missed the
  `AtomicU64`; the radio image's build is run again after the last edit of the firmware.
- The test of Ctrl-C at `box.sh wifi`'s password prompt presses Ctrl-C again every 0.5 s until the
  script ends, and bounds its wait (20 s): on a loaded Mac the first Ctrl-C landed before `read`
  started, the shell held it, and the test hung a `validate.sh` run.

Decided after the run on the box (2026-11-22, in autonomy, the user away):
- D36: the serial console's `pair` line opens the pairing window for 180 s whatever the boot, never
  the network (the API stays 404); `box.sh pair` sends it.
- D37: `/ws` registered directly with ESP-IDF, with the post-handshake callback: `esp-idf-svc`'s
  `ws_handler` never saw the handshake with ESP-IDF 5.5.
- `motionSince` is none while the clock reads before 2020-01-01 (`CLOCK_SET_AFTER_MS`): the box
  counts from 1970 until SNTP answers, and the interface showed 20779 days of calm.
- From the code review (Opus, 2026-11-22): the live thread's close of a client goes through the
  server's task (a shutdown of the socket for a session still open), since ESP-IDF's
  `httpd_sess_trigger_close` acts one loop turn later on a slot a new connection may hold by then;
  `box.sh pair` names the engine (exit 1) when the console took the line but no window opened.
- The 33 requests' failure and the `lastSeen` of 1970 are recorded, not fixed here: both are
  roadmap inbox lines, outside the three changes the run asked for.

Decided while building (plan steps 1 to 4, 2026-11-22):

- `box.sh wifi` reads the console's answers through ESP-IDF's log prefix (`I (<ms>) <module>:`,
  colours included), which it drops before matching `wifi: ...`: the firmware answers with log
  lines, and spec 022's script matched only lines starting with `wifi:`. Tested with prefixed lines
  (the 15 s window test).
- `box.sh log --image radio` prints the link's choice (`network: wifi (setting <s>)` or the
  `ethernet` line) in place of the last `network:` line, which is the radio's own line, printed
  below it anyway.
- SNTP is given `pool.ntp.org` in both of its two places: lwIP writes the DHCP server's time server
  (option 42) over the first, so `pool.ntp.org` stays in the second.
- A request body that stops for 5 s (ESP-IDF's receive timeout) is answered 408
  `{"error":"Request timeout"}` and its connection closed, so a stalled client frees its worker.
- The API's handler is registered on `/*` for GET, POST, PUT, DELETE and PATCH; any other method is
  answered by ESP-IDF itself (405, its own text). `/` and `/assets/*` take GET, POST, PUT and
  DELETE, as R4.6 says.
- A `/ws` message sent in fragments is put together up to `MAX_INCOMING_BYTES` before it is a
  subscribe; past it, 1009.
- The welcome message of `/ws` is the host's text, repeated in `firmware/src/live_ws.rs` (the host is
  unchanged by this spec).
- `memory-network:` prints the PSRAM heap's own total (as the `memory:` line does), not a fixed
  2048.
- The server keeps at most 10 response headers (`max_resp_headers`, 8 by default): the interface's
  answers carry 6 besides the content type.
- The plan's count says 12 script scenarios; its table holds 11 script rows (one of them, the box
  and sim logs, is one test). 11 script tests and 8 core tests are written.
- D33 says "SNTP with `pool.ntp.org`"; R3.1 adds the DHCP server's time server. Not rewritten (D33 is
  checked, not rewritten on the quiet): raised to the user.

## How the user will check it

The demo, on the Dongle Max on USB, with the SNZB-03PR2 and the ZBMINIR2 paired (or paired again in
the window of a power-on boot), the Mac host not running:

1. Build the interface and the radio image; flash it with the user's yes and MAC:
   `sh scripts/box.sh flash --image radio --keep-storage --backup <file>`. The flash ends with
   `--after no_reset`: the box is not running yet.
2. Boot it: `sh scripts/box.sh log --image radio --seconds 60` (resets the box; `console: ready`
   and `wifi: no settings ...` show). Without this step `box.sh wifi` gets no answer (exit 4).
3. `sh scripts/box.sh wifi`: type the network's name and password; the script prints the address.
4. `sh scripts/box.sh log --image radio --seconds 1200` in a second terminal, kept for the whole run
   (it resets the box once; its `wifi:`, `clock:`, `serving:`, `memory-network:` and `heap:` lines
   show). The capture must end at least 240 s after the access point is back on (step 8), so that a
   `heap:` line follows the lost link; if the steps run late, a longer `--seconds` is chosen before
   starting, since a second capture resets the box and its minimum.
5. Open `http://<address>/` in the Mac's browser; `sh scripts/demo-garage.sh <address>`; move before
   the sensor: the light switches and the interface shows it.
6. `sh scripts/measure-api.sh <address>`.
7. The checks on the box of the plan ("Checked on the box only") that need no restart.
8. Switch the access point off for a minute; move before the sensor while it is off (the light
   switches); switch it on: the log shows `wifi: lost`, the retries, then `wifi: connected`; the
   interface reconnects.
9. After the capture: `grep -c '<SSID>' <log file>` on the Mac prints 0. Then the checks that need a
   restart (the power-ons, the 1970 check, `box.sh wifi --forget` then a boot with no connection),
   each with its own `box.sh log` capture.

Result of the run (2026-11-22, the radio image flashed three more times, debug builds, with
`--keep-storage`; the spec is closed as done, box run partial, by the user's choice to move on to
features):

- [ ] `box.sh wifi`, the address printed, the password never shown, the SSID in no log: the first
      tries got no answer (the console's read never returned, and opening the port resets the box:
      both fixed, "Decided during the run on the box"); then the box joined: `wifi: connected,
      address 192.168.0.46, name solite-b5a42c, channel 6, rssi -56 dBm`, then `serving:
      http://192.168.0.46/`. The SSID grep, a wrong password and `wifi --forget`: NOT RUN.
- [ ] the interface served by the box, live: `/` answered 200, `GET /api/v1/devices` 200, `POST
      /api/v1/integrations/solite/permit-join` 404 with its message, `Host: evil.example` 403. The
      live state in a browser, a 5th live client, 413, 411, 431, 13 connections, 1009: NOT RUN.
- [ ] the garage set up through the API, the motion switching the light: NOT RUN (no ZBMINIR2
      paired).
- [ ] the 33 requests: NOT RUN (`measure-api.sh` stops on `no device with an order`: it needs a
      relay).
- [ ] the internal RAM: `memory-network: internal 171 KB free (largest block 62 KB), psram 1904 KB
      free of 2048 KB`, 10 s after `serving:`, one device known; the boot's `memory:` line showed
      289 KB internal free, so Wi-Fi, the server and the engine's start together took about 118 KB
      of internal RAM (a difference of two snapshots, not R9.1's difference of two `heap:` minima).
      The `heap:` minimum of a run, R9.1's network cost and the projection for the realistic home
      are NOT computed; the knob of R9.3 was not applied.
- [ ] the lost link and its return, the garage working meanwhile: NOT RUN.
- [ ] no 1970 date on the flash; `--forget` leaves no connection: NOT RUN (no `clock:` line was seen
      in the 20 s capture).
- [x] the radio `app.bin`: 1774 KB (1816560 bytes) of 4096 KB (the release build of the last source, 2026-11-22; the box ran debug builds).

Not run either: the power-on pairing with the access point on or off, the console's stack
high-water mark, the Mac host refused on port 6638. Pairing the ZBMINIR2 needed a power-on more than
10 minutes after a boot (a software restart opens no pairing window): answered by D36's `pair`
(second run, below).

Second run (2026-11-22, the radio image with the three changes made after the first run flashed,
Wi-Fi 192.168.0.46, the SNZB-03PR2 and the ZBMINIR2 paired; the Mac host not running):

- [x] `sh scripts/box.sh pair` (D36): `pairing: open for 180 s` after the reset of the port; the
      ZBMINIR2 paired on the box in this run.
- [x] the garage set up through the API (`demo-garage.sh`), the motion before the SNZB-03PR2
      switching the ZBMINIR2 (the relay heard clicking).
- [x] `/ws` live in Chromium (D37): the welcome and the events, 7 reloads out of 7; 4003 for a
      foreign `Host` and a foreign `Origin`; the development origin `http://localhost:5173`
      welcomed; with a browser tab holding one slot, a script's 4th and 5th clients closed 1013, the
      same on a second try (the closed ones freed theirs); a message of 1025 bytes closed 1009.
- [x] `motionSince` before the clock: the interface showed `Calme · 20779 j` (from 1970); fixed in
      the core (no `motionSince` before 2020-01-01), with its test.
- [x] the server's limits: a 17 KB body 413, a chunked body 411, a header of 1100 bytes 431 then a
      new request 200, 13 connections held: 2 closed by the box and a new request answered 200;
      `/` with `Host: evil.example` 403; `permit-join` 404 with its message for `GET` and for a JSON
      `POST` (a `POST` with no `Content-Type` gets 415 first: the JSON-only rule runs before the
      route); port 6638 refused.
- [x] the `clock:` line: `clock: synced, 2026-11-22T22:34:43.000Z` 19.5 s after the boot, after
      `wifi: connected` (7.5 s); `memory-network: internal 163 KB free (largest block 62 KB), psram
      1904 KB free of 2048 KB`. The ZBMINIR2, which spoke at 5.9 s, kept a `lastSeen` of
      `1970-01-01T00:00:04.639Z` (R3.3: a date taken before the clock; roadmap inbox).
- [ ] the 33 requests: eight runs of `measure-api.sh`, never a 503 `Busy`, totals 14.4, 9.0, 10.5,
      10.7, 16.1, 19.1, 22.3 and 20.6 s, so over the 10 s of R10 in seven; in two runs one request
      got no answer (curl status 000), the 32 others 201. Each creation takes 0.15 to 0.65 s (a GET
      about 0.05 s). FAILED: roadmap inbox.
- [ ] NOT RUN: the lost link (access point off), no 1970 date on the flash after a boot with the
      access point off, a wrong password, `wifi --forget`, the SSID grep, the 4 requests in progress
      and the 5th `Busy`, the unpowered relay, the console's long and unknown lines, a client that
      stops reading, the power-on cases, the `heap:` minimum of a run and the projection, the
      console's stack high-water mark: roadmap inbox.
