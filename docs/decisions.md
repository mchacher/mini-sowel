# Decisions

Every choice that shapes the project, with its reasons, so whoever comes next understands it.
Newest last. A decision is never edited: a new one replaces it and says so.

## D1: Stack

- **Date**: 2026-11-20
- **Choice**:
  - **Core and host program**: Rust stable (pinned in `rust-toolchain.toml`), a Cargo workspace
    (`crates/core`, `crates/host`). Checks: `cargo fmt`, `cargo clippy -D warnings`, `cargo test`;
    the compiler is the type check.
  - **ESP32 firmware**: Rust with its standard library (`std`) on ESP-IDF, through `esp-idf-svc`;
    the Xtensa toolchain installed by `espup` (`firmware/rust-toolchain.toml`, channel `esp`);
    ESP-IDF pinned in `firmware/.cargo/config.toml` (v5.5.5 at setup). A Cargo project of its own
    in `firmware/`, outside the Mac workspace, using the same `crates/core`.
  - **Interface**: Preact (with `preact/compat`, so React components of Sowel work unchanged),
    TypeScript strict, Vite, Tailwind 4. Checks: Prettier, ESLint (typescript-eslint and Sowel's
    design rules), `tsc`, Vitest, a first-load size budget.
  - **One command**: `sh scripts/validate.sh` (all), `--fast` for the pre-commit hook.
  - **Tests**: Rust integration tests in `crates/<crate>/tests/`, unit tests in-module; UI tests
    beside the components.
- **Target architecture** (from the framing note, `inputs/note-de-cadrage-mini-sowel.md`):

  ```
  ┌──────────────────────────── ESP32 (Rust) ────────────────────────────┐
  │  Sowel core (no hardware dependency): devices, equipments, zones,    │
  │  modes, WebAssembly recipes, recipe proposals                        │
  │      │              │               │                │               │
  │  Radio adapter  Storage adapter  HTTP / WS adapter  OTA adapter      │
  └──────┼──────────────────────────────┼────────────────────────────────┘
         │ UART (EZSP)                  │ Ethernet PoE / Wi-Fi
  ┌──────┴───────────┐          ┌───────┴────────────────────────────┐
  │ EFR32MG24        │          │ Mini PWA, sowel-plugin-mini,       │
  │ EmberZNet (NCP)  │          │ signed updates                     │
  └──────┬───────────┘          └────────────────────────────────────┘
         │ Zigbee: sensors, lights, plugs, shutters
  ```

  The core reaches the hardware only through the four adapters. On the Mac, `crates/host` gives
  it a simulated radio (replayed scenarios) or the real radio over TCP (the Dongle Max's original
  firmware) or USB (an EFR32 dongle); on the box, `firmware/` gives it the real ones. The
  interface is built into the firmware and served by the box. Recipes run in `wasmi`; device
  definitions come from an export of Zigbee2MQTT's converters (a Node tool in `tools/`, run in CI).
- **Why**: ESP-IDF already provides what the true north needs to run a month without
  intervention: OTA on two partitions with automatic rollback, the Dongle Max's Ethernet, Wi-Fi,
  an HTTP and WebSocket server, flash storage. With `std`, Rust is written the same way on the Mac
  and on the ESP32, which matters for a user new to Rust, and keeps about 90 % of the work on the
  Mac. The interface stays in TypeScript, which the user knows, and can reuse Sowel's components.
- **Alternative considered**: Rust `no_std` with `esp-hal` and Embassy, the path Espressif now
  backs: pure Rust, smaller binaries, finer memory control; but OTA with rollback and the HTTP
  server must be assembled, Ethernet on the classic ESP32 is marked "unstable" in `esp-hal` 1.2,
  and `no_std` is harder for a Rust beginner. The cost of the choice: `esp-idf-svc` is maintained
  by the esp-rs community with little paid time from Espressif, and the first build downloads
  ESP-IDF (a few GB). Because the core has no ESP-IDF dependency, moving to `esp-hal` would change
  `firmware/` only.

## D2: Git and hosting

- **Date**: 2026-11-20
- **Choice**: local git, branch `main`; remote: none yet, `/add-remote` connects one later (GitHub,
  beside `mchacher/sowel`, with GitHub Actions running `sh scripts/validate.sh`).
- **Why**: the project starts on one computer; the remote and its CI come when the user wants to
  publish. Commits use the same identity as Sowel, set for this repository only.

## D3: Sowel contract

