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

## D3: Sowel contract (equipment types: replaced by D43)

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
- **Choice**: remote `origin` on GitHub, `https://github.com/mchacher/solite.git` (renamed from mini-sowel on 2026-11-21), beside
  Sowel. CI: GitHub Actions, `.github/workflows/ci.yml`, runs `sh scripts/validate.sh` (cut on 2026-11-21 into three parallel jobs, `--only rust`,
  `--only ui`, `--only firmware`, so the slowest sets the time) on every
  pull request and on every push to `main` (without the slow checks, which run every night: D34), with the Rust version of `rust-toolchain.toml`, Node
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

## D14: Zones and equipments, and their divergences from Sowel (replaces D13's "zoneId null"; types and optional fields: replaced by D43)

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
  `device.heartbeat`,
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

## D20: The engine's memory ceiling is 300 KB for a realistic home

- **Date**: 2026-11-21
- **Choice**: the ceiling test of a realistic home (50 devices, 10 zones, 50 equipments, 3 recipe
  instances with full logs, a live client with a full queue, the largest answer in flight) allows
  300 KB, measured on the Mac; it replaces D15's 200 KB. Every optimisation found stays in the
  roadmap's inbox, and the figure is read again on the box (QEMU, then the Dongle Max).
- **Why**: milestone B brought the engine to 187 KB (198 KB in the worst case), with no room for
  the next feature. An estimate of the whole box (2026-11-21) leaves about 1.4 MB of PSRAM free in
  the likely case: the engine can take 300 KB, while the risks lie in the internal SRAM, the
  WebAssembly recipes and bounds no test checks together. Approved by the user.


## D21: The tool is named Solite

- **Date**: 2026-11-21
- **Choice**: the tool, until then mini-Sowel, is named **Solite**, pronounced "so-light": Sowel
  is "so well", Solite is "so light", the light version of Sowel that runs on its own (and its
  first recipe switches a light). Crates `solite-core`, `solite-host`, `solite-sim`,
  `solite-firmware`; skills `/solite-*`; the logo is Sowel's smiling house with three short rays
  above the roof, in the logo's own blues. A radio variant, if one comes, is "Solite Thread", not
  a new name. Earlier specs and decisions keep the old name: they are history.
- **Why**: "mini-Sowel" only said "a smaller Sowel"; Solite keeps Sowel's play on words and
  says what the box is. Checked on 2026-11-21: the crates are free; the common domains are taken
  (the site goes under Sowel's); trademarks named SOLITE exist in the United States in other
  classes (luggage, building materials; lighting abandoned or cancelled): a search in TMview and
  WIPO, classes 9 and 42, comes before a public release. Chosen by the user.

## D22: Solite's logo is Sowel's revised house with an antenna

- **Date**: 2026-11-21
- **Choice**: the logo is Sowel's revised mark (Sowel spec 210, commit `5da84dc1`: the house, the
  face and the wink, in its two weights) with an antenna on the roof, a ball and one radio wave on
  each side, and the lowercase wordmark "solite" in Nunito 700, outlined as paths. It replaces
  D21's "three short rays above the roof".
- **Why**: chosen by the user among five concepts: the antenna says what the box is (a small
  radio box) while the house and its smile stay Sowel's; outlined glyphs keep web fonts off the
  box. Nunito is under the SIL Open Font License (`ui/src/lib/NUNITO-OFL.txt`); the glyphs are
  outlined again from Google Fonts' file before a public release.

## D23: The worst case has its own ceiling of 1 MB, and small allocations go to internal RAM first

- **Date**: 2026-11-21
- **Choice**: a second test, `memory_worst_case.rs`, builds a home with every bound of the core at
  its maximum and keeps its peak under 1 MB (measured 987 KB with every recipe log entry at 200
  bytes); D20's 300 KB stays the ceiling of the realistic home, the two never added. A test fails
  when a `pub const MAX_*` of the core is missing from its header. In the firmware,
  `SPIRAM_MALLOC_ALWAYSINTERNAL=4096` keeps allocations of up to 4 KB in internal RAM,
  `SPIRAM_MALLOC_RESERVE_INTERNAL=65536` keeps 64 KB of it for the network and DMA, and mbedTLS
  takes its buffers from PSRAM.
