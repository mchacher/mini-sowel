# Plan: Spec 032

## Steps

- [x] 1. Living spec updated (the delta of `spec.md`, the "Signing a release" section included);
       decisions D52 to D54 written
- [x] 2. Core types: `Version`, the header, `UpdateAdapter`
- [x] 3. Core logic: header check, `Updates::install` (streamed), `Probation`, `Updates::watch`
- [x] 4. Adapters: host file adapter; firmware `ota.rs` on `EspOta` through an internal-RAM
       thread (calls and probation ticks), the restart; `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE`
- [x] 5. API: `Reply::Update`, `is_update_request`, `GET /system/version`, `POST /system/update`
       in the host and the box (streamed, before the 16 KB check); `ROUTES`; contract check
- [x] 6. Host commands: `--update-keygen`, `--update-sign`
- [x] 7. Interface: the "Firmware update" card on the Settings page
- [x] 8. Tests (table below), each failing once with the code broken
- [x] 9. Docs: sowel-subset, architecture, memory budget, roadmap (J5 row, the box run in "Tests
       to run with the box in hand", log line, inbox lines of the out-of-scope items), specs
       index, `.gitignore`, "No longer true" searched and corrected

## Test plan

Key pairs are made at run time from random bytes (`std::collections::hash_map::RandomState` or
the system's random source); no key is written in a test file.

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| `update` (`tests/update.rs`) | Valid file, version above `VERSION` | 200 `{"success":true,"message":"Installed <v>, restarting"}`; fake: image bytes equal, `finish` called, next boot = inactive slot, pending |
| `update` | Signature by another key | 400 `Invalid signature`; `begin` never called; boot unchanged |
| `update` | Header byte changed after signing (the version) | 400 `Invalid signature`; nothing written |
| `update` | Image byte changed after signing | 400 `Image does not match its signature`; `abort` called; boot unchanged |
| `update` | File cut mid-image | 400 `Truncated update`; `abort` called; boot unchanged |
| `update` | File of 100 bytes; empty body | 400 `Truncated update`; nothing written |
| `update` | One byte more than declared | 400 `Update longer than declared`; `abort`; boot unchanged |
| `update` | Version equal to `VERSION`; version lower | 400 `Version <v> is not newer than <VERSION>`; nothing written |
| `update` | Image length above the capacity | 413; nothing written |
| `update` | Image length 0; reserved byte not 0 | 400 `Malformed update header` |
| `update` | Wrong magic; format 2 | 400 `Not a Solite update`; 400 `Unsupported update format 2` |
| `update` | No key | 503 `Updates are not configured: this build has no update key` |
| `update` | Power cut, reader side: the reader returns an I/O error after half the image | 400 `Truncated update`; `abort`; boot record unchanged, the fake's "next boot" is the running slot; state idle again |
| `update` | Power cut, flash side: the fake refuses writes after n bytes | 500 `Update failed: <message>`; `abort`; boot record unchanged |
| `update` | Power cut, nothing called after the cut: the installer dropped mid-image (a reader that panics, caught) | the fake's next boot is the running slot; the state is idle again (guard) |
| `update` | `finish` refused by the fake | 500 `Update failed: <message>`; `abort`; boot unchanged |
| `update` | Second install while one runs (a reader that blocks, from another thread) | 409 `Update already in progress` |
| `update` | Install after a successful one | 409 `An update is installed and waits for the restart` |
| `update` | Install while the running image is on probation | 409 `The running image is not confirmed yet` |
| `update` | Chunks: the fake sees no write larger than 4096 bytes | holds |
| `update` (memory, `tests/update.rs` or `memory.rs`) | 1 MB image through a counting fake that keeps no bytes | peak heap of the install under 32 KB |
| `update` | `parse_public_key`: valid hex (upper and lower case), 63 chars, non-hex, invalid point | `Some` / `None` |
| `update` | `Version` parse and order: `0.2.0 > 0.1.9`, `1.0.0 > 0.99.99`, `x.y`, `1.2.3.4`, `70000.0.0` refused | holds |
| `update` (`Probation`) | Not pending | no action ever |
| `update` | Healthy from 5 s, ticks to 65 s | `confirm` at 65 s once; then installs accepted |
| `update` | Never healthy | `roll_back` at 300 s, once |
| `update` | Healthy 30 s, unhealthy, healthy again at 100 s | `confirm` at 160 s, not before |
| `update` | Healthy from 280 s | `roll_back` at 300 s (the deadline wins) |
| `update` | `confirm` fails once | tried again at the next tick, then confirmed |
| `update` | `roll_back` fails once | tried again at the next tick; after it succeeds, not on probation, installs accepted |
| `update` | Valid key: `verify_strict` used; a small-order public key | `parse_public_key` gives `None` |
| `api` (`tests/update.rs` or `tests/api.rs`) | `POST /system/update` without a token | 401, body not read |
| `api` | With a token, `Content-Type: application/json` | 415 |
| `api` | With a token, `application/octet-stream` | `Reply::Update` |
| `api` | `GET /system/version` | 200 `{"current":"<VERSION>","latest":null,"updateAvailable":false,"releaseUrl":null}` |
| `api` | `PUT /system/version`, `GET /system/update` | 405 |
| `api` | Plain `handle` on `POST /system/update` | 501 `Updates need the streaming handler` |
| `api` (`sowel_contract.rs`) | The two routes are in `ROUTES`, subset and deviation rows | the check passes |
| `api` | Existing behaviour: a 20 KB JSON body on another route | still 413 |
| `api` | `GET /system/version` without a token | 401 |
| host `update_files` (unit tests) | install, then `boot.json` = `{"boot":1,"pending":true}`, `slot-1.bin` equal | holds |
| host `update_files` | abort: no `.part` left, `boot.json` unchanged | holds |
| host `update_files` | confirm, roll back: `boot.json` contents | holds |
| host `update_files` | A second install writes slot 0 when booted on 1 | holds |
| host (`--update-sign`, `--update-keygen`, test in `crates/host/tests/` or unit) | key generated in a temp dir, image signed, the core accepts it with the public key printed | 200 |
| host `--update-sign` | image empty; image over 4 MB; version `1.2`; key file of 10 chars; existing `<out>` | exit 2 each, nothing written |
| host `--update-sign` | an ESP32 image (`0xE9`, descriptor magic, version `0.2.0`) signed as `0.3.0`; as `0.2.0` | exit 2; signed |
| host `--update-sign` | bytes without a descriptor | signed, with the `version not checked` line |
| host start lines (unit on the function that builds `Updates` from the key text) | no key; `zz..`; a valid key | `update: no key, updates refused`; `update: invalid key, updates refused`; `update: key <8 hex>` |
| host `update_files` + probation | `boot.json` `{"boot":1,"pending":true}` at start | on probation: `pending()` true, `update: slot 1 on probation`, installs 409 |
| host serve, end to end through the real server (`crates/host/tests/`) | a signed file of 64 KB uploaded with a token | 200; `boot.json` written |
| host serve, end to end | the same without a token | 401, `Connection: close`, the image bytes not answered as a second request |
| host serve, end to end | an upload that stops sending mid-image | ends within the idle timeout (shortened through the existing timing settings if any, else the test waits 30 s and is `#[ignore]`d as slow); the next upload is not 409 `in progress` |
| host | keygen into an existing file; into a folder with `.git` above | exit 2, nothing written |
| host serve (existing tests) | The API's other routes unchanged | existing tests pass |
| UI `SettingsPage.test.tsx` | The update card shows the version from `GET /system/version` | `0.1.0` shown |
| UI | A file chosen, "Install": POST with `application/octet-stream` and the file as body; success message shown | holds |
| UI | An error answer | the `error` text shown |
| UI | "Install" disabled before a file is chosen | holds |
| UI | During the upload (a pending promise) | button shows "Installing…" and is disabled |
| UI | After a success | the "reload the page in a minute" line shown |
| firmware | `cargo build --release` of every image (validate.sh) | builds |
| `validate.sh` | `firmware/Cargo.toml` version differs from the workspace's | the check fails, naming both |
| firmware image | the descriptor's version in the built radio image (`espflash save-image`, read at offset 48) | equals `VERSION` (written in the spec's result) |

## Validation

- [ ] Validate command green (all but the QEMU heap peak, pre-existing on main: see the result in spec.md)
- [x] Agent review done, blocking findings fixed
- [ ] Demo seen and approved by the user (pending: autonomous run)