- **Date**: 2026-11-20
- **Choice**: everything mini-Sowel exposes takes the names, types and JSON of Sowel's API; the
  reference is Sowel's `src/shared/types.ts`, which wins over Sowel's docs where they disagree
  (for example `zone.data.changed` carries `{zoneId, aggregatedData}`, as in `types.ts`, not
  `{zoneId, key, value}` as in `api-reference.md`). Equipment types are limited to light, shutter,
  sensor, switch (a plug is a `switch`), thermostat and button. mini-Sowel is a repository separate
  from Sowel; what it needs on the Sowel side is an ordinary Sowel issue.
- **Why**: the same contract is what lets `sowel-plugin-mini` show the box's equipments in Sowel,
  and lets Sowel's recipes and components be ported without translation.

## D4: Design system

- **Date**: 2026-11-20
- **Choice**: Sowel's design system (tokens, Tailwind theme, design rules, components), copied
  from the Sowel repository and adapted to the box: system fonts instead of Inter and JetBrains
  Mono, Preact instead of React, Lucide icons imported by name only, a first-load budget of
  170 KB gzip. The starter kit's design system is not used. Details: `docs/design-system.md`.
- **Why**: mini-Sowel is part of the Sowel family and becomes part of Sowel's interface when
  linked; the kit's look is unrelated. The box serves its pages from flash: the budget and the
  fonts follow from the framing note (100 to 170 KB gzip, system fonts).

## D5: Trajectory and hardware

- **Date**: 2026-11-20
- **Choice**: a personal open-source project with the door to a product left open. The hardware
  is the Sonoff Dongle Max (ESP32-D0WDR2-V3, EFR32MG24, 16 MB flash, 2 MB PSRAM, Ethernet PoE,
  Wi-Fi, USB); only its firmware is replaced. The product track (ESP32-S3 board, secure boot,
  backup keys, OEM, CRA) is out of the current trajectory.
- **Why**: the framing note left "personal or product from the start" open; the user chose
  personal first. The classic ESP32 holds a single key digest (no backup keys): accepted for a
  personal project, a known limit for a product.

## D6: Development hardware stays on its original firmware

- **Date**: 2026-11-20
- **Choice**: the firmware is built for the ESP32 from the start, but the Dongle Max is not
  flashed until the user decides it; its original firmware exposes the radio over TCP, which the
  host program uses for development.
- **Why**: flashing the dongle removes the TCP radio the first development steps rely on.

## D7: License

- **Date**: 2026-11-20
- **Choice**: none chosen at setup; the Cargo manifests carry no `license` field. The choice is in
  the roadmap inbox (Sowel is AGPL-3.0).
- **Why**: the license is the author's decision, and it weighs on the product door left open in
  D5.

## D8: License (replaces D7)

- **Date**: 2026-11-20
- **Choice**: GNU Affero General Public License v3.0 only (`AGPL-3.0-only`), the same as Sowel:
  `LICENSE` at the root, `license` in every Cargo manifest and in `ui/package.json`.
- **Why**: mini-Sowel is part of the Sowel family and reuses Sowel's code (design system,
  components, recipes); one license keeps that reuse simple. As the author holds the copyright,
  the product door of D5 stays open (a separate license can be granted for a product).

## D9: Remote and CI (replaces the remote part of D2)

- **Date**: 2026-11-20
- **Choice**: remote `origin` on GitHub, `https://github.com/mchacher/mini-sowel.git`, beside
  Sowel. CI: GitHub Actions, `.github/workflows/ci.yml`, runs `sh scripts/validate.sh` on every
  pull request and on every push to `main`, with the Rust version of `rust-toolchain.toml`, Node
  24, and the ESP32 toolchain (espup v0.19.0, ldproxy); ESP-IDF's download is cached. Pull
  requests are made with `gh pr create`; merges stay squash merges after the user's yes.
- **Why**: a backup and a public home for the open-source project (D5), and a CI that checks
  every change, the firmware build included, on a machine other than the author's.

## D10: Node tools in `tools/`

- **Date**: 2026-11-20
- **Choice**: the Node tools of D1 live in `tools/<name>/`, each with its own `package.json` and
  lock file, outside the interface; TypeScript run directly by Node 24 (type stripping, no build
  step), checked by `tsc` and `node:test`, formatted by Prettier with the interface's settings,
  and part of `sh scripts/validate.sh` and of the CI. A library a tool measures or exports is
  pinned to an exact version. The first one is `tools/z2m-coverage` (spec 002).
- **Why**: a tool's dependencies (Zigbee2MQTT's library, about 5 000 definitions) must not weigh
  on the interface's packages or budget; Node runs TypeScript the user already knows without a
  build step; pinning makes a measure repeatable.

## D11: Unsafe code in the firmware's adapters only

