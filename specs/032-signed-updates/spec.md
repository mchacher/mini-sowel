# Spec 032: signed-updates

## Context

Roadmap J5, "Updates from afar: signed firmware, written to the inactive partition, automatic
return to the old one if the new does not start", fifth item of the "Order of work while the box
is away" (2026-11-25): **the core part only**. The true north asks for signed binaries and a box
that runs a month without intervention; an update that cannot fall back would end that month with
a dead box. The partition table already has two application slots, `ota_0` and `ota_1` (D25).

This run is autonomous (the user is away): every open question is decided here and recorded
(decisions D52 to D54, and "Decisions taken in this spec" below). Nothing is flashed: the box's
run is listed in the roadmap's "Tests to run with the box in hand" (one pass, no loop, D39: an
update writes the flash).

## Goals

1. An update is a **signed update file**: a 128-byte header signed with Ed25519 (version, image
   length, SHA-256 of the image), then the image. The running program checks the signature with a
   public key given **at build time**; no private key is ever in the repository.
2. The core installs an update by **streaming** it, chunk by chunk, into the inactive slot through
   an adapter, and marks it as the next boot only when the whole image matches its signed digest:
   a bad signature, a truncated or altered image, a version not newer, an image too large, a write
   that fails or a power cut leave the running image as the one that boots.
3. A newly booted image is **on probation**: it confirms itself once it has run healthy for a
   minute; if it is not confirmed within five minutes it is rolled back to the previous image (and
   a crash before the confirmation rolls back at the next boot, by the bootloader).
4. Sowel's `POST /api/v1/system/update` and `GET /api/v1/system/version`, behind the token of spec
   029; a small "Firmware update" card on the Settings page.
5. The host keeps its slots as files (a file adapter), so the API and the interface are shown on
   the Mac; the firmware's adapter is written on ESP-IDF's OTA (`esp-idf-svc`) and wired in the
   radio image, checked by its build only.
6. The release side: `solite-host` generates a signing key into a file outside the repository and
   signs an image; the steps a maintainer follows are written in the living spec.

## Out of scope

- Fetching an update from the network (Sowel's GitHub release poll, `POST /system/version/check`,
  `system.update.available`): the image is uploaded from the browser. Inbox.
- Sowel's events `system.update.progress` and `system.update.error` on `/ws`: the upload's
  answer carries the outcome. Inbox.
- Publishing a release (where the signed file is hosted, a manifest): the `/solite-release` skill,
  later.
- ESP32 Secure Boot V2 and flash encryption (eFuses, irreversible): D52 explains why the update's
  own signature comes first. Inbox.
- Key rotation, several keys, a revocation list: one public key per build.
- Running the uploaded image on the Mac: the host stores it and its boot record only.
- A box run: pending, in the roadmap's list.

## Requirements

### R1: The signed update file

One file: a header of exactly 128 bytes, then the image (the ESP32 application image of the slot,
as `espflash save-image` writes it). All integers little-endian.

| Offset | Bytes | Field | Value |
| ------ | ----- | ----- | ----- |
| 0 | 8 | magic | ASCII `SOLITEUP` |
| 8 | 4 | format | `1` (u32) |
| 12 | 2 | major | u16 |
| 14 | 2 | minor | u16 |
| 16 | 2 | patch | u16 |
| 18 | 2 | reserved | 0 |
| 20 | 4 | image length | u32, bytes after the header, at least 1 |
| 24 | 32 | digest | SHA-256 of the image |
| 56 | 8 | reserved | 0 |
| 64 | 64 | signature | Ed25519 (RFC 8032, pure, not prehashed) over bytes 0 to 63 |

- The header is checked in this order, **before anything is written**: magic (else 400 `Not a
  Solite update`), format (else 400 `Unsupported update format <n>`), reserved bytes zero and
  image length at least 1 (else 400 `Malformed update header`), signature against the build's key
  with `ed25519-dalek`'s `verify_strict`, which refuses non-canonical signatures (else 400
  `Invalid signature`), version strictly newer than the running one, compared as
  (major, minor, patch) (else 400 `Version <x.y.z> is not newer than <running>`), image length at
  most the slot's capacity (else 413 `Image too large for the update slot`).
