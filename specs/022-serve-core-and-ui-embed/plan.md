# Plan: Spec 022

## Steps

- [x] 1. Living spec updated (the delta of `spec.md`), D33 written to `docs/decisions.md`, the other
      docs and `CLAUDE.md`, the sentences of "No longer true" rewritten, and a new search for the
      same words over `docs/`, `README.md`, `CLAUDE.md` and `.claude/skills/`.
- [x] 2. Core, logic moved without change of behaviour: `live/session.rs` (origin, subprotocol,
      constants) and the host's `ws.rs` using it; `api/slots.rs` and the host's `http_loop` using it;
      the static headers and `cache_control` into `api/static_files.rs`, the host using them. The
      host's tests pass unchanged (the "unchanged" rows below) before anything new is added.
- [x] 3. Core, new: `Request.reach` and the LAN `Host` rule; the pairing route's refusal under
      `Reach::Lan`; `network.rs` Wi-Fi settings, refusals, `Backoff`, `wifi_reason`,
      `boot_lines_radio`, the `wifi:` lines, `parse_console_line`; `heap_line`'s http stack;
      `static_files::find`; the clock jump test. Tests first (the plan below), then the code.
- [x] 4. `crates/ui-embed`: `collect`, `generate`, their tests; the workspace member.
- [x] 5. Scripts: `box.sh wifi` and `--forget`, `flash --keep-storage`, the ownership line;
      `demo-garage.sh` address; `measure-api.sh`. Their tests.
- [x] 6. `sh scripts/validate.sh` green (the four firmware images and QEMU unchanged).
- [x] 7. Review (`code-reviewer`), blocking findings fixed.
- [x] 8. The demo on the Mac ("How the user will check it" of `spec.md`).
- [x] 9. Roadmap reviewed (E3c: part 1 done, spec 023 next).

Build order of the skill (3.2): living spec, core, adapters (host, scripts), `ui-embed`; the API
routes and the interface in `ui/` are not applicable (no route, type or screen changes).

## Test plan

