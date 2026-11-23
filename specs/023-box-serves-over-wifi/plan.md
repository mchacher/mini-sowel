# Plan: Spec 023

Starts once spec 022 (`specs/022-serve-core-and-ui-embed/`) is merged: this plan uses its core
rules, `crates/ui-embed` and scripts, with the three core additions and the two script changes
named in `spec.md` (context).

## Steps

- [x] 1. Living spec updated (the delta of `spec.md`), D33's two sentences changed (the development
      origins on `/ws`, decided by the user; the socket table of 16) and the rest of D33 checked
      against this spec, the roadmap notes applied (E3c reads Wi-Fi only, the E3e line, the E3d
      line, the inbox lines), the sentences of "No longer true" rewritten, and a new search for the
      same words over `docs/`, `README.md`, `CLAUDE.md` and `.claude/skills/`.
- [x] 2. Core, tests first (the core rows below), then the code: `api::static_reply`
      (`crates/core/src/api/mod.rs`), the saver's gate `Saver::with_gate`
      (`crates/core/src/engine/saver.rs`), `WifiSettings::to_json` (`crates/core/src/network.rs`)
      and the doc comment of `boot_lines`. `cargo test -p solite-core` green; the host unchanged.
- [x] 3. Firmware: `build.rs` embedding for `radio` through `crates/ui-embed` (with
      `rerun-if-env-changed=SOLITE_UI_DIST`); `sdkconfig.radio.defaults` (the settings of
      `architecture.md`, `CONFIG_LWIP_MAX_SOCKETS` 16 and lwIP's task on core 0 included);
      `wifi.rs` (station with no NVS and RAM storage, power save, log tag, backoff, SNTP, the
      `CLOCK_SET` gate flag, `memory-network:`); `console.rs` (`to_json` written, the stop-save-
      restart); `http_server.rs` (static through `static_reply`, API with slots and async workers
      whose stacks and body buffers are in PSRAM, their queue of 4, bodies, lists, orders);
      `live_ws.rs` (Host and Origin from the raw request); `radio_engine.rs` and `main.rs` wiring
      (shared home, channel, the saver's gate, entropy switch, forced save, restart, the comments
      of "No longer true"). Built and linted for the four images; the radio `app.bin`'s size noted
      against the 4096 KB slot (R7.5: beyond it, stop and report).
- [x] 4. Scripts, tests first (the script rows below), then the code: `box.sh flash`'s `solite-ui `
      check; `box.sh log --image radio` and `scripts/lib/firmware.sh` checks, `fw_missing_line`'s
      radio `network:` line included (l.111); `box.sh wifi`'s answer window 15 s
      (`SOLITE_WIFI_ANSWER_SECONDS`, l.628) and its header comment; `validate.sh` (`ui/dist`
      always built before the radio image, the embed and settings checks); CI's firmware job gets
      Node.
- [x] 5. `sh scripts/validate.sh` green.
- [x] 6. Review (`code-reviewer`), blocking findings fixed.
- [ ] 7. The run on the box, with the user's explicit yes and MAC for the flash (D6): the demo of
      `spec.md` and the rows "Checked on the box only"; figures written in `docs/memory-budget.md`,
      `docs/hardware.md`, the result in `spec.md`. If the internal RAM minimum or the projection is
      under 64 KB (R9.3): `CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL` 2048 in `sdkconfig.radio.defaults`
      (approved by the user), rebuilt, flashed again with the user's yes, measured again; still
      under: stop and report the figures. Partial (2026-11-22): Wi-Fi, serving and three answers
      checked, `memory-network:` written. Second run (2026-11-22): the garage, the motion switching the
      ZBMINIR2, `/ws` live in a browser, `box.sh pair`, the `clock:` line, the error cases of the
      server and the 33 requests (over 10 s); the rest NOT RUN (the column "Run 2026-11-22" below
      and "Result of the run" in `spec.md`).
- [x] 8. Roadmap reviewed (E3c status, inbox).
- [x] 9. After the run on the box (2026-11-22), tests first: the console's `pair` line (D36) in the
      core, the firmware and `box.sh pair`; `/ws` registered directly with ESP-IDF (D37) and its
      setting checked by `validate.sh`; `motionSince` waits for a clock that is set; the docs of
      "Added after the run on the box" in `spec.md`; `validate.sh` green; review (`code-reviewer`).

## Test plan

Core tests are Rust integration tests in `crates/core/tests/`. Script tests are Rust tests in
`crates/host/tests/box_script.rs` that run the scripts with fakes (no real port, no real box). The
firmware has no tests (its logic is in the core, tested by spec 022 and the core rows below); it is
built and linted by `validate.sh`, and run on the box.

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| core `api::static_reply` (`tests/static_files.rs`) | `GET /` under `Reach::Lan` with `Host: evil.example` | `Some` 403 `{"error":"Forbidden host"}` |
| core `api::static_reply` | `POST /` with the box's `Host` (`solite-3abcde`) | `Some` 405 `{"error":"Method not allowed"}` |
| core `api::static_reply` | `GET /` and `GET /assets/index-abc.js` with the box's `Host`, and with `192.168.1.20` | `Some` file: `index.html` (no-cache) and `assets/index-abc.js` (immutable), their content types |
| core `api::static_reply` | `GET /api/v1/zones` with the box's `Host` | `None` (left to `handle_stream`) |
| core `Saver::with_gate` (`tests/engine.rs`) | Gate closed; a zone and a device changed; ticks over 120 s | nothing written to the memory storage, nothing shown on the sink; both still due |
| core `Saver::with_gate` | The gate opens 30 s after a closed tick | the next tick writes `zones.json` and `devices.json` (no `RETRY_MS` delay, no failure counted) |
| core `Saver::with_gate` | Stop (`tick(.., true)`) while the gate is closed | the files due are written |
| core `WifiSettings::to_json` (`tests/network.rs`) | Settings with `"`, `\`, a non-ASCII SSID, and an empty password | `wifi_settings(&to_json())` gives the same settings; the text holds the keys `ssid` and `password` only, within `MAX_WIFI_BYTES` for the longest SSID and password |
| script `box.sh flash` | A radio build without the `solite-ui ` marker | refused with the build command, nothing written |
| script `box.sh log` | Radio log with `network: wifi (setting auto)` (no `no link started`), `wifi: connected, ...`, `serving: http://...`, `clock:`, `memory-network:` | passes; prints the four lines |
| script `box.sh log` | Radio log with `network: ethernet (setting ethernet), not in this image, no link started` and no `wifi:` line | passes; prints the network line |
| script `box.sh log` | Radio log with `wifi: no settings ...` and no `serving:` | passes; prints the `wifi:` line |
| script `box.sh log` | Radio log with `wifi: connected` but no `serving:` after it | fails: `the log is missing the serving line (serving: http://...)` |
| script `box.sh log` | Radio log with `wifi: settings refused (invalid)` | fails, naming it |
| script `box.sh log` | Radio log with `network: wifi (setting auto)` and no `wifi:` line at all | fails: missing the Wi-Fi line |
| script `box.sh log` | Radio log whose `heap:` internal minimum is 63 KB | fails (existing rule, kept) |
| script `box.sh log` | Box and sim logs (existing tests) | unchanged checks, `no link started` still required |
| script `box.sh wifi` | A console that answers `wifi: settings saved` 6 s after the line, then `wifi: connected, address ...` | exit 0, the address printed (the 15 s window) |
| script `box.sh wifi` | A silent console, `SOLITE_WIFI_ANSWER_SECONDS=1` (the existing test, adapted to stay fast) | exit 4, `no answer from Solite's console` |
| core `parse_console_line` (`tests/network.rs`) | `pair`, `pair\r`, `pair now` | `Pair`, `Pair`, `Unknown` |
| core `Home::aggregate` (`tests/aggregate.rs`) | A motion sensor bound while the clock reads 6 s (1970), then the clock set | `motionSince` null, then the time of the first computation after the clock, kept afterwards |
| script `box.sh pair` | A console that answers `pairing: open for 180 s` | the line `pair` sent, exit 0, `box: pairing is open (open for 180 s)` printed, no prompt |
| script `box.sh pair` | A silent console, `SOLITE_WIFI_ANSWER_SECONDS=1` | the line `pair` sent, exit 4, `no answer from Solite's console` |
| script `box.sh pair` | A console that answers `pairing: failed: the radio refused` | exit 1, the line printed |
| script `box.sh pair` | A console that answers `pairing: asked from the console` only (from the code review) | exit 1, the engine named, not `no answer` |
| `validate.sh` (firmware step) | The radio build's `sdkconfig` | holds `CONFIG_HTTPD_WS_POST_HANDSHAKE_CB_SUPPORT=y`, else stops naming it |
| `validate.sh` (firmware step) | `ui/node_modules` missing | stops with `cd ui && npm ci` named; checked by hand once, as the other validate branches |

Bounds covered here: the 64 KB internal minimum (the existing `log` test), `MAX_WIFI_BYTES` for the
text `to_json` writes, `box.sh wifi`'s 15 s answer window. The others are covered by spec 022's
tests (`MAX_WIFI_BYTES` of the parser, `MAX_CONSOLE_LINE`, SSID and password lengths,
`MAX_BODY_BYTES`, `MAX_WORKERS`, `MAX_UI_FILES`, 1 MB per UI file, `MAX_BINDINGS`,
`WIFI_RETRY_MAX_MS`, `live::MAX_CLIENTS` on the host). `live::MAX_CLIENTS` on the box (1013),
`CLOCK_WAIT_MS`, `MAX_OPEN_SOCKETS`, the workers' queue of 4, `CONFIG_LWIP_MAX_SOCKETS` (ESP-IDF
refuses to start the server below 15), `CONFIG_HTTPD_MAX_REQ_HDR_LEN`, `BODY_BUFFER_BYTES` and the
5 s timeouts live in the firmware or ESP-IDF: checked in the run on the box (rows below), not by a
test.