- The running version is `solite_core::VERSION` (the workspace's `Cargo.toml`), parsed as
  `major.minor.patch`.
- A file shorter than 128 bytes: 400 `Truncated update`.

### R2: The public key, a build-time input

- The environment variable `SOLITE_UPDATE_PUBLIC_KEY`, at build time, holds the Ed25519 public
  key as 64 hexadecimal characters (either case). The host and the firmware read it with
  `option_env!`; the core takes the key as a value and never reads the environment.
- Absent: the program has no update key; `POST /api/v1/system/update` answers 503 `Updates are not
  configured: this build has no update key`, and its start logs `update: no key, updates refused`.
- Present but not 64 hex characters, not a valid Ed25519 point, or a weak (small-order) key: same
  as absent, and the start
  logs `update: invalid key, updates refused`.
- With a key, the start logs `update: key <first 8 hex characters>`.
- The verification adds `ed25519-dalek` without its precomputed tables to the core: a few tens of
  KB of code in a firmware image that must stay inside its 4 MB slot (the build's size is read
  and written in the spec's result).
- No private key, seed or test key is in the repository; the tests generate their key pairs at
  run time from random bytes.

### R3: The update adapter (core trait)

`update::UpdateAdapter`, implemented by the host (files) and the firmware (ESP-IDF OTA), and by a
fake in the tests:

| Method | Does |
| ------ | ---- |
| `capacity()` | The inactive slot's size in bytes |
| `begin(length)` | Opens the inactive slot for an image of `length` bytes (the box erases that much) |
| `write(chunk)` | Writes the next bytes, in order |
| `abort()` | Drops what was written: the inactive slot is not bootable; the boot record unchanged |
| `finish()` | Closes the slot, has the platform check its image, and makes it the next boot, on probation |
| `pending()` | True when the running image is on probation (booted from an update, not confirmed yet) |
| `confirm()` | The running image is kept from now on |
| `roll_back()` | The running image is marked bad and the previous one becomes the next boot (the box restarts into it) |

Errors are a message (`String`). The boot record changes in `finish`, `confirm` and `roll_back`
only, each in one step that a power cut cannot leave half done.

### R4: The installation, streamed

`update::Updates` holds the key, the running version and the state (idle, installing, installed);
the host and the firmware share one. `Updates::install(source, adapter)` reads the update from
`source` (`std::io::Read`):

1. Refusals before reading the body (the state is released on every path, a panic included: a
   guard): no key (503); an installation in progress (409 `Update
   already in progress`); an update already installed and waiting for the restart (409 `An update
   is installed and waits for the restart`); the running image on probation (409 `The running image
   is not confirmed yet`: the inactive slot holds the image to fall back to).
2. Reads the 128-byte header, checks it (R1).
3. `begin(length)`; then reads at most `CHUNK_BYTES` (4096) at a time into one fixed buffer, hashes
   and writes each chunk, until `length` bytes are written. The image is never held whole: the
   installation's memory is the header, the buffer and the hash state, whatever the image size.
4. The source ends before `length` bytes: `abort()`, 400 `Truncated update`. A read error (a
   client gone, a timeout): `abort()`, 400 `Truncated update`.
5. One more byte after `length`: `abort()`, 400 `Update longer than declared`.
6. The SHA-256 of what was written differs from the header's digest: `abort()`, 400 `Image does
   not match its signature`.
7. A `begin`, `write` or `finish` error: `abort()` (after `begin` succeeded), 500 `Update failed:
   <message>`, logged `update: failed: <message>`.
8. Otherwise `finish()`; the state becomes installed; 200 with Sowel's `{"success": true,
   "message": "Installed <x.y.z>, restarting"}`; logged `update: <x.y.z> installed, <KB> KB`.

A power cut at any point before `finish` returns leaves the boot record unchanged: the running
image boots again.

### R5: Probation and rollback

`update::Probation` decides, from the time and the program's health, ticked about once a second:

- **Healthy** means: the engine's loop is running (set when `engine::run` starts, cleared if it
  returns) **and** the API server is listening (set once its start succeeded). The Wi-Fi link is
  not required: a router off during the probation must not roll back a good update (D54).
- Healthy without a break for `CONFIRM_AFTER_MS` (60 000 ms): **confirm**. A break restarts the
  minute.
- Not confirmed `ROLLBACK_AFTER_MS` (300 000 ms) after the start: **roll back**.
- Not on probation (`pending()` false at start): nothing to do.
- `Updates::watch(adapter, now, healthy)` applies the verdict once: `confirm()` (logged `update:
  running image confirmed`) or `roll_back()` (logged `update: not confirmed in time, rolling
  back`); a `confirm()` error is logged and tried again at the next tick; a `roll_back()` error is
  logged and tried again at the next tick.
- While on probation the state refuses installations (R4.1); once confirmed it accepts them.
- A crash, a restart or a power cut before the confirmation: the bootloader rolls back by itself at
  the next boot (ESP-IDF's app rollback, `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE`). So unplugging
  the box during the minute after an update brings the old image back: the update is then simply
  installed again.
- After `roll_back()` succeeds the program is no longer on probation (`pending()` false); on the
  box it restarts into the old image at once; on the host, which does not restart, installations
  are accepted again.

### R6: The routes

- `GET /api/v1/system/version`, behind the gate of spec 029: 200 `{"current": "<VERSION>",
  "latest": null, "updateAvailable": false, "releaseUrl": null}` (Sowel's `VersionInfo`, its two
  Docker fields left out: a subset). Another method: 405.
- `POST /api/v1/system/update`, behind the gate (401/403 as every route, before any body byte is
  read): its body is the signed update file, `Content-Type: application/octet-stream` (else 415
  `Unsupported Media Type`). The core answers `Reply::Update` once the gate and the type pass; the
  host or the box then streams the body into `Updates::install` (R4). Another method: 405.
- The 16 KB body limit does not apply to this route only; a declared `Content-Length` above 128 +
  4 MB + 1 is 413 before any read; on the box a missing `Content-Length` is 411.
- The body source ends (end-of-file) once `Content-Length` bytes are read: the check for a longer
  file (R4.5) never waits on the socket.
- Every refusal of this route that leaves body bytes unread (401, 403, 405, 409, 411, 413, 415,
  503, a header refused, an error mid-image) **closes the connection** after the answer (the box:
  `close_after`; the host: the door, `front.rs`, closes it after each request), so the image's bytes are never read as a next
  request.
- Through the plain `handle` (tests, tools, which cannot stream): `POST /system/update` answers
  501 `Updates need the streaming handler`, after the same gate-free checks as other routes.
- A `Transfer-Encoding: chunked` upload on the box: 411, as every request there (a browser sends a
  `File` with its length).

### R7: The host

- `--serve` builds `Updates` with the build's key and a file adapter on `<data>/update/`:
  `slot-0.bin`, `slot-1.bin` and `boot.json` (`{"boot": 0|1, "pending": bool}`, absent = slot 0,
  not pending). The capacity is 4 MB (`0x400000`, the box's slot). `begin` creates
  `slot-<n>.bin.part` (`n` the slot that is not `boot`), `finish` renames it to `slot-<n>.bin` and
  writes `boot.json` (`{"boot": n, "pending": true}`) through a temporary file and a rename;
  `abort` deletes the `.part` file; `confirm` writes `pending: false`; `roll_back` writes the other
  slot, `pending: false`.
- A stalled upload on the host ends within 30 s without a byte: the door (`front.rs`) already
  sets that idle read timeout on the client, which ends the stream; the installation then aborts
  as truncated and the state returns to idle (tested through the real server).
- The host does not run the image it stores, nor restart: after an installation it prints
  `update: <x.y.z> written to slot <n>; restart the host to put it on probation`. At start, a
  `boot.json` with `pending: true` puts the host on probation (R5), logged `update: slot <n> on
  probation`; its health as R5 (engine loop running, API server listening).
- `--update-keygen <file>` writes a new signing key (32 random bytes from the system's random
  source, as 64 hex characters and a newline) to `<file>`, created new with mode `0600`, and prints
  `public key: <64 hex>`. It refuses an existing file, and a path inside a git working tree (a
  `.git` in the file's folder or one above), exit 2 with a line saying why.
- `--update-sign <image> --version <x.y.z> --key <file> --out <file>` writes the signed update file
  of R1 and prints `signed: <out>, version <x.y.z>, <bytes> bytes, key <first 8 hex>`. It refuses
  an image larger than 4 MB or empty, a version not `x.y.z` of u16, a key file that is not 64 hex
  characters, an existing `<out>` (exit 2, nothing written).
- The version is tied to the image: when the image is an ESP32 application image (first byte
  `0xE9`, and the application descriptor's magic `0xABCD5432` at offset 32), `--update-sign` reads
  the descriptor's version (offset 48, 32 bytes, NUL-terminated) and refuses a `--version` that
  differs (exit 2). An image without a descriptor (the Mac demo's bytes) is signed with the line
  `signed: no application descriptor, version not checked`.
- The descriptor must carry `VERSION`: `firmware/Cargo.toml`'s own `version` (the firmware is
  outside the workspace) is checked equal to the workspace's by `scripts/validate.sh` (a failing
  line names both). The implementer checks in a built image which crate's version the descriptor
  holds; if it is not the firmware's (esp-idf-sys writing its own), the firmware emits its own
  with `esp_idf_sys::esp_app_desc!()` and esp-idf-sys's default one switched off. The result is
  written in this spec's result.
- The signing is in the host (`ed25519-dalek` with its signing part); the core verifies only.

### R8: The box (radio image)

- `firmware/src/ota.rs` implements `UpdateAdapter` on `esp_idf_svc::ota::EspOta`: `begin` with
  the known size, `write`, `abort`, `finish` (complete and set as boot partition), `pending` (the
  running slot's state is pending verify), `confirm` (`mark_running_slot_valid`), `roll_back`
  (`mark_running_slot_invalid_and_reboot`). Every call that touches the flash runs on one OTA
  thread whose stack is in internal RAM (the API workers' stacks are in PSRAM and must never write
  the flash): the adapter sends each call to it through a channel of one request at a time and
  waits for its answer. The same thread ticks the probation every second (one thread, not two).
  Its stack: 6 KB of internal RAM, for the whole run (set against the 64 KB internal reserve; the
  box run reads the internal RAM free after it). A chunk crosses in one 4096-byte buffer allocated
  once by that thread with `heap_caps_malloc(MALLOC_CAP_SPIRAM)`, as `body_buffer()` of
  `http_server.rs` does, so in PSRAM (ESP-IDF's flash write copies
  from PSRAM through its own internal bounce buffer); `docs/memory-budget.md`'s update row is
  rewritten to match.
- The API worker sends `POST /api/v1/system/update` to `handle_guarded` before reading the body
  (the 16 KB check of the worker is skipped for this route only); on `Reply::Update` it streams
  the body (its length from `Content-Length`, end-of-file there) into `Updates::install`; on success it answers, then asks the engine to stop: the engine's final save
  runs and the box restarts (`esp_restart`), logged `update: restarting`.
- At boot, the OTA thread ticks `Updates::watch` every second with health as R5; logs as R5.
- `sdkconfig.defaults` gets `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y`. It is a **bootloader**
  setting: an update over the air replaces the application, never the bootloader, so it reaches
  the box only through the bootloader that `scripts/box.sh flash` writes. Until the box has been
  flashed once over USB with this spec's image (the user's yes, D6), a new image is never put on
  probation (its state stays `NEW`, `pending()` false) and nothing rolls back. The setting also
  reaches the bootloader of the sim and QEMU images (with `otadata` erased, they boot `ota_0` as
  before).
- Checked by `cargo build --release` of every firmware image (validate.sh); the box run is pending.

### R9: The interface

A "Firmware update" card on the Settings page, after the backup card, Sowel's tokens and classes:
the running version (`GET /system/version`), a file input (`.sup`), an "Install" button disabled
until a file is chosen and while the upload runs ("Installing…"), then the answer's `message` or
`error` on a line under it (success in the success token, error in the danger token). After a
success the card adds "The box restarts; reload the page in a minute." (the box's restart drops
the connection; the page does not poll). Cost to the first load: about 1 to 2 KB gzip, under the
170 KB budget that `npm run check:bundle` checks. The request
carries the bearer token as every API call and `Content-Type: application/octet-stream`. French
and English labels.

## Decisions taken in this spec

- D52: Ed25519 over a 128-byte header carrying the image's SHA-256, the key at build time, signing
  in the host; why not ESP-IDF Secure Boot now.
- D53: Sowel's `POST /system/update` takes the signed file as its body (a deviation: Sowel's takes
  none and pulls from GitHub), `GET /system/version` as a subset; one installation at a time,
  none while the running image is on probation or an update waits for the restart.
- D54: Probation: healthy (engine loop running and API server listening, not the Wi-Fi link) for
  one minute confirms, five minutes without it rolls back; the bootloader's rollback needs one USB
  flash of this spec's bootloader; a release
  that can roll back must read what its successor wrote (the rule for every format change from
  now on); the first image that can roll back is this one (once its bootloader is flashed), which
  reads `json` data (D51), so the rollback note of J5 is met.
- The version check refuses an equal or older version (no downgrade through the API: an attacker
  with a token cannot reinstall an old signed image with a known flaw). A downgrade is a flash
  with the user's yes (D6).
- File extension `.sup` ("Solite update package").

## Living spec delta

### docs/living-spec/README.md

- New section "Updates itself from a signed file": R1 to R6 in business words: the file's form,
  the key given at build time, the checks and their answers, the streaming (never held whole), the
  probation (one minute healthy confirms, five minutes without rolls back), the refusals while on
  probation or waiting for the restart.
- The API's route list: `GET /api/v1/system/version` and `POST /api/v1/system/update`.
- The host's commands: `--update-keygen`, `--update-sign`; `--serve`'s `update:` lines and its
  `<data>/update/` folder.
- New section "Signing a release" (the maintainer's steps, the `/solite-release` skill points to
  the living spec for them): generate the key once with `--update-keygen` into a file outside the
  repository (kept offline, e.g. an encrypted volume or a password manager, backed up: a lost key
  means a box can only be updated by a flash), build the firmware with
  `SOLITE_UPDATE_PUBLIC_KEY=<hex>`, make the application image with `espflash save-image`, sign it
  with `--update-sign`, publish the `.sup` (publishing: later).
- The firmware: the radio image's update route, its OTA thread (calls and probation) and its
  `update:` lines; `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE`, which needs one USB flash.
- The `flash` paragraph of `box.sh`: the bootloader it writes carries the app rollback.
- Unplugging the box during the minute after an update brings the old image back.
- The Settings page: the "Firmware update" card.

### No longer true

Searched in the build (names `OTA`, `ota`, `update`, `signed`, `sign`, `system/`, `J5`, `rollback`
in `docs/` except the roadmap and decisions, `README.md`, `CLAUDE.md`, `inputs/README.md`,
`.claude/skills`, `.claude/agents`); each sentence, file: before, after:

- `docs/memory-budget.md`, PSRAM table: "Update buffers: the download's chunk buffer and the flash
  write buffer, 32 KB" became one 4 KB chunk buffer in PSRAM, one update at a time; the worst-case
  arithmetic (1224 KB, margin 824 KB) became 1196 KB, margin 852 KB, and the realistic home's (564
  KB, margin 1484 KB) became 536 KB, margin 1512 KB; the internal RAM table gained the OTA thread's
  6 KB stack, not added (inside the task stacks, set against the 64 KB reserve).
- `docs/living-spec/README.md`, `flash` paragraph of `box.sh`: it said only what is written; it now
  says the bootloader it writes carries the app rollback, and that a box never flashed with it never
  puts an update on probation.
- `docs/living-spec/README.md`, "Serves Sowel's API from the box": "a body over 16 KB, 413" now has
  the update route's exception (streamed).
- `docs/living-spec/README.md`: the route table and history gained the two `/system` routes and
  spec 032 (it listed no update route); the Settings page's description gained the Firmware update card.
- `docs/architecture.md`: the Blocks table had no update block (core module, adapters, route
  streamed); "Builds" and "Data" gained the build-time key and the host's `update/` folder.
- `.claude/skills/solite-release/SKILL.md`, Phase 3 step 3: "signed binary, OTA manifest: see the
  roadmap ... the steps written in `docs/living-spec/` for publishing a firmware" became a pointer to
  "Signing a release" for the signing steps, publishing and the manifest being not written yet.
- `docs/roadmap.md`: J5 row, item 4 of the order of work, the tests to run, the inbox and the log.
- `docs/specs-index.md`: row 032, status "built (demo and box check pending)".
- Already done by part A: `docs/sowel-subset.md` (two routes, D53); `api/system.rs`'s module comment.
- Not mine, left for the firmware part: `firmware/sdkconfig.defaults` comments.
- Checked and still true: `docs/hardware.md` (Sonoff's OTA images, `otadata` erased by `flash`),
  `CLAUDE.md` (signed binaries as a foundation; the adapter traits include updates), `README.md`,
  `inputs/README.md`, `.claude/agents/*` (rules on signatures, no change needed).

## Acceptance criteria

- [ ] A valid update file is installed into the inactive slot, the boot record points to it on
      probation, the answer is 200 `{"success": true, "message": "Installed <v>, restarting"}`.
- [ ] A bad signature, a header altered after signing, an image altered after signing, a truncated
      file, a longer file, a version equal or older, an image larger than the slot, a wrong magic
      or format: each refused with its status and message, the boot record unchanged, nothing left
      bootable in the inactive slot.
- [ ] A power cut (the installation dropped mid-image) leaves the old image as the next boot.
- [ ] An image on probation is confirmed after one minute healthy, and rolled back after five
      minutes without it; installations are refused while on probation.
- [ ] An installation of 1 MB peaks at under 32 KB of heap (bounded buffers).
- [ ] The routes answer 401 without a token, 415 with another content type, 503 without a key.
- [ ] `solite-host --update-keygen` and `--update-sign` make a file the host accepts; keygen refuses
      a path inside the repository.
- [ ] The Settings page shows the version and installs a chosen file (Vitest).
- [ ] Every firmware image builds with the OTA adapter (validate.sh); the box run is listed as
      pending.
- [ ] No private key or seed in the repository (`git grep` of the diff for 64-hex strings shows
      none but public test vectors, if any).

## Edge cases

- No key at build, an invalid key: 503, start lines.
- Header shorter than 128 bytes; an empty body: 400 `Truncated update`.
- Image length 0: 400 `Malformed update header`. Larger than the slot: 413 before any write.
- Two uploads at once: the second 409.
- The client stops sending mid-image (a timeout on the box): 400, slot aborted.
- `finish` refused by the platform (ESP-IDF finds the image invalid, e.g. a host binary signed by
  mistake): 500, slot aborted.
- A second update after a successful one, before the restart: 409.
- The probation's `confirm` failing (a flash error): logged, tried again next tick.
- Health lost after being gained: the minute starts over; the five-minute deadline still counts
  from the start.
- The wall clock jumping (NTP): the probation uses the monotonic clock.

## Sowel contract

- `GET /api/v1/system/version`: Sowel's `VersionInfo` (`src/core/version-checker.ts` of the Sowel
  repository, served by `src/api/routes/system.ts`), its fields `current`, `latest`,
  `updateAvailable`, `releaseUrl`; `dockerAvailable` and `composeManaged` left out (a subset).
- `POST /api/v1/system/update` (`src/api/routes/system.ts`): Sowel's answer `{success, message}`,
  its error shape `{error}`, its 409 `Update already in progress` and its 503 when updates cannot
  run; the body (the signed file, `application/octet-stream`) is a deviation: Sowel's route takes
  none and pulls the release itself (D53). Both rows go into `docs/sowel-subset.md`.
- No `EngineEvent` added (Sowel's `system.update.*` events: out of scope).

## How the user will check it

On the Mac, no box: `--update-keygen` into a folder outside the repository, a host built with the
printed key serving the simulated garage, a fake "image" (any bytes) signed as a version above the
running one with `--update-sign`; on the Settings page, the version shown, the file chosen,
"Install", the success message; `<data>/update/boot.json` pointing to slot 1, pending; the same
file again refused (409 waiting for the restart); after a restart of the host, `update: slot 1 on
probation`, then `update: running image confirmed` a minute later. A file with one byte changed:
`Image does not match its signature`. The rollback and the power cut are shown by the tests.

On the box (pending, one pass, in the roadmap's list): first one USB flash of this spec's radio
image with `--keep-storage` (the user's yes, D6: it brings the bootloader with the rollback);
then a real image of a higher version signed and installed from the interface, the restart,
`update: running image confirmed`, the internal RAM free read; then an image that panics at start
(a local throwaway change, not committed) installed: the box comes back to the previous image.
The serial log of these passes is read with `box.sh log --raw` (the checked mode fails on a
second banner, which a restart prints).

## Result (2026-11-25, autonomous run, on the Mac)

- Core, host and interface built and tested (`cargo test --workspace`: 1251 tests; UI tests;
  first load 73.4 KB gzip of 170 KB).
- Firmware: the four images build with the OTA adapter. The radio image from `espflash
  save-image` is 2,133,520 bytes (2,005,888 before this spec), 51.7 % of its 4 MB slot. Its
  application descriptor reads `0.1.0` at offset 48, project `solite-firmware`: esp-idf-sys's own
  descriptor held `git describe` (`98d38da-dirty`), so the firmware emits its own
  (`firmware/src/app_desc.rs`, `esp_app_desc! {}`).
- `pending()` on the box is true only for ESP-IDF's `PENDING_VERIFY` state, so a box whose
  bootloader has no rollback (state `NEW`) is never on probation and still accepts updates.
- On the box, a request of the update route without `Content-Length` answers 411 before the gate
  (as a chunked request, refused before it), and 413 for a declared length too large after it.
- `validate.sh`: every step green but the last, the engine's run in QEMU: heap peak 363 KB above
  D27's 350 KB, the same figure on `main` at 98d38da without this spec (inbox line).
- Not run: the demo with the user, the box run (pending in the roadmap's list).
