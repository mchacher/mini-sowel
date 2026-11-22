# Plan: Spec 023

Starts once spec 022 (`specs/022-serve-core-and-ui-embed/`) is merged: this plan uses its core
rules, `crates/ui-embed` and scripts unchanged.

## Steps

- [ ] 1. Living spec updated (the delta of `spec.md`), D33 (written by spec 022) checked against this
      spec, the roadmap notes applied (E3c reads Wi-Fi only, the E3e line, the E3d line, the inbox
      lines), the sentences of "No longer true" rewritten, and a new search for the same words over
      `docs/`, `README.md`, `CLAUDE.md` and `.claude/skills/`.
- [ ] 2. Firmware: `build.rs` embedding for `radio` through `crates/ui-embed`;
      `sdkconfig.radio.defaults`; `wifi.rs` (station, power save, log tag, backoff, SNTP, the saves
      held for the clock, `memory-network:`); `console.rs`; `http_server.rs` (static, API with slots
      and async workers whose stacks and body buffers are in PSRAM, bodies, lists, orders);
      `live_ws.rs`; `radio_engine.rs` and `main.rs` wiring (shared home, channel, entropy switch,
      forced save, restart). Built and linted for the four images; the radio `app.bin`'s size noted
      against the 4096 KB slot (R7.5: beyond it, stop and report).
- [ ] 3. Scripts: `box.sh flash`'s `solite-ui ` check; `box.sh log --image radio` and
      `scripts/lib/firmware.sh` checks; `validate.sh` (UI built before the radio image, the embed and
      settings checks); CI's firmware job gets Node. Their tests first, then the code.
- [ ] 4. `sh scripts/validate.sh` green.
- [ ] 5. Review (`code-reviewer`), blocking findings fixed.
- [ ] 6. The run on the box, with the user's explicit yes and MAC for the flash (D6): the demo of
      `spec.md`; figures written in `docs/memory-budget.md`, `docs/hardware.md`, the result in
      `spec.md`; R9.3 applied if the internal RAM is short (stop and report).
- [ ] 7. Roadmap reviewed (E3c status, inbox).

## Test plan

Script tests are Rust tests in `crates/host/tests/` that run the scripts with fakes (no real port, no
real box). The firmware has no tests (its logic is in the core, tested by spec 022); it is built and
linted by `validate.sh`, and run on the box.

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| script `box.sh flash` | A radio build without the `solite-ui ` marker | refused with the build command, nothing written |
| script `box.sh log` | Radio log with `wifi: connected, ...`, `serving: http://...`, `clock:`, `memory-network:` | passes; prints the four lines |
| script `box.sh log` | Radio log with `wifi: no settings ...` and no `serving:` | passes; prints the `wifi:` line |
| script `box.sh log` | Radio log with `wifi: connected` but no `serving:` after it | fails: `the log is missing the serving line (serving: http://...)` |
| script `box.sh log` | Radio log with `wifi: settings refused (invalid)` | fails, naming it |
| script `box.sh log` | Radio log with no `wifi:` line at all | fails: missing the Wi-Fi line |
| script `box.sh log` | Radio log whose `heap:` internal minimum is 63 KB | fails (existing rule, kept) |
| script `box.sh log` | Box and sim logs (existing tests) | unchanged checks, `no link started` still required |
| `validate.sh` (firmware step) | `ui/dist` missing and `ui/node_modules` missing | stops with `cd ui && npm ci` named; checked by hand once, as the other validate branches |

Bounds covered here: the 64 KB internal minimum (the existing `log` test). The others are covered by
spec 022's tests (`MAX_WIFI_BYTES`, `MAX_CONSOLE_LINE`, SSID and password lengths, `MAX_BODY_BYTES`,
`MAX_WORKERS`, `MAX_UI_FILES`, 1 MB per UI file, `MAX_BINDINGS`, `WIFI_RETRY_MAX_MS`,
`live::MAX_CLIENTS` on the host). `live::MAX_CLIENTS` on the box (1013), `CLOCK_WAIT_MS`,
`MAX_OPEN_SOCKETS`, `CONFIG_LWIP_MAX_SOCKETS`, `CONFIG_HTTPD_MAX_REQ_HDR_LEN`, `BODY_BUFFER_BYTES`
and the 5 s timeouts live in the firmware or ESP-IDF: checked in the run on the box (rows below), not
by a test.

**Sowel JSON unchanged**: no type, field or event is added or changed; spec 022's row "the same
request sequence under `Loopback` and `Lan`" checks the bodies byte for byte, and the box answers
through the same `handle_stream`.

### Checked on the box only (the run of step 6)

| What | Expected |
| ---- | -------- |
| `wifi.json` missing (after `box.sh wifi --forget`) | `wifi: no settings ...`, no `serving:`, the engine runs |
| Wrong password set with `box.sh wifi` | `wifi: not connected (wrong password or refused), retry in ...`, doubling; `box.sh wifi` exits 1 |
| Access point off, then on | `wifi: lost (...)`, retries, `wifi: connected`, the interface reconnects, the garage kept working |
| The interface from the Mac | loads, gzip, cached assets (a second load fetches only `index.html`) |
| A 5th browser tab | closed 1013, reconnecting; the 4 others live |
| `curl` with `Host: evil.example` | 403; a page of another origin on `/ws`: closed 4003 |
| `curl -X POST .../permit-join` | 404 `Pairing from the API is off on the box` |
| A 17 KB body | 413 |
| `curl -X POST -H 'Transfer-Encoding: chunked' -H 'Content-Type: application/json' --data-binary @body.json .../api/v1/zones` | 411 `Length required` |
| `curl` with a header of 1100 bytes (one long `X-Pad` header) | 431 from ESP-IDF; the next request on a new connection answered normally |
| 13 connections opened and held (a small Python script on the Mac), then a request on the first | the least recently used connection closed by the box; the request on a new connection answered; with one browser tab open, its live client reconnects if it was the one closed |
| A `/ws` message of 1025 bytes (`websocat` or a small Python client, `Origin` absent) | closed 1009; the other live clients unaffected |
| A power-on (USB unplugged and plugged back) with the access point on | `pairing: open for 180 s` before `clock: synced`; a device paired in the window; the first save written after `clock:`, with dates of the day |
| A power-on with the access point off | `pairing: open for 180 s` without waiting; `clock: not synced after 20 s ...` 20 s after `wifi: connecting`; the engine runs, its saves written after it |
| The radio `app.bin` | its size noted in the run's result, under the 4096 KB slot |
| The 33 requests | `measure-api.sh` passes (R10) |
| The internal RAM | `heap:` minimum ≥ 64 KB; `memory-network:` and the projection written (R9.3); the projection likely under 64 KB (R9.4): stop and report |
| A Mac host at `--radio tcp://<box>:6638` | connection refused |

27 scenarios: 9 tested by `validate.sh`, 18 checked on the box. The other 74 scenarios of E3c (the
core, `ui-embed`, `box.sh wifi`, `--keep-storage`, `demo-garage.sh`, `measure-api.sh`) are in spec
022's plan.

## Validation

- [ ] `sh scripts/validate.sh` green (docs, format, lint, types, Rust tests including the scripts', UI
      budget, the four firmware images with the radio one embedding the interface, QEMU unchanged)
- [ ] Agent review done (`code-reviewer`), blocking findings fixed
- [ ] The run on the box done with the user's yes (flash), its result written in `spec.md`
- [ ] Demo seen and approved by the user
