# Plan: Spec 039

## Steps

- [x] 1. Living spec updated (the delta of `spec.md`)
- [x] 2. Core types: `Request.content_length`, `Reply::VersionCheck`/`UpdatePull`, `Releases`,
      `Checked`, `Fetcher`, `Fetched`, `Limits`, settings keys
- [x] 3. Core logic: `update/release.rs` (manifest, URL rule, redirects, check, pull, cache,
      automatic check), `install_expecting`, the routes in `api/`
- [x] 4. Host adapter `fetch.rs`, `serve.rs` wiring, automatic check thread
- [x] 5. Firmware: `fetch.rs`, `http_server.rs`, `job_thread.rs`, `radio_engine.rs`, sdkconfig,
      `update_check.rs` and `scripts/qemu.sh` (R10)
- [x] 6. API: done in 3 (core routes); interface: not applicable (step 3 of J5b)
- [x] 7. Docs: sowel-subset, memory-budget (thread, TLS, bundle, image sizes before and after),
      decisions D65 to D69, architecture, roadmap (on-box checks, inbox lines, J5b status, history),
      specs index, release.md, README; "No longer true" searched and corrected

## Test plan

Core tests (`crates/core/tests/release.rs`) use a fake `Fetcher` (a map of URL to answer, a body
that can fail after n bytes or report a timeout), a counting `UpdateAdapter`, a fake `Pacing`
whose clock the test moves, and a key pair made at run time.

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| manifest | Nominal manifest of spec 038 R2 | parsed: version, minVersion null, size, sha256, url |
| manifest | `format` 2 | `Unsupported manifest format 2` |
| manifest | Not JSON; `version` "1.2"; `sha256` 63 hex; `size` 0 and above `HEADER_BYTES + SLOT_BYTES`; `url` plain http public | `Invalid manifest: <field>` each |
| manifest | Unknown extra keys, `notes` long | accepted, not kept |
| manifest | Body of 16 KB + 1 | `Manifest too large` |
| releaseUrl | GitHub asset address | `https://github.com/o/r/releases/tag/v0.2.0`; other host: `null` |
| URL rule | https any host; http 192.168.x, 10.x, 172.16-31.x, 127.x, localhost | accepted |
| URL rule | http public IP, http name, ftp, 172.32.x | refused `plain http is refused for <host>` (ftp: invalid) |
| redirects | One and two redirects (absolute, and `/path` relative) | followed, check 200 |
| redirects | Six redirects; a loop | `too many redirects` |
| redirects | 302 without Location | `redirect without Location` |
| redirects | https → http (even LAN) | `redirect from https to http refused` |
| check route | Newer manifest | 200 `VersionInfo` latest, updateAvailable true, releaseUrl; `GET /system/version` the same |
| check route | Manifest equal or older | 200, updateAvailable false, latest set |
| check route | minVersion above running | 200, updateAvailable false |
| check route | 404 | 500 `Update check failed: <url>: 404`; cache unchanged |
| check route | Second check within 60 s | 429 `Please wait <n>s before checking again`; after 60 s (moved clock) accepted |
| check route | A check while another check or a pull holds the ticket | 409 `Update check already in progress` |
| check route | Clock before 2020 (`now` small) | 503 clock message, no fetch made |
| check route | Safe mode | let through (200) |
| check route | Plain handler | 501 |
| check route | No Content-Type, or a body | 200 (no 415) |
| URL rule | http `127.1`, `0x7f.0.0.1`, `010.0.0.1`, `256.1.1.1` | refused |
| check route | GET/PUT on `/system/version/check` | 405 |
| version | Before any check | latest null, updateAvailable false, releaseUrl null (unchanged behaviour) |
| update route | Content-Length 0, no content type / `application/json` | `Reply::UpdatePull` |
| update route | Content-Length > 0, octet-stream | `Reply::Update` (unchanged); other type 415 (unchanged) |
| update route | `content_length` None (chunked) | upload form, as today |
| pull | Nominal: manifest + `.sup` via one redirect | 200 `Installed x.y.z, restarting`; adapter finished once, bytes equal |
| pull | Fresh cache (< 10 min) | manifest not fetched again; older cache: fetched |
| pull | Version not newer | 400 `No update available`, nothing written |
| pull | minVersion too high | 400 `Version a needs b or later first (running c): install b before it` |
| pull | image.size above slot | 413 |
| pull | Content-Length differs from image.size | 400 `Download size differs from the manifest`, `begin` never called |
| pull | Wrong signature in the `.sup` | 400 `Invalid signature`, `begin` never called |
| pull | Image byte changed (header and manifest agree in size, manifest sha256 of the changed file) | 400 `Image does not match its signature`, aborted |
| pull | manifest sha256 of another file | 400 `Download does not match the manifest`, aborted, `finish` never called |
| pull | Body longer than image.size, no Content-Length | 400 (`Update longer than declared` or `Download does not match the manifest`), aborted |
| pull | Connection cut mid-image | 500 `Download failed: connection lost`, aborted; a next pull succeeds from byte 0 |
| pull | Read timeout | 500 `Download failed: timed out`, aborted |
| pull | Total time passed (pacing clock moved past the limit) | 500 `Download failed: took longer than <n> s`, aborted |
| pull | No key | 503, no fetch |
| pull | Pull while a pull/check holds the ticket | 409 `Update already in progress` |
| pull | Running image on probation | 409 spec 032 text, no fetch |
| pull | Memory: a 1 MB image pulled through the counting allocator | peak heap under 32 KB (as spec 032's `update_memory`) |
| pull | Pacing | `pause` called about every 64 KB |
| auto | autoCheck off | `auto_check_ticket` never `Some` |
| auto | On, before 10 min / at 10 min / next at +24 h | None / Some / None until 24 h, then Some |
| auto | On, clock not set or busy | None (postponed) |
| auto | Auto check failure | does not block an API check (no 429 from it) |
| settings | `update.manifestUrl` https, http LAN, 512 chars | accepted; 513 chars, http public, "x" | 400 `Invalid value for update.manifestUrl` |
| settings | `update.autoCheck` "true"/"false"/"" | accepted; "yes" 400 |
| settings | Manifest URL setting used | the ticket's URL is the setting's, else the default |
| settings | Old `settings.json` without the keys | loads as today (unchanged behaviour) |
| upload | Every existing spec 032 test | green; the empty-body ones moved to a declared length |
| URL form | space, `\r\n`, non-ASCII, userinfo `u@h`, `[::1]`, port 0 or 70000, 2049 chars, `Location: //h/x`, `Location: x` | `invalid URL` each |
| pull | Header length + 128 differs from image.size | 400 `Download does not match the manifest`, `begin` never called |
| pull | Header version differs from the manifest's | 400 `Download does not match the manifest`, `begin` never called |
| pull | Body ends early with no error (close-delimited) | 500 `Download failed: connection lost`, aborted |
| pull | Clock not set; safe mode; failing check | 503; let through; 500 `Update check failed` |
| pull | Second pull within 60 s of a failed check | 429, no fetch |
| pull | minVersion equal to the running version | allowed, 200 |
| pull | An upload while a pull holds `Installing` | spec 032's 409 |
| check route | A check during an upload; a body on the check route | allowed; ignored |
| auto | The automatic check finds a newer release | cache filled, the adapter never written |
| HttpFetcher | Request headers seen by the test server | `User-Agent: solite/<v>`, no Cookie, no Authorization |
| HttpFetcher | A captured curl head (`HTTP/1.1 302`, a proxy's `200 Connection established` first) | parsed; `Redirect` |
| HttpFetcher | The real GitHub address (`#[ignore]`) | 404 or a redirect, TLS works |
| host serve | Empty POST with no Content-Length header | the pull (`content_length` `Some(0)`) |

Host tests (`crates/host/src/fetch.rs` `#[cfg(test)]`, a test HTTP server on `127.0.0.1:0` in a
test thread, the core's `Releases` and `Updates` with a run-time key and a temporary `FileAdapter`):

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| HttpFetcher | 200 with Content-Length | `Body` with length and bytes |
| HttpFetcher | 200 without Content-Length (close-delimited) | body to the end |
| HttpFetcher | 302 with Location | `Redirect(Some)` |
| HttpFetcher | 404 | `Status(404)` |
| HttpFetcher | chunked answer | error `chunked answers are not supported over http` |
| HttpFetcher | Server never answers | error after `idle_ms` (short in the test) |
| HttpFetcher | Server stops mid-body (connection closed before Content-Length) | read error `connection lost` |
| pull through real sockets | Nominal with two redirects | 200, slot file equals the image |
| pull through real sockets | Cut connection, wrong hash, wrong signature, version not newer, minVersion too high, manifest too large, download larger than the manifest, slow server | the statuses of the core table; slot not bootable (`boot.json` unchanged) |

Host end-to-end (`crates/host/tests/update.rs`): the keyed host, a test server, `PUT /settings`
with `update.manifestUrl`, `POST /system/version/check` 200 with `updateAvailable` true, `GET
/system/version` the same, empty `POST /system/update` 200 and the `written to slot` line; an empty
POST with a manifest not newer 400.

Firmware: no tests (the logic is the core's); QEMU's R10 check in `scripts/qemu.sh`.

## Validation

- [x] Validate command green, QEMU included
- [x] Agent review done, blocking findings fixed
- [x] Demo on the Mac by the tests (the user away); the box checks listed in the roadmap, run by the main session