- **Why**: the realistic home says nothing about the day every bound is reached, and a bound added
  later must not escape the budget. The placement is ESP-IDF's usual one for a Wi-Fi application
  with PSRAM; the budget (`docs/memory-budget.md`) shows it fits on paper, with the internal RAM
  as the tight resource (most of the engine's bytes are small allocations). The figures are
  estimates until the box measures them, as D20's are. Approved by the user.

## D24: One network link, chosen at boot, Wi-Fi first

- **Date**: 2026-11-21
- **Choice**: one image holds Wi-Fi and Ethernet, and only one link is active, chosen once at
  boot from the setting `{"link": "auto" | "wifi" | "ethernet"}` and the cable: `auto` (the
  default, and the fallback for a missing or wrong setting) means Ethernet with a cable at boot,
  else Wi-Fi; nothing switches while running. The choice is a function of the core
  (`network::choose`); the firmware starts the link. Values are case-sensitive, as Sowel's enums.
  Wi-Fi is built first.
- **Why**: two links at once would add their memory (the budget takes the larger, Wi-Fi), and a
  switch while running is a source of states nobody tests. Wi-Fi first because QEMU has no Wi-Fi
  to test against and the box needs one to be reachable without a cable. Approved by the user.

## D25: The partition table has two application slots and a data partition

- **Date**: 2026-11-21
- **Choice**: `firmware/partitions.csv`, 16 MB, no factory slot: `nvs` 24 KB, `otadata` 8 KB,
  `phy_init` 4 KB, `ota_0` and `ota_1` of 4 MB each, `storage` (FAT with wear levelling, built
  into ESP-IDF) of about 7.9 MB. With `otadata` empty the bootloader starts `ota_0`. The table is
  not set in `sdkconfig.defaults`: `esp-idf-sys` ignores a custom table there (its README says to
  leave `CONFIG_PARTITION_TABLE_*` out), so the table reaches the box through `espflash
  --partition-table` and QEMU through `scripts/qemu.sh`, which builds its image from the same
  file and places the application at the offset of `ota_0` that the built table gives.
- **Why**: two slots allow an update that can fall back, and a data partition holds the home's
  files. The slots are 4 MB because the firmware is about 1 to 2 MB and the interface and
  recipes need room. FAT rather than LittleFS: it is in ESP-IDF, with no extra component. The
  table is cheap to change before the first flash and needs a full re-flash after. Chosen in
  autonomy, to be confirmed by the user at the demo.

## D26: The engine's loop is in the core, and the simulator is built into the QEMU image

- **Date**: 2026-11-21
- **Choice**: the radio loop and the saver of the host move to `crates/core/src/engine/`, generic
  over the radio transport, a `Storage` trait (the engine's four named files, written chunk by
  chunk, D15), clock closures, a random closure and an output closure; the host (files in a
  folder) and the firmware (the FAT partition) implement `Storage`. `crates/sim` builds for the
  ESP32: the firmware has a cargo feature `sim`, which embeds the simulator, the realistic home
  of D20 and its scenario (`firmware/fixtures/`) and runs the engine on them in a thread of 32 KB
  of stack; without it the image is the box's and starts no radio. This replaces spec 007's "the
  simulator is never built for the ESP32": it is built for the image QEMU runs, not for the box.
  The engine's heap peak and the internal RAM minimum are read by the firmware with ESP-IDF's
  local-minimum monitor of the free heap, phase by phase (load the home, build the simulator,
  run), and `scripts/qemu.sh` fails above 350 KB (D27, first 300 KB of D20) or below 64 KB of internal RAM free
  (spec 015's reserve).
- **Why**: the loop had to run on the box before the box is flashed, and moving it keeps one loop
  for both programs (the host's tests are its safety net). D20 said the 300 KB is "read again on
  the box (QEMU, then the Dongle Max)": the QEMU figure is an upper bound, with the simulator
  inside, and the box measures it again in E3. The monitor's figures are the sum of the low
  watermarks of the internal RAM and the PSRAM, which may have been reached at different
  moments: over the whole run the sum gave 329 KB for the first run, phase by phase 287 KB; both
  readings are upper bounds of the true peak, the per-phase one the tighter and never below it,
  so the gate uses the phases. A bound holds for what the run exercised: the realistic home,
  the scenario of `firmware/fixtures/` (motion, light orders), no network. Final run: 312 KB
  by phases, heap 170 KB at the end. `SPIRAM_MALLOC_ALWAYSINTERNAL` was
  tried at 512 as the first remedy (D23's knob): the total heap does not shrink when allocations
  move between the two memories, and the peak read 359 KB (whole-run sum), so it stays at 4096.
  What the peak holds: the 32 KB stack, the home loaded from its files (text and parsed data,
  about 260 KB at the highest), then the simulator; the loading is the largest transient and
  is the first optimisation to make if the ceiling is crossed.

## D27: The engine's ceiling is 350 KB in QEMU, replacing D20's 300 KB as the gate

- **Date**: 2026-11-21
- **Choice**: the heap peak of the engine in QEMU (`scripts/qemu.sh`) may reach 350 KB; D20's
  300 KB was the gate until then. Approved by the user on 2026-11-21, pending optimisation: the
  inbox of `docs/roadmap.md` has the line to bring the engine back under 300 KB, loading the
  home by streaming first. The Mac test (`memory_ceiling.rs`: 182 KB, 194 KB with every log full)
  stays at 300 KB, and D23's 1 MB for the worst case is unchanged.
- **Why**: once the scenario plays at its own pace, with motion and light orders (spec 016
  review), the QEMU run reads 312 KB by phases: the second phase (the recipes' start and the
  simulator's build) is the largest at 312 KB, the load of the home 258 KB, the run 197 KB, the
  identification 186 KB. Both readings of the monitor (whole run, per phase) are upper bounds, the
  per-phase one the tighter, and the figure holds for what the run exercised. Internal RAM stays
  at 107 KB free at the lowest, above the 64 KB floor. The measured run differs from the box: its
  saves are dropped (the last one written to flash), the simulator's memory is kept in RAM, and the
  scenario is clocked by its waits, so no File buffer or wear-levelling write is in the figure.

## D28: Pairing while serving

- **Date**: 2026-11-22
- **Choice**: pairing opens and closes while serving (spec 017), through
  `GET` and `POST /api/v1/integrations/solite/permit-join`, under Sowel's integration namespace
  (Sowel has no pairing route: in Sowel, Zigbee2MQTT pairs). The body takes Zigbee2MQTT's `time`
  (seconds, 0 closes, 180 when missing); `POST` answers 202 with the state asked for
  (`permitJoin`, `remaining`, and `error` when the radio refused the last opening), the radio
  loop applies it at its next turn. One window at a time; a new `POST` restarts it. No live event
  for the window (only Sowel's events, D18: the interface reads the state with `GET`). `--serve`
  starts the network as `--pair` does, forming one on a radio that has none, so a fresh box or
  simulator is set up from the browser alone; D18's `device.discovered` is sent. The window is in
  the core (`home/pairing.rs`, `engine::run`), so the box gets it with the API it serves.
  **Constraint**: the route has no authentication (D13): on the box (E3) it stays disabled
  (404) until the password of J4 exists.
- **Why**: J1 of the roadmap: add a device from the interface with no terminal, where `--pair`
  could not run together with `--serve`. The route under `/integrations/solite` follows Sowel's
  grouping of what an integration does; 202 with the state asked for avoids a new wait mechanism
  for a rare action whose refusal is read with `GET`.

## D29: Equipments from the interface

- **Date**: 2026-11-22
- **Choice**: Sowel's zone and equipment screens are ported (spec 018) with D3's seven types and
  without bindings edited one by one. The equipment form creates the equipment and its bindings in
  one `POST /api/v1/equipments` with `deviceIds` (alias = data key, the core's auto-binding of
  spec 010), not as Sowel's interface does, which computes a binding plan in the browser
  (`autoCreateBindings`, `bindingUtils.ts`) and sends one request per binding. The zone form has
  no "None" parent: Solite's tree has one root (spec 010 R7.10), and `parentId: null` means the
  root on `POST` and is refused on `PUT`.
- **Why**: J2 of the roadmap: the garage and its equipments made with the mouse. One request is
  atomic (no half-made equipment when a request fails) and runs the core's tested rules; the
  browser-side plan is over 800 lines of Sowel code the box's interface has no room for (170 KB
  budget).

## D30: The equipment is the entry point

- **Date**: 2026-11-22
- **Choice**: replaces D29's way of binding (D29's screens and zone form stay). The interface never
  gives a device a room (`Device.zoneId` is kept in the API for Sowel's format, D3, and read by
  nothing). The equipment form needs at least one device, since Solite has no equipment page to
  complete one. It binds as Sowel's interface does, from a plan computed in the browser (Sowel's
  `bindingUtils.ts` and `binding-candidates.ts`, ported for D3's seven types) and one request per
  binding, the equipment deleted again if one is refused. `POST /equipments` with `deviceIds` keeps
  Sowel's `createWithAutoBindings` (every key) for the API's clients. Sowel's Administration section
  is ported, with a Discovery screen per integration in the place of Sowel's Integrations entry.
- **Why**: Sowel's model (`equipments.md`, "Equipment vs Device"); the plan sits beside the selector
  that uses the same rule (Sowel's architecture: one module decides compatibility and bindings); the
  core keeps Sowel's route as Sowel's server has it, and the realistic home of D20, its firmware
  fixtures and memory figures are untouched. **D29's two arguments, reversed**: (1) "the browser-side
  plan is over 800 lines of Sowel code the box's interface has no room for": only the part for D3's
  seven types is ported (about 250 lines, about 5 KB gzip), and the first load stays far under the 170 KB
  budget (51.8 KB gzip, checked with `npm run check:bundle`, against a gate of 60 KB for this spec);
  (2) "one request is atomic": atomicity is given up for Sowel's bindings; a refused binding deletes the
  equipment again, and the half-made equipment is visible for the time of the requests (as in Sowel).
  The equipment made by the old way, every key bound, was wrong for the type, which outweighs atomicity.
  **Risk on the box**: one creation sends up to 33 sequential requests (1 + 16 + 16), one connection
  each, to an ESP32 that answers 503 `Busy` past 4 requests at once; sequential requests stay under it,
  but neither the Mac tests nor QEMU measure the box's time or connection behaviour; the hardware demo
  (from the Mac) and E3 on the box are the checks.

## D31: The simulator runs on the box in a measuring image; esptool replaces espflash

- **Date**: 2026-11-22
- **Choice**: replaces the part of D26 that kept the simulator out of the box ("not for the box") and
  the part of D25 that gave the partition table to the box "through `espflash --partition-table`".
  The simulator now also runs on the Dongle Max, in a third image, the measuring image (cargo feature
  `flash-saves`, which turns on `sim`): the realistic home with every save written to the flash and
  timed, to measure on the real ESP32 what QEMU could only bound. The box's own image never holds the
  simulator. Every write to the box goes through `sh scripts/box.sh`, which uses `esptool.py` for each
  step (identify, backup, restore, flash) and refuses to flash without a backup that was restored once;
  espflash is dropped, and the `runner` line of `firmware/.cargo/config.toml` that flashed with it is
  removed, so `cargo run` in `firmware/` flashes nothing. The EFR32 radio stays wired but not driven
  (no file of `firmware/src/` names GPIO12 or GPIO15).
  The prompt of `box.sh` reads the end of the box's MAC from `/dev/tty`: that stops a pipe and a
  script's plain standard input, not a pseudo-terminal. The guarantee is the user's explicit yes
  in the conversation plus the MAC typed, not the terminal alone.
- **Why**: QEMU's flash takes a quarter of an hour for the saves the box does in seconds, and says
  nothing of the box's timing; a measuring image answers (free memory at boot, the engine's peak, the
  FAT partition, the save stall) without waiting for the radio's serial link. One tool to fake in the
  tests, the same image parts as `scripts/qemu.sh` (bootloader, table from `partitions.csv`, `ota_0`
  offset), and a runner that skipped every check was a write path to the box (D6).

## D32: The radio over the UART in a radio image, pairing at boot

- **Date**: 2026-11-22
- **Choice**: a fourth image (feature `radio`) drives the EFR32 over UART1 (TX GPIO17, RX GPIO13,
  8N1, no flow control), a 4 KB receive ring and the interrupt in internal RAM; the radio and the
  engine share one thread on core 1 with 32 KB of stack. The interrupt setting is in
  `sdkconfig.radio.defaults`, built in `target/radio`, so the other images keep their settings.
  The speed is probed at each boot, the fixed `RADIO_BAUD` first, then 115200, 460800, 921600.
  The core describes the pins by role (reset active low, bootloader active high, never set
  active) and the reset sequence; the firmware runs it after the ESP32's boot, each level written
  before the pin becomes an output, GPIO12 low before GPIO15 (both strapping pins). GPIO12 and
  GPIO15 are named in `firmware/src/radio_uart.rs` only (a test). Pairing opens for 180 s at a
  power-on boot when the marker `/data/booting` (removed after 10 minutes of running) is absent: a
  crash, a watchdog, a brown-out or a boot loop opens nothing. The Zigbee backup is
  `/data/zigbee.json`, the network and trust-centre keys in plain text on a flash that is not
  encrypted, accepted until flash encryption is decided. Only a radio error restarts the radio
  (reset, probe, resume, never a forming nor a window), at most 3 times in 10 minutes; a storage
  error stops the loop. The radio image is a development image, in no public release before J4.
  Replaces D31's "the EFR32 radio stays wired but not driven" and D26's "without it the image is
  the box's and starts no radio" for the radio image.
- **Why**: the simplest bounded way to pair without a network (a console command shares the log's
  port, the button's pull-up is unknown); the probe finds a radio at another speed for one try's
  cost; the ring holds more than the radio's ASH window, and the interrupt keeps the FIFO (11 ms at
  115200) drained while a flash write stalls the caches; logic in the core is tested on the Mac.

## D33: The radio image serves the API on the local network, over Wi-Fi

- **Date**: 2026-11-22
- **Choice**: the radio image (feature `radio`) joins the Wi-Fi network of `/data/wifi.json`
  (written by `box.sh wifi` through the firmware's serial console; never NVS, never git) and serves
  Sowel's API, `/ws` and the interface on port 80, with no authentication. A `Host` is
  accepted when it is a private IPv4 address (10/8, 172.16/12, 192.168/16, 127/8), `localhost` or
  the box's name `solite-<6 hex>` (alone or with `.local`, `.lan`, `.home`, `.home.arpa`); the
  pairing route answers 404 (D28). ESP-IDF's HTTP server through `esp-idf-svc`, 4 workers by its
  asynchronous requests (their stacks and 16 KB body buffers in PSRAM), at most 12 connections in
  a socket table of 16, TCP keep-alive; on `/ws` the refusals are close
  codes after the upgrade (4003, 1013), the development origins of `live::session::origin_allowed`
  (`http://localhost:5173`, `http://127.0.0.1:5173`) are accepted as the host does (decided by the
  user, 2026-11-22: a development page on a machine of the home's network can open the live events),
  only `bearer.none` is echoed, and a dead client is found by
  TCP keep-alive instead of 60 s of silence; a client that cannot be written to holds the server's
  task up to 5 s. The interface is embedded at build time, gzip only, from flash. Wi-Fi power save
  off; SNTP with `pool.ntp.org`; the engine's saves wait at most 20 s for the clock, the radio and
  the pairing window of a power-on do not. `flate2` is in the firmware's build (never its code).
  The setting `ethernet` starts no link in this image. Built in two specs: the rules in the core,
  the packing and the scripts (spec 022), the firmware (spec 023).
- **Why**: the true north's box is reached from the home's network; the radio image is a
  development image, in no public release (D32), so an open API on the local network is
  accepted, also while the box is the family's annex controller (the user, 2026-11-22), on two
  conditions: the box stays on the home's own network (not a guest network), and no router port is
  forwarded to it. The guards are the Mac host's (Host check against DNS rebinding, JSON-only
  bodies against forms, Origin on `/ws`), and the pairing route stays off. The settings file
  survives a flash with `--keep-storage`, which NVS would not.

## D34: The slow checks run every night and on the Mac, not on each pull request

- **Date**: 2026-11-22
- **Choice**: the slow checks are the Rust tests marked `#[ignore = "slow ..."]` (each waits a
  production delay: an order's 12 s deadline, the 10 s wait for the network, the 10 s limit of
  `measure-api.sh`, a minute of the garage at speed 10) and the engine's run in QEMU. No test is
  ignored for another reason. `sh scripts/validate.sh` runs them (`cargo test -- --include-ignored`
  and `scripts/qemu.sh`), so the Mac has the full verdict;
  `--skip-slow` leaves them out, and Claude's commit hook runs it. The CI (`.github/workflows/ci.yml`, D9) runs `--skip-slow` on
  pull requests and pushes to `main`, and the full verdict every night at 02:00 UTC on `main` and
  by hand (Run workflow). The git pre-commit hook stays `--fast`. The firmware job caches the Xtensa
  toolchain espup installs, keyed by espup's version and `firmware/rust-toolchain.toml`. The tests
  of `serve.rs` and `ws.rs` pair the garage once per file and copy its folder into each test.
- **Why**: the pull-request CI took about 8 minutes, most of it waiting (real pairing windows,
  production timeouts, QEMU's run); the user chose (2026-11-22) a fast verdict on each pull
  request and the slow checks every night and on the Mac, where each one still runs.

## D35: The backup of the original firmware is optional for `flash`

- **Date**: 2026-11-22
- **Choice**: replaces, for `box.sh flash`, the mandatory backup and the MAC confirmation that
  followed D6 (spec 020, D31's "refuses to flash without a backup that was restored once").
  `flash` takes `--backup <file>` as an option: a backup given is still checked (16 MB, the name
  holds this box's MAC, its SHA-256 in a row of `docs/hardware.md`'s backup table with a restore
  date); without it the plan says the way back is Sonoff's own reflash. `flash` asks no question
  beyond the command itself; `backup` and `restore` keep the question of the MAC's last 6 hex
  digits. D6's rule that the box is not flashed without the user's yes stays, as a rule for Claude
  in `CLAUDE.md`.
- **Why**: the user does not need the original firmware's backup: Sonoff supplies its own reflash
  (its web flasher, or its Home Assistant add-on), which brings the box back to its original
  firmware. The backup and its proven restore stay available for whoever wants them.

## D36: The pairing window also opens from the serial console, never from the network

- **Date**: 2026-11-22
- **Choice**: the radio image's serial console (spec 022 R9) takes a third line, `pair`, sent by
  `sh scripts/box.sh pair` over the USB cable: the engine opens the pairing window for 180 s
  (`PAIR_AT_BOOT_SECONDS`), whatever the boot was and whether the marker `/data/booting` is there,
  and logs `pairing: open for 180 s` (or `pairing: failed: <reason>`). It completes D32's window
  at a power-on boot, which stays as it is, and keeps D28's rule for the box: the pairing route
  answers 404 on the network until J4's password. D32's "a console command shares the log's port"
  is no longer a reason against it: the console of D33 already reads that port. The window still
  opens only from what a person does by hand at the box (power, or the cable), never from the
  local network.
- **Why**: on the real box (2026-11-22), pairing the ZBMINIR2 needed a power-on more than 10
  minutes after the previous boot: opening the serial port resets the box and, within 10 minutes,
  the marker refuses the window; a software restart opens none. A line on the cable that already
  carries the Wi-Fi settings opens it at once, with no new input path and no exposure on the
  network (the API has no authentication, D13).

## D37: `/ws` is registered directly with ESP-IDF on the box

- **Date**: 2026-11-22
- **Choice**: the radio image registers `/ws` itself with `httpd_register_uri_handler`
  (`is_websocket`, the subprotocol `bearer.none`, `ws_post_handshake_cb`), with
  `CONFIG_HTTPD_WS_POST_HANDSHAKE_CB_SUPPORT=y` in `sdkconfig.radio.defaults` (checked by
  `validate.sh`), instead of `esp-idf-svc`'s `ws_handler`. The post-handshake callback, on the
  handshake's request, reads `Host` and `Origin`, takes a slot of the core's `LiveHub` or closes
  (4003, 4003, 1013), sets a session context released by ESP-IDF when it deletes the socket (which
  frees the slot) and sends the welcome. The live thread hands each frame to the server's task
  with `httpd_queue_work` and `httpd_ws_send_frame_async`, and waits for its result; a frame for a
  session already closed is not sent (its socket number may belong to a new connection), and a
  close asked by the live thread is a shutdown of the socket run on the server's task, for a
  session still open. The rest
  of the server stays `esp-idf-svc`'s. D33's other `/ws` rules are unchanged.
- **Why**: with ESP-IDF 5.5, the WebSocket handshake never calls the URI handler, only the frames
  that follow do, and `esp-idf-svc` 0.53 builds its session (its `New` case) on the handshake's
  call: on the box (2026-11-22) a browser got its 101 and then nothing, no welcome and no event.
  The post-handshake callback is ESP-IDF's own place for that work, on the request that still holds
  the headers.

## D38: A recipe settles when what it uses changes

- **Date**: 2026-11-23 (spec 024, decided in an autonomous run, for the user's review)
- **Choice**: right after an equipment is deleted or changed, a binding removed, a device removed
  or a zone deleted, the core settles the recipe instances: a deleted equipment's id leaves the
  settings of every instance that names it (warning line); every enabled instance is validated
  again and, when it fails, stopped and **disabled** (`Disabled: <reason>`, events `stopped` and
  `error`); a running one whose settings lost an id restarts with them; the instances of a deleted
  zone are deleted. `enable` refuses (400 `Invalid params: <reason>`) an instance that does not
  validate. It diverges from Sowel, which keeps the stale ids, lets the instance fail on each order
  and enables an instance that cannot start.
- **Why**: the box runs for a month with nobody reading its logs; an instance that looks enabled
  but sends orders to an equipment that is gone is a state the interface cannot show. Disabled
  with its reason, it shows as a switch off and a line in its journal, and it is never restarted
  in error at each boot. A deleted zone's instance could never be valid again (a new room has a
  new id) and would be shown nowhere, so it goes with its zone. The settle walks at most 16
  instances x 32 ids and never fails the request that caused it.

## D39: A budget of flash writes for the tests on the box

- **Date**: 2026-11-23, proposed in an autonomous run from the user's instruction; finalised
  2026-11-24 by spec 025, which gives it its means (D40)
- **Choice**: a test on the real box causes at most one flash write per file the flow changes, or
  one write and a restart to check that it persists. A run on the box is one pass of the flow being
  shown, never a loop: no repeated `measure-api.sh` (at most once per milestone), no burst or
  stress against the box. Loops, measures and stress run on the Mac (`--sim`) or under QEMU. The
  flow runs inside `sh scripts/box.sh saves --budget <n>` (saves held, one flush at the end), with
  `n` = 1 (the boot marker, written because opening the port resets the box) + the number of files
  the flow changes (4 at most: devices, zones, equipments, recipes) + 1 if the session lasts over
  10 minutes (the marker's removal); a restart to check persistence is one `box.sh log --budget 1`.
  Each spec that runs on the box writes in its result the `flash: <n> writes since boot` count of
  its run and the images it flashed.
- **Why**: the box's flash wears with writes (each save of a JSON file rewrites it, and a flash
  of an image rewrites megabytes); the box is meant to run for years in an annex, and the test
  runs should not spend its endurance. On 2026-11-22 eight runs of `measure-api.sh` and 60 zone
  creations and deletions cost hundreds of saves.

## D40: Saves held from the serial console, writes counted, definitions coalesced over 2 s

- **Date**: 2026-11-24 (spec 025, decided in an autonomous run, for the user's review)
- **Choice**: the radio image's serial console reads `saves hold` (nothing written, everything in
  RAM, forgotten at a restart) and `saves flush` (one write per changed file, then the hold ends),
  never from the network (as `pair`, D36). Because opening the serial port resets the box, the
  hold and its flush are sent in one session of the port by `box.sh saves`, which runs the test's
  command in between; there is no separate `flush` command. Every firmware image counts its writes
  to `/data` and logs `flash: <n> writes since boot`; `box.sh saves` and `box.sh log` check it
  against `--budget`. In the radio image and the host's `--serve`, a change of definitions is
  written 2 s (`DEFINITION_DELAY_MS`) after the saver first sees it unsaved, counted from the
  first change so that a stream of changes is still written every 2 s; a stop writes at once.
  This replaces D13's "definitions at once (the user's choice)": a power cut now loses at most the
  definitions of the last 2 s (and one loop turn), the values of the last 10 minutes as before, and
  everything since a hold. A delay of 0 gives back the former rule.
- **Why**: the 33 requests of the interface's largest creation gave 33 rewrites of
  `equipments.json`; with the delay they give one per 2 s, and inside a held test one in all. Two
  seconds is shorter than any action a person repeats by hand, and a power cut within two seconds
  of a change is rare; the hold and the counter make the budget of D39 something a script checks,
  not a promise.

## D41: The home's time zone is a setting picked from a fixed list, with POSIX rules in the core (the key: see D44)

- **Date**: 2026-11-24 (spec 026, decided in an autonomous run, for the user's review)
- **Choice**: Sowel's settings keys (`home.name`, `home.latitude`, `home.longitude`,
  `home.sunriseOffset`, `home.sunsetOffset`) plus Solite's own `home.timezone`, a name from a fixed
  list of 59 IANA names compiled in the core, each with its POSIX rule
  (`CET-1CEST,M3.5.0,M10.5.0/3`), served by Solite's own `GET /api/v1/settings/timezones`. The core
  parses the rule and converts UTC to local time itself; the sun times are a port of suncalc 2, as
  Sowel's sunlight manager uses, on the root zone only. Unknown keys and values that break a strict
  pattern are refused (Sowel accepts any key and parses with `parseFloat`); `""` is stored and reads as unset (as Sowel); a
  new location or time zone applies at once (no `system.restart_required`). Until this, the root
  zone's `isDaylight` was always `null` (D-history of spec 012).
- **Why**: Sowel takes its time zone from `TZ` or derives it from the location with `tz-lookup`,
  whose database does not fit the box, and the box has no `TZ` to set. A short list with today's
  rules covers the homes the box is for; a rule changed by law, or a zone without a fixed yearly
  rule (Casablanca, Cairo), is out until a list update. The sun times decide motion-light's
  "Inactive during day", the J6 test.

## D42: The backup is the home's settings and zones, as one JSON document of at most 16 KB

- **Date**: 2026-11-24 (spec 026, decided in an autonomous run, for the user's review)
- **Choice**: `GET /api/v1/backup` gives `{"version":1,"settings":{...},"zones":[...]}`;
  `POST /api/v1/backup` takes it as a JSON body, checks it whole, and only then replaces the
  settings whole and creates or updates the document's zones by id (none deleted; equipments,
  devices and recipes untouched; an unchanged zone not rewritten). Both ways at most
  `MAX_BODY_BYTES` (16384 bytes): an export beyond is refused with 413, so that every export can be
  imported back. Sowel's backup (its SQLite tables, a ZIP, a file upload) is not mirrored.
- **Why**: equipments, recipes and devices depend on the devices paired to one box and on its
  radio network's keys; a backup of them is a different feature (secrets, a body beyond the box's
  16 KB worker buffers, a streamed import). The settings and the rooms are what can be prepared on
  the Mac and moved to a box, and fit the existing bound. Merging zones rather than replacing them
  avoids deleting a room that holds equipments.

## D43: Solite is a checked subset of Sowel (replaces D3's equipment types and D14's kinds and flags)

- **Date**: 2026-11-24 (spec 027, decided in an autonomous run, for the user's review)
- **Choice**: every name and value Solite has is Sowel's (`src/shared/types.ts` wins), or a
  deviation listed with its decision in `docs/sowel-subset.md`; anything outside the subset is
  refused or kept untouched. A test (`crates/core/tests/sowel_contract.rs`, a step of
  `scripts/validate.sh`) checks it against a snapshot of Sowel checked in
  (`contract/sowel-snapshot.json`) and against Sowel's repository when present: a snapshot
  behind Sowel is a warning, Solite not fitting Sowel's repository a failure. `EquipmentType` has
  Sowel's 31 values; eight have Sowel's binding rules (D3's seven and `light_color`, the most
  common Zigbee light), the others none. Sowel's twelve optional equipment fields are kept as
  stored; `PUT` takes the four whose rules are shape only (`requireConfirmation`,
  `plugInHybrid`, `trackOrderOutcome`, `family`, with Sowel's `FamilyNotApplicable` and the
  separate-supply and local-production rules on the resulting type) and refuses the eight whose
  rules need what Solite lacks (`energyProfile`, `solarProfile`, `timedCommand`,
  `meteringParentId`, `usualVehicleId`, `separateSupply`, `localProduction`, and
  `invertDirection`, which Sowel applies to orders). The zone aggregation leaves out a `weather`
  equipment and a `water_valve`'s bindings as Sowel does, but has none of Sowel's own counts for
  the types without behaviour (`doorsTotal`, `displaysTotal`, `waterValvesTotal` stay 0), nor
  Sowel's stale rule for the electrical data of meters.
  Value-level differences kept: `null` not stored (Sowel's "absent / null = default"); the texts
  `Not supported: <field>` and `Invalid <field>` (Sowel answers with its schema validator's own
  body); a string field at most 64 characters; one equipment's fields at most 1024 bytes of JSON
  and all of them 8192 (Sowel has no bound), past which a `PUT` is refused and a field is dropped
  at load with a warning; an object compared by its bytes. Names kept as deviations: the routes
  of D28 (permit-join) and D41 (`/settings/timezones`); the backup document of D42 has Sowel's
  paths and its own body.
- **Why**: the user's rule (2026-11-24): Solite becomes an extension of the full Sowel, so a
  client, an export or a future import must read the same model; a check catches a drift that
  care alone misses. The bounds keep the worst case under D23's 1 MB (1004 KB with them).

## D44: Sowel's `GET /api/v1/system/timezone`, and `home.timezone` as a Sowel key (narrows D41)

- **Date**: 2026-11-24 (spec 027, decided in an autonomous run, for the user's review)
- **Choice**: Solite serves Sowel's `GET /api/v1/system/timezone` (`{tz, source, offsetHours}`).
  `home.timezone` is a name Sowel has (its `GLOBAL_READABLE_KEYS`, the keys every plugin may
  read), so it is no longer Solite's own key; Sowel's own time zone does not read it yet, and the
  Sowel issue asking it to stays open. Differences kept: with the setting, `source` is `env` (the
  setting holds the POSIX rule a `TZ` variable would hold; Sowel's three values are `env`, `auto`,
  `fallback`); without it, `fallback` even with a location (Sowel's `auto` needs `tz-lookup`'s
  database, too large for the box); `offsetHours` is the offset now, Sowel's is taken at start.
  `GET /api/v1/settings/timezones` stays (D41).
- **Why**: Sowel's interface reads that route (its current-time pill, the rain summary); the
  list route has no Sowel counterpart, and moving the list into the interface would duplicate the
  core's table of POSIX rules.

## D45: Device dates taken before the clock is set are moved by its jump

- **Date**: 2026-11-25 (spec 028, decided in an autonomous run, for the user's review)
- **Choice**: the home keeps the last wall time it was given (by each turn of the radio loop and
  each tick of the saver). When it was before 2020 and the time given is after, every device date
  before 2020 (`lastSeen`, `createdAt`, `updatedAt`, the data's `lastUpdated` and `lastChanged`)
  is moved forward by the difference. A `lastSeen`, `lastUpdated` or `lastChanged` read in 1970
  from storage is `null`. The shift marks the values as changed, never the definitions.
- **Why**: the box starts in 1970 and gets the time 20 s later; a device heard in between showed
  1970, turned `offline` at the jump and wrote 1970 to the flash. Shifted, its date is right
  within one turn of the loop and it stays `online`; `null` would lose the last message. The last
  wall time lives in the home, not in the loop, so a radio restart (which calls the loop again)
  does not lose the crossing. Marking only the values adds no flash write at boot. Other dates
  taken during the wait (recipe logs and state keys, equipments and zones) are not moved.

## D46: A motion-light keeps running when a button goes stale, and follows its zone's sensors (narrows D38)

- **Date**: 2026-11-25 (spec 028, decided in an autonomous run, for the user's review)
- **Choice**: when the recipes settle, a motion-light button whose equipment no longer has an
  `action` data binding leaves the settings (`Button removed from settings: <name>`), instead of
  the instance being disabled. A running instance restarts only when its lights lost an id; a
  change of its buttons alone, and the motion sources of its zone, are taken in place
  (`Runner::refresh`), its timers and state kept. The settle also runs after an equipment is
  created, a data binding added and a zone updated. Diverges from Sowel's motion-light, which
  resolves its sources once at its start.
- **Why**: a box that runs a month alone must not stop lighting the garage because a button's
  device was removed, nor switch a lit garage off (a restart's start sync) for it; and a motion
  sensor replaced, unbound or moved must count, or stop counting, for the safety auto-off without
  anyone restarting the recipe. Creation and the form still refuse a button without `action`.

## D47: One account, opaque tokens in RAM, PBKDF2-HMAC-SHA256 (narrows D13 and D33)

- **Date**: 2026-11-25 (spec 029, decided in an autonomous run, for the user's review)
- **Choice**: the home has one account, Sowel's admin, set up at first start (no default
  password, EN 18031-1). The password is hashed with PBKDF2-HMAC-SHA256 (RFC 8018), a 16-byte salt
  from the caller's random source, 10 000 iterations (stored with the hash, so a later change still
  checks the old ones), a 32-byte result, compared in constant time; 8 to 128 characters and at
  most 512 bytes (Sowel asks 6: 8 is NIST SP 800-63B's floor). The hash and the user are in
  `auth.json`, written by the saver like `settings.json` (one write per setup or reset, none per
  login); a file that cannot be read stops the start, it is never read as "no account", which would
  open the setup to anyone. A login or a setup opens a session: an access token (900 s) and a
  refresh token (30 days), each 16 random bytes as 32 hex characters, opaque (not a JWT), kept in
  RAM only, at most 8 (a 9th drops the first), lost at a restart. Sowel's five `/auth` routes and
  its gate's three errors are mirrored; setup and login together are limited to 10 attempts per
  minute for the whole box (the core does not see addresses), a 429 beyond, without hashing. Three
  RustCrypto crates enter the core (`sha2`, `hmac`, `pbkdf2`, no default features): std has no hash
  (D12), and a hash written by hand is worse than three audited crates.
- **Why**: a box on the home's network (D33) with no authentication is not acceptable for a product
  (EN 18031-1: no common default password, protection against brute force). The cost is reasoned
  for the ESP32: one iteration is two SHA-256 compressions of about 25 to 50 µs in software at 240
  MHz, so a hash takes 0.5 to 1 s on the HTTP worker that asked, never under the home's lock; the
  ESP32's SHA accelerator (more iterations) is an adapter for later. Tokens in RAM cost no flash
  write and a restart only asks the owner to log in again; opaque tokens need no JWT library, no
  clock agreement with a client and can be closed at logout (stricter than Sowel's JWT). One rate
  limit for the box accepts that a device of the home network can keep the owner from logging in
  while it tries, open sessions and their refresh working: accepted for a home network. Sowel's
  users, roles, API tokens, MFA and password change stay out (password change: roadmap inbox).

## D48: The confirmation at the box is an adapter, and the factory reset is Solite's route

- **Date**: 2026-11-25 (spec 029, decided in an autonomous run, for the user's review)
- **Choice**: the setup and the factory reset need a person at the box. The core defines the
  adapter `Presence` (`confirmed()`, `hint()`), held by the home (`Home::set_presence`), counting
  time on a monotonic uptime clock given to it, never the wall clock, which jumps when it is set;
  the default never confirms. The Mac host confirms always (`AlwaysConfirmed`: it serves on
  loopback, the person at the Mac is at the host). Until the box's button is wired, the box
  confirms during the 10 minutes after a power-on boot (`BootPresence`, the reset reasons of D32's
  pairing window, never after a software restart or a crash). `ButtonPresence` (press, confirmed
  for 120 s) is in the core for the firmware's button later. A refusal is 403
  `{"error": <hint>, "confirmRequired": true}`. `POST /api/v1/system/factory-reset` (Solite's, no
  Sowel counterpart) erases the recipe instances, equipments, devices, zones but the root (its name
  and description back to a first start's), the settings, the account and the sessions, as the
  API's deletions do (with their live events), and answers 409 while a pairing window is open. It
  keeps the Zigbee network and its keys (`zigbee.json`), the Wi-Fi settings and the network
  setting. `Home::factory_reset` is what the button's long press will call.
- **Why**: EN 18031-2 asks that the user can delete their data, and the first person on the home's
  network must not be able to take a box that was not set up. Confirmation by a physical action
  at the box is the answer; the real button (pin, short and long press) needs the box in hand, so
  the core takes the decision behind an adapter, tested here, and the box's 10 minutes after a
  power-on stand in for it. The factory reset leaves the radio network: forgetting its keys would
  make every paired device unreachable, and the forgotten devices come back when they announce
  themselves during a pairing window, so the deletion is partial (an inbox line).

## D49: The box's pairing route behind the token, and `/ws`'s token in the second subprotocol (replaces D28's constraint and D36's "never from the network", narrows D13 and D33)

- **Date**: 2026-11-25 (spec 029, decided in an autonomous run, for the user's review)
- **Choice**: every route of the API and `/ws` need Sowel's bearer token on the Mac host and on
  the box alike; the interface's files stay public. The box's pairing route
  (`/api/v1/integrations/solite/permit-join`) answers as on the Mac, behind the token: D28's
  "stays disabled (404) until J4's password exists" and D36's "never from the network" are
  replaced. The power-on window (D32) and the console's `pair` (D36) stay until the button is
  wired. On `/ws` the token is read from the `Authorization` header or from the first offered
  subprotocol `bearer.<token>` other than `bearer.none`; the interface offers `bearer.none` then
  `bearer.<access token>`, because ESP-IDF echoes only a fixed subprotocol and reads at most 49
  characters of the header: the box echoes `bearer.none` and reads the whole header itself. After
  the upgrade, no token closes 4001 `Authentication required`, a token that is not a live access
  token closes 4001 `Invalid token`; a session that ends later does not close an open socket (as
  Sowel, which checks the token at the handshake). A client that offers only `bearer.<token>`, as
  Sowel's own interface does, gets no subprotocol echoed on the box and its browser drops the
  socket (a deviation, `docs/sowel-subset.md`). The firmware sets ESP-IDF's `httpd_ws` log to
  errors only, so the offered subprotocols (the token) are never written to the serial log.
- **Why**: with a password and a token the reason for D28's and D36's rule (an API open to the
  home network) is gone, and the interface can open pairing from "Add a device" on the box as on
  the Mac. The token goes where a browser can put it (the subprotocol: a browser cannot set
  headers on a WebSocket), and the fixed echo is the one thing the box's server allows.

## D50: The equipment and device pages, Solite's Enabled row, an equipment created without a device (narrows D19 and D30)

- **Date**: 2026-11-25 (spec 030, decided in an autonomous run, for the user's review)
- **Choice**: Sowel's `EquipmentDetailPage` and `DeviceDetailPage` are ported, cut to what Solite
  serves (no history, no type panels, no raw expose), at `#/equipments/<id>` and `#/devices/<id>`
  (no router, D19's hash); D19's "leaf components, not its pages" is narrowed: whole pages are
  ported when they fit the budget. The equipment page adds one row Sowel's interface does not
  have, **Activation** (titled so, its summary "Enabled" or "Disabled"; Sowel's `ConfigRow` and `Switch`, `PUT {"enabled"}`, which Sowel's API
  takes), so a disabled equipment is enabled again without `curl`. The creation form no longer
  needs a device (D30's "at least one device, since Solite has no equipment page" goes): with none,
  the equipment is created alone and its page opens. "Change devices" removes the bindings then
  writes the creation form's plan, stopping at the first refusal (Sowel skips a refused removal),
  not atomic, as Sowel's.
- **Why**: J2: an equipment completed, trimmed or moved to another device from the mouse; Sowel's
  form allows an equipment without device; a refused removal skipped could leave a binding under
  an alias the new plan then fails to write.

## D51: The colour of a bulb as Zigbee2MQTT's object, Sowel's `json` type

- **Date**: 2026-11-25 (spec 031, decided in an autonomous run, for the user's review)
- **Choice**: a device with the Color Control cluster (`0x0300`) gets the data `color_temp`
  (number, `light_color_temp`, mired) and `color` (Sowel's `json` type, `light_color`), and the
  orders `color_temp` (`set_color_temp`, 150 to 500 mireds, Zigbee2MQTT's default range, not read
  from the bulb) and `color` (`json`, `set_color`). The `color` value is Zigbee2MQTT's object, at
  most `x`, `y` (0 to 1), `hue` (0 to 360), `saturation` (0 to 100), in two groups: the colour
  measures of one frame replace their fields, and a frame of one group drops the other group (no
  `color_mode` is kept). A `color` order takes `{x, y}` (Move to Color) or `{hue, saturation}`
  (Move to Hue and Saturation), options 0 as Zigbee2MQTT (a bulb that is off may ignore it).
  Reporting is set up for the colour temperature and X/Y; hue and saturation are read back after a
  hue and saturation order; a read-back carries up to four attributes in one request, so a colour
  comes back in one update. Every `0x0300` device gets both colour data and orders:
  `ColorCapabilities` is not read, so a tunable white bulb has a `color` that its bulb refuses.
  The memory worst case puts the four-field object (632 bytes on the Mac) on its 16 lights, not on
  its 64 devices: a colour bulb has 4 data where the test's devices have 16, and the 12 it lacks
  weigh more than its object. A `devices.json` holding `"type":"json"` is not read by an older
  release (the host stops, the file is kept): going back needs the device paired again or the file
  removed.
- **Why**: Sowel's model names the colour `color`, `light_color`, typed `json`
  (`src/shared/constants.ts`, `binding-candidates.test.ts`), with Zigbee2MQTT's payload; one
  update per colour keeps clients and recipes from seeing a half-changed colour; reading the
  capabilities is a new step of the pairing setup, left for later (inbox).