- **Date**: 2026-11-20
- **Choice**: the core and the host forbid `unsafe` code. The firmware denies it
  (`unsafe_code = "deny"` in `firmware/Cargo.toml`, instead of `forbid`), and allows it only on
  the small functions that call ESP-IDF's C functions with no safe wrapper in `esp-idf-svc` (the
  first: reading the heap sizes, spec 003), each with a `SAFETY` comment saying why the call is
  sound. The firmware targets ESP32 chips of revision 3 and later (`CONFIG_ESP32_REV_MIN_3`).
- **Why**: Rust cannot check C code, so every call into ESP-IDF's C API is `unsafe`; `forbid`
  cannot be lifted even on one function. Keeping such calls few, small and commented keeps the
  rest of the firmware checked by the compiler. Below revision 3 the PSRAM needs a workaround the
  C compiler applies and Rust code would not get; the Dongle Max's ESP32-D0WDR2-V3 is revision 3.

## D12: Small crates in the host when std cannot do it

- **Date**: 2026-11-21
- **Choice**: the host may use a small, widely used crate when the standard library cannot do
  the job portably; the first is `ctrlc` (spec 005), so that Ctrl-C during pairing closes the
  radio's joining. Randomness is read from the system (`/dev/urandom`) without a crate. The core
  stays free of operating-system crates; it uses `log` only.
- **Why**: leaving joining open on the radio after the user stopped the host would let a device
  join a network nobody watches; std has no signal handling. Each crate added is named in a
  spec and its review.

## D13: The device API on the Mac, and its divergences from Sowel