Rust tests run on the Mac (`cargo test --workspace`); script tests are Rust tests in
`crates/host/tests/` that run the scripts with fakes (no real port, no real box).

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| core `network` (Wi-Fi settings) | Nominal: `{"ssid":"Home","password":"abcdefgh"}` | settings with that SSID and password |
| core `network` | Open network: password `""` | accepted; the line builder says `open network` |
| core `network` | Raw key: 64 hex digits | accepted |
| core `network` | Missing text (no file) | `None` handled by the caller: the line `wifi: no settings (/data/wifi.json missing), no link started: sh scripts/box.sh wifi` |
| core `network` | Empty and blank texts | refused `empty` |
| core `network` | 257 bytes | refused `too long`, not parsed |
| core `network` | Not JSON; a JSON array; `ssid` a number | refused `invalid` |
| core `network` | An extra field (`"link"`) | refused `invalid` |
| core `network` | SSID `""`; SSID of 33 bytes; SSID of 32 bytes of UTF-8 | `invalid ssid`, `invalid ssid`, accepted |
| core `network` | Password of 7 characters; of 64 non-hex characters; of 63 printable; with a control character | `invalid password`, `invalid password`, accepted, `invalid password` |
| core `network` | `Debug` of the settings and every `wifi:` line built from them | never contains the password nor the SSID (checked with a distinctive password and SSID) |
| core `network` | `Backoff` from the start | 1, 2, 4, 8, 16, 32, 60, 60 s; after `reset`, 1 s again |
| core `network` | `wifi_reason` of 201, 15, 202, 204, 2, 200, 999 | `network not found`, `wrong password or refused` (four times), `beacon lost`, `reason 999` |
| core `network` | `retry_line` before and after a first connection | `wifi: not connected (<reason>), retry in <n> s` / `wifi: lost (<reason>), retry in <n> s` |
| core `network` | `boot_lines_radio` for `auto` and `wifi` | `network: wifi (setting <s>)`, no `no link started` |
| core `network` | `boot_lines_radio` for `ethernet` | `network: ethernet (setting ethernet), not in this image, no link started` |
| core `network` | `boot_lines` (the other images), every existing case | unchanged byte for byte (existing tests pass) |
| core `network` (console) | `parse_console_line`: a line of exactly 512 bytes (`x` repeated); a `wifi-set` line of 512 bytes (its JSON over 256 bytes) | `Unknown` (read, not dropped); `Refused(TooLong)` (R1's bound, the line itself read) |
| core `network` (console) | A line of 513 bytes (`x` repeated, and a `wifi-set` line) | `TooLong` both, nothing parsed |
| core `network` (console) | `reboot`; `wifi-forget now`; bytes that are not UTF-8 | `Unknown` each |
| core `network` (console) | `wifi-set {"ssid":"Home","password":"abcdefgh"}`, with and without a trailing `\r` | `WifiSet` with that SSID and password, the same both ways |
| core `network` (console) | `wifi-set {"ssid":"Home","password":"short"}`; `wifi-set` with no JSON | `Refused(InvalidPassword)`; `Refused(Empty)` |
| core `network` (console) | `wifi-forget`; `wifi-forget\r` | `WifiForget` |
| core `network` (console) | An empty line; `\r` alone | `Empty` |
| core `network` (console) | `Debug` of every `ConsoleCommand` built from a `wifi-set` line | never contains the password nor the SSID |
| core `api` (Host rule) | `Reach::Lan`, Host `192.168.1.20`, `192.168.1.20:80`, `10.1.2.3`, `172.16.0.5`, `172.31.255.1`, `127.0.0.1:80`, `localhost` | allowed |
| core `api` | `Reach::Lan`, Host `solite-a1b2c3`, `SOLITE-A1B2C3.local`, `.lan`, `.home`, `.home.arpa` | allowed |
| core `api` | `Reach::Lan`, Host `8.8.8.8`, `172.32.0.1`, `192.169.0.1`, `[fe80::1]`, `evil.example`, `solite-a1b2c3.evil.example`, `solite-ffffff` (another box), `192.168.1.20:8080`, missing | 403 `Forbidden host` |
| core `api` | `Reach::Loopback` (the host), every existing case | unchanged (existing tests pass) |
| core `api` (pairing route) | `Reach::Lan`, `GET` and `POST /api/v1/integrations/solite/permit-join` | 404 `Pairing from the API is off on the box`; the pairing state untouched |
| core `api` | `Reach::Loopback`, the same requests | 200 / 202 as spec 017 (existing tests pass) |
| core `api` (bounds) | `Reach::Lan`, a body of 16 384 bytes, then 16 385 | accepted (parsed), then 413 `Body too large` |
| core `api` (Sowel JSON) | The same request sequence under `Loopback` and `Lan` (zones, equipments, bindings, devices, recipes) | byte-identical bodies and statuses, but the pairing route |
| core `api` (33 requests) | `Reach::Lan`: 1 + 16 + 16 creation requests in sequence, the same data and order under 16 aliases | 33 × 201; the equipment holds 16 and 16 |
| core `api::Slots` | 4 taken, a 5th | the 5th refused; one dropped, the next taken |
| core `api::Slots` | A guard dropped while a panic unwinds | the slot is released |
| core `static_files` | `STATIC_HEADERS`, `cache_control` for `index.html` and an asset | the host's three headers; `no-cache` / `public, max-age=31536000, immutable` |
| core `static_files::find` | A sorted table of 4 files: an existing asset, `index.html`, a missing name | found, found, `None` |
| core `static_files` | `..`, `%2e%2e`, a sub-folder, an empty name (existing route) | 404 with the security headers (existing tests pass) |
| core `live::session` | `origin_allowed`: absent, own host, the two development origins, another site, `null`, a path in the origin | as the host's existing tests (moved, unchanged) |
| core `live::session` | `pick_subprotocol`: `bearer.none`; `x, bearer.abc`; none | `bearer.none`; `bearer.abc`; `None` |
| core `memory` | `heap_line` with and without the workers' stack | `..., engine stack 27 KB unused of 32 KB, http stack 6 KB unused of 10 KB`; without it, the line of spec 021 unchanged |
| core `engine` (clock) | A wall clock that jumps forward by 56 years while the loop runs | the engine goes on, no error, the next save written; a device that reports after the jump is online |
| host `ws` | The existing tests of `crates/host/tests/ws.rs` (origin, 503 for a 5th, 1009, 1013, pings) | pass unchanged after the move to `live::session` |
| host `serve` | The existing tests of `serve.rs` and `static_files.rs` (503 `Busy` past 4, headers) | pass unchanged after `Slots` and the headers move |
| `ui-embed` | Nominal: a folder with `index.html` and three assets | 4 files, each gzip decompresses to the original; `ui_files.rs` lists them sorted, with `include_bytes!` paths that exist; the marker `solite-ui 4 files, <n> KB` |
| `ui-embed` | `index.html` missing (folder missing, or empty) | error `interface: ui/dist not built: cd ui && npm ci && npm run build` |
| `ui-embed` | 33 files in `assets/` | error naming `MAX_UI_FILES` (32) |
| `ui-embed` | A file of 1 MB + 1 byte | error naming the file and the 1 MB bound |
| `ui-embed` | A sub-folder in `assets/`, a name starting with `.`, a name with `%` | error naming the file (the core's route would never serve it) |
| `ui-embed` | Two runs on the same folder | the same `ui_files.rs` (reproducible: no time in the gzip header) |
| `ui-embed` | The real `ui/dist` after `npm run build` (skipped when absent) | under 170 KB of gzip in all |
| script `box.sh wifi` | Nominal (pseudo-terminal, fake console answering `wifi: settings saved`, then `wifi: connected, address 192.168.1.20, name solite-a1b2c3, ...`) | the fake receives one line `wifi-set {"ssid":...,"password":...}` (valid JSON, password with quotes and a backslash escaped); exit 0; `box: Wi-Fi connected, address 192.168.1.20, ...` printed |
| script `box.sh wifi` | The password and the SSID in the script's output, in every file of the work folder, in the fake's argument list and environment | absent (only the fake's standard input holds them) |
| script `box.sh wifi` | Echo: the characters typed for the password | not echoed on the pseudo-terminal; echo on again after the script, also after Ctrl-C at the prompt (nothing sent) |
| script `box.sh wifi` | No terminal (answer piped on standard input) | `box: no terminal to ask for the Wi-Fi settings, nothing done`, exit 1, the fake not called |
| script `box.sh wifi` | The fake answers `wifi: settings refused (invalid password)` | printed, exit 1 |
| script `box.sh wifi` | The fake answers nothing within 5 s | `box: no answer from Solite's console on <port>: is the radio image running?`, exit 4 |
| script `box.sh wifi` | Saved, then `wifi: not connected (wrong password or refused), retry in 1 s` and no `connected` within the wait (shortened by an environment variable in the test) | the last `wifi:` line printed, exit 1 |
| script `box.sh wifi --forget` | Fake answers `wifi: settings removed` | the fake receives `wifi-forget`; exit 0 |
| script `box.sh flash` | `--image radio --keep-storage`, dry run | plan erases `nvs` and `otadata` only, says `keep storage (...)`, no `zigbee.json` warning; the ownership line shown |
| script `box.sh flash` | `--image radio --keep-storage`, confirmed | esptool fake called with two `erase_region` (nvs, otadata), never storage's offset; then the write |
| script `box.sh flash` | `--image radio` without `--keep-storage` | as today, the warning ends `(use --keep-storage to keep them)` |
| script `box.sh flash` | `--image sim --keep-storage` and `--image box --keep-storage` | exit 2, `--keep-storage is for the radio image: the sim image seeds /data` |
| script `demo-garage.sh` | Argument `8080`-style port (existing tests) | unchanged |
| script `demo-garage.sh` | Argument `127.0.0.1:<port>` against a test host | the garage set up, as with the port |
| script `demo-garage.sh` | Argument `127.0.0.1` (port 80, nothing listening) | `demo-garage: no host answers at 127.0.0.1:80 ...`, exit 1 |
| script `measure-api.sh` | Nominal against a host with a motion sensor and a relay | `measure: 33 requests, 33 answered 201, 0 Busy, total <s> s, slowest ...`, exit 0; afterwards no zone `Measure 33` and no equipment left |
| script `measure-api.sh` | `--keep` | the zone and the equipment (16 + 16 bindings) remain |
| script `measure-api.sh` | A zone `Measure 33` exists | refused, nothing created, exit 1 |
| script `measure-api.sh` | No device with an order | refused (`no device with an order: pair a relay first`), exit 1 |
| script `measure-api.sh` | A test host answering 503 `Busy` to the 10th request | `... 1 Busy ...`, the first non-201 answer printed, exit 1; the equipment and zone deleted |
| script `measure-api.sh` | A total above 10 s (a test host delaying each answer 400 ms) | the time printed, exit 1 |
| script `measure-api.sh` | No host answering | `measure-api: no API answers at <address>`, exit 1 |

74 scenarios. The other 27 scenarios of E3c (the `log --image radio` checks, the flash's refusal of
a radio build without the interface, the firmware step of `validate.sh`, and the 18 checked on the
box) are in spec 023's plan.

Bounds covered: `MAX_WIFI_BYTES` (256/257), `MAX_CONSOLE_LINE` (512/513), SSID 32/33 bytes, password
7/8, 63/64 characters, `MAX_BODY_BYTES` (16 384/16 385), `MAX_WORKERS` (4/5), `MAX_UI_FILES`
(32/33), 1 MB per UI file, `MAX_BINDINGS` (16 + 16), `WIFI_RETRY_MAX_MS` (the cap),
`live::MAX_CLIENTS` (the host's existing 5th-client test).

**Sowel JSON unchanged**: no type, field or event is added or changed; the row "the same request
sequence under `Loopback` and `Lan`" checks the bodies byte for byte, and the existing
`sowel_json.rs` tests pass unchanged.

## Validation

- [x] `sh scripts/validate.sh` green (docs, format, lint, types, Rust tests including `ui-embed` and
      the scripts', UI budget, the four firmware images and QEMU unchanged)
- [x] The host's existing tests pass unchanged after the moves
- [x] Agent review done (`code-reviewer`), blocking findings fixed
- [x] Demo on the Mac seen and approved by the user