**Sowel JSON unchanged**: no type, field or event is added or changed; spec 022's row "the same
request sequence under `Loopback` and `Lan`" checks the bodies byte for byte, and the box answers
through the same `handle_stream`.

### Checked on the box only (the run of step 7)

| What | Expected | Run 2026-11-22 |
| ---- | -------- | -------------- |
| `box.sh wifi --forget`, then a boot (`box.sh log --image radio`) | `wifi: no settings ...`, no `wifi: connected`, no `serving:`, the engine runs (no copy kept in NVS) | NOT RUN |
| Wrong password set with `box.sh wifi` | `wifi: not connected (wrong password or refused), retry in ...`, doubling; `box.sh wifi` exits 1 | NOT RUN |
| Access point off for a minute, then on | `wifi: lost (...)`, retries; a motion before the sensor while it is off switches the ZBMINIR2; then `wifi: connected`, the interface reconnects | NOT RUN |
| The interface from the Mac | loads, gzip, cached assets (a second load fetches only `index.html`) | partly: `/` answered 200; gzip and the cache NOT RUN |
| A 5th browser tab | closed 1013, reconnecting; the 4 others live | NOT RUN |
| `curl` with `Host: evil.example` on `/api/v1/zones` and on `/` | 403 both; a page of another origin on `/ws`: closed 4003 | seen: 403 on an API route and on `/`; `/ws` closed 4003 for `Host: evil.example` and for `Origin: http://evil.example` (second run) |
| A `/ws` client (small Python client) with `Origin: http://localhost:5173` | the welcome message (development origin accepted, decided by the user) | seen (second run) |
| `curl -X POST .../permit-join` | 404 `Pairing from the API is off on the box` | seen: 404 with its message |
| A 17 KB body | 413 | seen: 413 `Body too large` (second run) |
| `curl -X POST -H 'Transfer-Encoding: chunked' -H 'Content-Type: application/json' --data-binary @body.json .../api/v1/zones` | 411 `Length required` | seen (second run) |
| `curl` with a header of 1100 bytes (one long `X-Pad` header) | 431 from ESP-IDF; the next request on a new connection answered normally | seen: 431, then 200 (second run) |
| 13 connections opened and held (a small Python script on the Mac), then a request on the first | the least recently used connection closed by the box; the request on a new connection answered; with one browser tab open, its live client reconnects if it was the one closed | seen: 2 of the 13 closed by the box, a request on a new connection answered 200, a browser tab open (second run); its reconnection not watched |
| 4 requests in progress at once (a small Python script: 4 `POST /api/v1/zones` with `Content-Length: 100`, bodies sent one byte a second), then a 5th request | the 5th answered 503 `{"error":"Busy"}` at once; the 4 answered once their bodies end | NOT RUN |
| The ZBMINIR2 unpowered; 3 orders to it at once | 2 answered `device did not answer` after 12 s, the 3rd 503 `Busy` at once | NOT RUN |
| A line of 600 bytes, then `hello`, sent on the serial console (pyserial on the Mac) | `console: line over 512 bytes, ignored`, `console: unknown command`; neither line's text in the log | NOT RUN |
| A `/ws` message of 1025 bytes (`websocat` or a small Python client, `Origin` absent) | closed 1009 `Message too big` (second run); the other clients not watched |
| A `/ws` client that stops reading (a small Python client with a small receive buffer) while events flow | dropped after a blocked write of at most 5 s, or by keep-alive; the other live client stays live; a `curl` during it answered within about 5 s | NOT RUN |
| `GET /api/v1/devices` in a loop while `measure-api.sh` runs (each binding writes `equipments.json` to the flash) | every answer 200, none lost; the interface's live state late at most, no 1013 for the browser tab | NOT RUN |
| A power-on (USB unplugged and plugged back) with the access point on | `pairing: open for 180 s` before `clock: synced`; a device paired in the window; the first save written after `clock:`, with dates of the day | NOT RUN |
| A power-on with the access point off | `pairing: open for 180 s` without waiting; `clock: not synced after 20 s ...` 20 s after `wifi: connecting`; the engine runs, its saves written after it | NOT RUN |
| A power-on with the access point on but its Internet cut (WAN unplugged) | `wifi: connected`, `serving:`, then `clock: not synced after 20 s ...`; the interface and the API work; WAN back: one later `clock: synced` line | NOT RUN |
| In that boot, a device paired during the clock wait (1970 dates), then the WAN back | at the clock's jump the device shows `offline` in `GET /api/v1/devices` until its next message, then `online` | NOT RUN |
| No 1970 date on the flash: after a boot with the access point on has saved, a power-on with the access point off, then the access point on | `GET /api/v1/devices` before any device has spoken since that boot: every `lastSeen` of the day, none in 1970 | NOT RUN |
| The console thread's stack: its high-water mark after a `box.sh wifi` (it writes the flash), in that run's `console: restarting (stack ... B unused of 4096 B)` line | a margin left on its stack (the figure written in `docs/memory-budget.md`); none: the stack raised and the run made again | NOT RUN |
| A `/ws` message sent in two fragments (a small Python client, `Origin` absent) | accepted as no concern (spec, "Decided in autonomy"): the box answers or closes the connection, no other client affected, the engine runs on | NOT RUN |
| The SSID in the logs | `grep -c '<SSID>' <log file>` on the Mac prints 0 for every capture of the run | NOT RUN |
| A 5th live client | closed 1013 | seen: with a browser tab holding one slot, the 4th and 5th clients of a script closed 1013, the same count on a second try (no slot kept by the closed ones) (second run) |
| `sh scripts/box.sh pair` on the box (D36) | `pairing: open for 180 s` within 15 s of the reset | seen (second run) |
| The radio `app.bin` | its size noted in the run's result, under the 4096 KB slot | seen: its size in `spec.md` |
| The 33 requests | `measure-api.sh` passes (R10) | FAILED (second run, eight runs): never `Busy`, but totals 9.0 to 22.3 s, over 10 s in seven; one request with no answer in two runs (roadmap inbox) |
| The internal RAM | `heap:` minimum ≥ 64 KB; `memory-network:`, the network's cost (234 KB − the first `heap:` minimum) and the projection written (R9.1, R9.3); below 64 KB: the knob `SPIRAM_MALLOC_ALWAYSINTERNAL` 2048, measured again; still below: stop and report | partly: `memory-network:` written; the minimum, the cost and the projection NOT RUN |
| A Mac host at `--radio tcp://<box>:6638` | connection refused | seen with `nc`: `Connection refused` (second run) |

58 scenarios: 28 in the table above (27 tested by `validate.sh`: 10 core, 16 script and the settings
check; 1 checked by hand), 30 checked on the box. The other 74 scenarios of E3c (the core, `ui-embed`, `box.sh wifi`,
`--keep-storage`, `demo-garage.sh`, `measure-api.sh`) are in spec 022's plan.

## Validation

- [x] `sh scripts/validate.sh` green (docs, format, lint, types, Rust tests including the core's and
      the scripts', UI budget, the four firmware images with the radio one embedding the interface,
      QEMU unchanged)
- [x] Agent review done (`code-reviewer`), blocking findings fixed
- [ ] The run on the box done with the user's yes (flash), its result written in `spec.md`: partial,
      its result written
- [ ] Demo seen and approved by the user