- **Date**: 2026-11-21
- **Choice**: the host serves Sowel's device routes (spec 008) on 127.0.0.1 only, without
  authentication, with a `Host` header check; Sowel's bearer token comes with the box's
  commissioning password (milestone E). The routing and the JSON live in the core, so the box
  reuses them; the host uses the `tiny_http` crate (the user's choice; it widens D12, which
  allowed a crate only where std cannot do the job). The core gains `serde` and `serde_json`
  (CLAUDE.md names them for JSON). Divergences from Sowel, kept on purpose: devices in pairing
  order (Sowel: by name), data in a fixed order (Sowel: by key), dates with `T` and milliseconds
  (Sowel's SQLite: a space, no milliseconds), data ids random (Sowel: derived), a stricter `PUT`,
  405 and 413 with `{"error"}` bodies, `zoneId` `null` until zones exist, no `rawExpose`. Values
  are written to disk every 10 minutes and on stop, definitions at once (the user's choice).
- **Why**: the Mac is a development host, reached from the same machine; a token without a
  commissioning flow would be a fake guard. The box's flash must last: writing every measure
  would wear it out within months. The divergences do not change what a Sowel client reads (the
  same fields, types and values).

## D14: Zones and equipments, and their divergences from Sowel (replaces D13's "zoneId null")

- **Date**: 2026-11-21
- **Choice**: zones, equipments, bindings and the zone aggregation follow Sowel's types, routes,
  messages and rules, read from Sowel's code (spec 010). Devices now take a zone, so D13's "`zoneId`
  `null` until zones exist" no longer holds. Divergences kept on purpose: only the six equipment
  kinds of D3; no computed data, timed actions, energy flags, zone orders or reorder; bounds (32
  zones, 64 equipments, 16 bindings) and lengths (names 64 characters, Sowel 100); `PUT /devices`
  checks the zone (404) and deleting a zone returns its devices to no zone (Sowel does neither);
  `POST /equipments` always returns the details; validation errors as `{"error"}` texts; unknown
  fields refused on zone and binding routes, ignored on equipment routes (Sowel's clients send
  extra flags).
- **Why**: the box has bounded memory, and a Sowel client must read the same fields, values and
  rules; the stricter checks only refuse what would leave dangling references.

## D15: Long lists and files streamed in chunks

- **Date**: 2026-11-21
- **Choice**: the long API lists (`GET /devices`, `GET /equipments`) and the store's files are
  produced by a cursor in the core, a few items at a time (at most 4), each chunk under the lock
  and written out with the lock released (HTTP chunked transfer on the host; the box's HTTP server
  sends chunks the same way). The memory ceiling stays 200 KB for a realistic home (50 devices,
  10 zones, 50 equipments), model and largest response in flight included; measured 269 KB with
  whole responses, 150 KB with chunks, 124 KB once the model stopped storing what it can derive
  (parent ids, constant fields, shared aliases): the model itself went from 132 KB to 103 KB;
  128 KB with chunks of 4 items and the aggregation's index, so four slow clients holding a chunk
  each add about 48 KB.
- **Why**: building a whole response held the model plus a 143 KB string, and up to four requests
  run at once, so the peak grew with the home and with the clients; on the box's 2 MB of PSRAM,
  shared with WebSocket clients, recipes, TLS and OTA, that growth would end in a restart, against
  the true north (a month without intervention). Discussed with the user, who chose it.

## D16: Orders, and their divergences from Sowel

- **Date**: 2026-11-21
- **Choice**: equipment orders follow Sowel's route, messages and value rules (spec 011), and go
  through a queue the radio loop serves, so HTTP never touches the radio. Divergences kept on
  purpose: no order outcome events yet; a value Sowel would pass to its integration unchecked is
  refused here (400); every order is answered within 12 s and dropped, never sent late, once its
  deadline passed; a retry only for a missing answer or a busy radio; brightness 255 counts as
  254; after an order the state is read back from the device instead of being set from the order
  (Zigbee2MQTT sets it optimistically). At most 2 orders at once (one being sent, one waiting), so reads keep being served.
- **Why**: a light that switches after the client was told it failed is worse than no answer; the
  ZBMINIR2 is not set up for reporting by Zigbee2MQTT, so its state must be read; the box serves
  few HTTP clients at once and must stay readable while orders wait.

## D17: Recipes in native Rust first, and motion-light's divergences from Sowel

- **Date**: 2026-11-21
- **Choice**: until milestone F (recipes loaded at run time, `wasmi`, D1), recipes are written in
  Rust in the core, with Sowel's id, slots, messages, behaviour and routes; `motion-light` 1.1.2
  first, its quirks kept as Sowel has them and filed as Sowel issues. Divergences: recipes read
  compact, filtered engine events (not Sowel's full events); the zone aggregate is recomputed once
  per batch of measures, not per measure; orders from the API go before recipe orders, and the
  limit of 2 orders at once (D16) counts orders from the API only, recipe orders having their own
  limit (32); at most 16 instances, 16 lights and 16 buttons each, 50 log lines of at most 200
  bytes; logs and state are not kept across a restart; no 24.8-day timer limit; numbers Sowel
  would pass through (negative or fractional durations) are clamped; the body error text is
  `Invalid body`; `isDaylight` is always `null`; no `recipe.instance.*` events yet.
- **Why**: the milestone's demo needs one recipe now, and WebAssembly is a milestone of its own;
  the user is Sowel's author, so a quirk is fixed in Sowel first; the bounds keep the box's memory
  known; a person waiting at the API goes before an automation.

## D18: Live events, and their divergences from Sowel

- **Date**: 2026-11-21
- **Choice**: `GET /ws` follows Sowel's WebSocket (spec 013): the `bearer.` subprotocol echoed,
  subscribe by topics, arrays every 200 ms, Sowel's dedup and event fields. It replaces two
  points of D17: `recipe.instance.*` events are sent, and an order carries its source (the
  recipe) to its `equipment.order.executed` or `equipment.order.failed` event (D16's outcome
  events beyond these two stay unsent). Divergences: at most 4 clients (503); incoming messages
  of at most 1 KB (1009); a client's queue bounded (256 events, 8 KB), closed 1013 past it;
  pings every 30 s, a client silent for 60 s or unwritable for 5 s dropped; the token not
  checked (D13); the interface's development server accepted as an origin; not sent:
  `device.discovered` (no pairing while serving), `device.heartbeat`,
  `equipment.order.outcome|unconfirmed`, `equipment.timed_action.*`, `system.*`, `mode.*`,
  `activity.*`, and the optional `sourceTimestamp` and `startedAt`; deleting a device sends no
  `equipment.updated` for the bindings it drops; the aggregate is also recomputed every 60 s
  without a change (staleness), which can give the recipes a zone event too. Three small crates join the
  host (D12): `tungstenite` for the frames, `socket2` for the socket's buffer, `httparse` for a
  front door that keeps `/ws` on sockets the host owns (with their timeouts) and passes every
  other request to `tiny_http` on a loopback port: `tiny_http` cannot give an upgraded socket a
  timeout, and a listener timeout stops its accept loop on Linux. The box will use its own server
  (E), so this stays on the Mac.
- **Why**: Sowel's interface works unchanged against it; the bounds keep the box's memory known
  and a sleeping laptop from holding a slot for good; the host has no async runtime.

## D19: The interface ports Sowel's leaf components, not its pages

- **Date**: 2026-11-21
- **Choice**: the interface (spec 014) ports Sowel's small components and stores unchanged where
  possible (light control, sensor values, zone pills and lights line, status badge, the zone,
  equipment and aggregation stores, the API client) and writes its own page, WebSocket store and
  `t()` (French and English, Sowel's keys, `react-i18next` aliased to it); no router (the room in
  the address's hash); the host serves `ui/dist` from disk on the Mac. Target: first load at most
  80 KB gzip.
- **Why**: Sowel's home page pulls about 35 modules, an 11-store WebSocket, i18next with 67 KB of
  locales and a router, over the 170 KB budget; redrawing would break "ported, not redrawn" (D4);
  the box will embed the files (E).

