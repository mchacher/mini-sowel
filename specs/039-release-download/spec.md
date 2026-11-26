# Spec 039: The box checks for and downloads a release itself

## Context

Step 2 of 3 of the roadmap's J5b ("the box updates itself from a GitHub release"). Step 1 (spec
038, D63, D64) makes the CI publish, as the assets of a GitHub release, `solite-<version>.sup` (the
signed update file of spec 032) and `manifest.json` (spec 038 R2), with
`https://github.com/mchacher/solite/releases/latest/download/manifest.json` as its stable address.
Today a box is updated only by uploading the `.sup` in the body of `POST /api/v1/system/update`
(D53), and `GET /api/v1/system/version` always answers `latest: null`, `updateAvailable: false`.

This spec makes the box (and the Mac host) **read the manifest, say whether a newer release
exists, and download and install it on request**, through the same installation as the upload
(signature, streamed hash into the inactive slot, no downgrade, probation, rollback). It mirrors
Sowel's version check and self-update (`src/core/version-checker.ts`, `src/api/routes/system.ts`
of the Sowel repository): `GET /system/version` answers the last check, `POST
/system/version/check` checks now, `POST /system/update` with no body pulls the latest release.
The true north asks a box that runs for a month alone and a release that reaches it without a
file from the user: this is the box side of that.

Decided in an autonomous run (the user away): every choice below is recorded here and in
decisions D65 to D69, for the user's review.

## Goals

1. A check: the box fetches the manifest (a setting, default the GitHub address), compares it with
   its running version and fills Sowel's `latest`, `updateAvailable` and `releaseUrl`.
2. A download: `POST /api/v1/system/update` with an empty body downloads the `.sup` of the latest
   release and installs it through spec 032's pipeline, streamed, bounded in size and time, never
   held whole in memory; the body form is unchanged.
3. TLS on the box with ESP-IDF's certificate bundle, one session at a time, the clock required, on
   a thread whose stack is in internal RAM and measured.
4. The engine, the radio and the API keep working during a check and a download.
5. Every case is a Mac test: a test HTTP server in the tests serves manifests and signed files,
   with redirects, cuts, wrong hashes, wrong signatures, old versions, slow answers.

## Out of scope

- The interface: a "Check for updates" and "Install" button, the release notes shown (step 3 of
  J5b, its interface part; an inbox line).
- Resuming a cut download (HTTP `Range`): a cut download starts over at the next attempt (an
  inbox line).
- Key rotation, a revocation list (step 3 of J5b, D64).
- Sowel's `system.update.available`, `system.update.progress` and `system.update.error` events
  (`EngineEvent` of `src/shared/types.ts`): the pull answers when it is over (an inbox line).
- Installing automatically: the automatic check only reports (R6), never installs.
- `scripts/box-session.sh`: not changed by this spec.
- Running a check on the box in this spec's validation: the box is checked by the main session
  afterwards ("How the user will check it").

## Requirements

### R1: The manifest and its rules

- The manifest is spec 038 R2's JSON. It is fetched whole into memory, at most
  `MAX_MANIFEST_BYTES` (16 KB; a manifest is under 1 KB plus its notes); more: the check fails
  `Manifest too large`.
- Read: `format` must be `1` (`Unsupported manifest format <n>`); `version` an `x.y.z` of spec
  032's `Version` (`Invalid manifest: version`); `minVersion` `null`, absent or an `x.y.z`
  (`Invalid manifest: minVersion`); `image.size` an integer from 129 (a header and one byte) to
  `HEADER_BYTES + SLOT_BYTES` (128 + 4 MB, spec 032) (`Invalid manifest: image.size`); `image.sha256`
  64 hex characters, either case (`Invalid manifest: image.sha256`); `image.url` a URL the rule of
  R3 accepts, at most `MAX_URL_CHARS` (2048) characters (`Invalid manifest: image.url`). `notes`,
  `releasedAt` and `image.name` are not kept (nothing serves them in this spec). Unknown keys are
  ignored (a later format 1 may add some).
- What a check keeps (the **cache**, one entry, bounded by the fields above): `latest` (the
  manifest's version), `minVersion`, the image's `url`, `size` and `sha256`, `releaseUrl`, and the
  monotonic time of the check. A failed check keeps the previous entry.
- `releaseUrl`: when `image.url` is `https://github.com/<owner>/<repo>/releases/download/<tag>/<name>`,
  it is `https://github.com/<owner>/<repo>/releases/tag/<tag>` (the release page, Sowel's
  `html_url`); for any other address, `null`.

### R2: `GET /api/v1/system/version` and `POST /api/v1/system/version/check`

- `GET /api/v1/system/version` (unchanged route, Sowel's `VersionInfo` without its two Docker
  fields, D53) answers from the cache, never from the network: `current` the running version;
  `latest` the cached version or `null` before any successful check; `updateAvailable` true when
  `latest` is newer than `current` **and** `minVersion` is `null` or not newer than `current` (a
  release the box cannot install directly is not "available"); `releaseUrl` the cached one or
  `null`.
- `POST /api/v1/system/version/check` (Sowel's route, new in Solite), behind the token, no body
  read (a body is ignored, as Sowel's): checks now and answers 200 with the `VersionInfo` above.
  Refusals, in order: the clock not set (R5) 503 `The clock is not set yet: checking a release
  needs the date`; another check or a download in progress 409 `Update check already in progress`;
  a check started less than `MIN_CHECK_INTERVAL_MS` (60 s) ago, on the monotonic clock, 429
  `Please wait <n>s before checking again` (Sowel's message, its interval 10 s widened to 60 s for
  the box's TLS session; Sowel's `Retry-After` header is not sent: Solite's `Response` carries no
  header, the seconds are in the message); the check fails (network, status, manifest) 500
  `Update check failed: <reason>` (Sowel's 500 with its `error`), logged `update: check failed:
  <reason>`. A successful check logs `update: check: latest <x.y.z>, running <x.y.z>`.
- `<reason>` texts: `<url>: <status>` for an HTTP status other than 200 and the redirects (a 404
  before any release exists reads `...manifest.json: 404`), `too many redirects`, `redirect without
  Location`, the refusals of R3 (`plain http is refused for <host>`, `redirect from https to http
  refused`), `timed out`, `took longer than <n> s`, the adapter's connection error, and R1's
  manifest messages.
- Safe mode (D58): the check is a `POST` that changes nothing stored; it is let through in safe
  mode like `/system/update` (a box in safe mode must still be able to find a release that fixes
  it): the exceptions of D58's 503 gate become the account's routes, `/system/update` and
  `/system/version/check` (refines D58, in D65).
- Through the core's plain handler (tests, tools): 501 `Updates need the streaming handler`, as the
  update route.

### R3: Where the box may fetch from

- **The URL's form** (every URL: the setting, `image.url`, a redirect's `Location`): at most
  `MAX_URL_CHARS` (2048) characters, visible ASCII only (`!` to `~`: no space, no control
  character, no `\r` or `\n`); the scheme `http` or `https` without case; then `//`, a host of
  letters, digits, `-` and `.` (no userinfo `user@`, no IPv6 `[...]`), an optional `:port` of 1 to
  65535, then nothing or a path starting with `/` (query and fragment allowed as visible ASCII; the
  fragment is not sent). Anything else: `invalid URL` (in a manifest: `Invalid manifest:
  image.url`; in the setting: R7's 400).
- `https://` to any host. `http://` only to an IPv4 literal of `10/8`, `172.16/12`, `192.168/16`
  or `127/8`, or to `localhost`, any port (a test server on the local network, as the on-box check
  uses). A plain `http://` name or public address is refused: `plain http is refused for <host>`.
  The file is signed and its hash checked, so its integrity does not rest on TLS; TLS keeps a
  public network from seeing or blocking which version a box takes, and a name could resolve to
  the Internet, hence the literal-only rule (decision D66).
- Redirects: `301`, `302`, `303`, `307`, `308` are followed with a `GET`, at most `MAX_REDIRECTS`
  (5; GitHub's asset takes two: `releases/latest/download/...` to `releases/download/<tag>/...`, then
  to `objects.githubusercontent.com` or `release-assets.githubusercontent.com`). The `Location` is an
  absolute URL, or a path starting with a single `/` taken on the current origin (`//host/...` and
  a relative path without `/` are `invalid URL`); the result passes the rules above, and a redirect
  never goes from `https` to `http`. A sixth: `too many redirects`.
- No other method, no request body, no cookie, no authentication header is ever sent; the request
  carries `User-Agent: solite/<version>` (GitHub refuses requests without one) and `Accept: */*`.

### R4: `POST /api/v1/system/update` with an empty body: the pull

- **Which form.** The route takes two forms. With a body (a declared `Content-Length` above 0, or
  a chunked body): spec 032's upload, unchanged, `application/octet-stream` required (415
  otherwise). With an empty body (`Content-Length: 0`, or neither `Content-Length` nor
  `Transfer-Encoding`): **the pull**, whatever its `Content-Type` (Sowel's clients send none, or
  `application/json`). One route for the interface and for Sowel's clients (D65, replaces D53's
  "takes the signed file as its body").
- **Order of the pull's checks**, each refusal a whole answer, nothing written: the gate (401,
  403) and the safe mode's exception as today; the clock not set 503 (R5's message, `checking a
  release needs the date`), these three under the home's lock; then, on the caller's thread, the
  lock released: a check or another pull in progress 409 `Update already in progress` (Sowel's
  text); no key 503 `Updates are not configured: this build has no update key`; an installation in
  progress, an update waiting for the restart, the running image on probation: spec 032's 409s,
  **before any network access** (the pull takes spec 032's state to `Installing` at this point and
  keeps it through the manifest and the download, so an upload meanwhile gets spec 032's 409);
  then the manifest: the cache when its check is younger than `FRESH_MS` (10 minutes), else a check
  now (R2's rules and messages, `Update check failed: <reason>` 500; it refreshes the cache and
  counts for R2's 60 s; a check through the API or a pull less than 60 s ago that failed makes the
  pull answer R2's 429 rather than open a new session); `latest` not newer than the running version
  400 `No update available` (Sowel's text); `minVersion` newer than the running version 400
  `Version <latest> needs <minVersion> or later first (running <current>): install <minVersion>
  before it`; `image.size` above the slot (header plus `capacity`) 413 `Image too large for the
  update slot`.
- **A release behind a `minVersion`.** The manifest names only the latest release, so a box below
  its `minVersion` cannot fetch the intermediate itself: the user downloads the intermediate
  release's `.sup` from its GitHub page and uploads it (the body form), then pulls the latest. A
  pull of a given version is an inbox line.
- **The download.** `image.url` is fetched with R3's rules. An answer whose `Content-Length` is
  given and differs from `image.size`: 400 `Download size differs from the manifest`, before any
  byte is written. The body is read through a reader bounded to `image.size` + 1 bytes, which
  hashes every byte (SHA-256) and counts them, and is given to spec 032's installation as its
  source. The reader turns an end of the body before `image.size` bytes into an error (`connection
  lost`) and keeps the reason of any read error, so a cut, a timeout and an early close are
  download failures (below), never spec 032's `Truncated update`. Right after the header, **before
  `begin`**, the installation also checks that 128 + the header's image length equals `image.size`
  and that the header's version equals the manifest's `version`: otherwise 400 `Download does not
  match the manifest`, nothing written. Then spec 032's checks (signature, version newer, slot) and
  the image hashed into the inactive slot 4096 bytes at a time, never held whole. After the image,
  and **before the slot is made the next boot**, the reader's count must equal `image.size` and its
  hash `image.sha256`: otherwise the slot is dropped and the answer is 400 `Download does not match
  the manifest`. A body longer than `image.size` gives the installation its "one byte more": 400
  `Update longer than declared`, the slot dropped.
- **Bounds.** Size: `image.size`, itself at most the 4 MB slot plus the header (above). Time: an
  idle timeout of `DOWNLOAD_IDLE_MS` (30 s) on each read (the adapter's), and a total of
  `DOWNLOAD_TOTAL_MS` (10 minutes) checked between reads on the monotonic clock. A check alone:
  `CHECK_IDLE_MS` 15 s and `CHECK_TOTAL_MS` 30 s.
- **A failure mid-download** (the connection drops, a read error, a timeout, the total time
  passed, the body ending early): the slot is dropped exactly as a truncated upload (spec 032: not
  bootable, the boot record unchanged), and the pull replaces the installation's answer by 500
  `Download failed: <reason>` whenever the reader recorded a failure (`connection lost`, `timed out`,
  `took longer than 600 s`, the adapter's error), logged `update: download failed: <reason>`. The
  next attempt starts over from byte 0 (resuming is out of scope).
- **The answer** is spec 032's: 200 `{"success": true, "message": "Installed <x.y.z>, restarting"}`,
  then, on the box, the restart and the probation of spec 032 (D54); the host prints its line of
  spec 032. The request waits for the whole download (Sowel answers at once and reports progress by
  events; Solite has none: a difference of value, D65). A refusal that leaves body bytes unread
  closes the connection (the upload's); the empty form has no body, so the box keeps the
  connection open after a refused pull.
- **The watchdog** (D58): the download thread is not subscribed to the task watchdog; it blocks in
  its reads, so the idle tasks run. Between reads it hands the CPU back for a moment every 64 KB
  (`backup::Pacing::pause`, as the backup's derivation), so a fast local network never starves the
  idle task of core 0.

### R5: The clock

A check and a pull need the wall clock set (certificate dates): before `CLOCK_SET_AFTER_MS` (2020,
the core's existing rule) both are refused 503 with R2's message, before any network access. The
automatic check (R6) waits for it silently.

### R6: The automatic check

- Setting `update.autoCheck` (Solite's own key, R7): `"true"` turns it on; unset, `""` or
  `"false"` leave it off. **Off by default** (decision D67): a box contacts the Internet only when
  its user asked, until the TLS session's memory has been measured on the box (an inbox line
  revisits the default).
- On: a check runs 10 minutes after the start (`AUTO_FIRST_AFTER_MS`, the clock and the network
  set by then), then every 24 hours (`AUTO_EVERY_MS`) on the monotonic clock, when the clock is set
  and no check or pull is running (a busy or unset clock postpones it to the next minute). It runs
  on its own thread (R8), fills the cache like R2 and logs the same lines; it never installs.
- Its failures are logged and wait for the next day; they do not count for R2's 60 s interval
  (only a check asked through the API or a pull does).
- Turning it on after the first 10 minutes checks at the next minute. On the box it is asked by the
  engine's loop, so a box in safe mode (engine stopped) makes no automatic check.

### R7: Settings

Two keys join the settings of spec 026 (Sowel's flat map of strings), Solite's own (Sowel has no
such keys), listed as deviations in `docs/sowel-subset.md` (decision D67):

- `update.manifestUrl`: a URL of R3's rule, at most 512 characters; unset or `""`: the default,
  `https://github.com/mchacher/solite/releases/latest/download/manifest.json`. Another value: 400
  `Invalid value for update.manifestUrl`.
- `update.autoCheck`: `"true"` or `"false"` (R6). Another value: 400 `Invalid value for
  update.autoCheck`.

`settings.json` gains them as any key; the previous release reads such a file, drops the two
unknown keys with its warning and writes the file again (spec 026's lenient read): after a
rollback the two settings are lost, nothing else (D54's rule: readable by the previous release).

### R8: Where it runs on the box

- **Threads.** A check (API or automatic) and a pull run on a thread started for the job, its stack
  in internal RAM, pinned to core 0 (`job_thread.rs`, as D59 and D60): `update-check` and
  `update-pull`, **20 KB** each (`FETCH_STACK_BYTES`), one at a time (R2's 409). The API worker that
  took the request lends it and waits, as for the upload; the automatic check's thread is not
  waited for. Each logs `<name>: thread stack <n> of 20480 bytes never used` at its end. The size,
  by reasoning (decision D68): ESP-IDF's `esp-tls` runs mbedTLS in the calling task; ESP-IDF's
  HTTPS examples give such a task 8 KB, its handshake (an ECDHE key exchange, an RSA or ECDSA
  chain check against the bundle) being the deepest part; the installation's Ed25519 check takes
  9.2 KB (D60) and runs after the handshake returned, never nested in it; 20 KB covers the larger of
  the two with the request's frames and a margin. QEMU measures the pull's core part on that
  thread (R10); the TLS handshake's share is measured on the box (the on-box checks).
- **TLS.** esp-idf-svc's HTTP client (`EspHttpConnection`), redirects not followed by it (the core
  follows them, R3), ESP-IDF's certificate bundle attached (`esp_crt_bundle_attach`), the bundle
  set to its **common** list (`CONFIG_MBEDTLS_CERTIFICATE_BUNDLE_DEFAULT_CMN`, the certificate
  authorities of most of the web, GitHub's included) rather than the full one, to keep the image
  smaller. A receive buffer of 4096 bytes (GitHub's redirect `Location` to its asset host is a
  signed address of several hundred characters, over the client's 512-byte default) and a send
  buffer of 1024 (both at or under `CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL`, so in internal RAM:
  budgeted with the thread, about 25 KB of internal RAM during a check or a pull; the worst overlap
  allowed, an automatic check, an upload and a backup at once, is 20 + 16 + 10 KB of stacks, written
  in `docs/memory-budget.md`). One TLS session at a time (the 409s above), its record buffers in PSRAM
  (`CONFIG_MBEDTLS_EXTERNAL_MEM_ALLOC`, already set). The image's size before and after, and the
  bundle's share, are recorded in `docs/memory-budget.md` from the validate run's size lines.
- **The engine keeps working**: the check and the download run on their own thread and never hold
  the home's lock while they wait on the network; the API keeps its 4 workers (one waits for the
  pull, the 503 `Busy` beyond 4 unchanged), the radio loop and the saver are not touched. The flash
  writes go through the OTA thread as for the upload.
- The image QEMU runs has no network: QEMU proves the thread and the core's pull, the box proves
  TLS.

### R9: Where it runs on the Mac

The host implements the fetch adapter: `http://` with the standard library (`TcpStream`, the
head parsed with `httparse`, already a dependency; a body delimited by `Content-Length` or by the
connection's end; a chunked answer refused `chunked answers are not supported over http`);
`https://` through the system's `curl` program, run as `curl -q -sS -i -g --http1.1 --proto =https
--max-redirs 0 --connect-timeout <idle s> --speed-limit 1 --speed-time <idle s> -H "User-Agent:
solite/<version>" -- <url>` (`-q` ignores `~/.curlrc`, so no `-L` or proxy from it; `-g` so `[ ] { }` in a URL are never expanded; curl prints the server's `Transfer-Encoding: chunked` header but writes the body decoded, so for curl's output it is ignored and the body read to the end; HTTP/1.1 so
the head is one `httparse` reads; no redirect followed by curl; the URL already passed R3, which
has no userinfo), its output's head parsed the same way, a leading `HTTP/1.x 200 Connection
established` head of a proxy skipped, a non-zero exit before the head, or after a body with no declared length, an error with curl's message;
the child is killed when the body is dropped. Without `curl`: `https needs the curl program`. The
head parser is tested on a captured curl head; one test against the real GitHub address is
`#[ignore]` (network). No TLS library is added to the workspace (decision D69). The host runs the check and the
pull on the API worker that took the request (a Mac thread's stack is large), and the automatic
check on a thread it starts.

### R10: QEMU

The image QEMU runs (feature `sim`) adds to its update stack check (`update_check.rs`, D60) one
pull, on an `update-pull` thread of `FETCH_STACK_BYTES`, from an in-memory fetch adapter (a manifest,
one redirect, a signed file made at boot): its status must be 200, and `scripts/qemu.sh` fails when
fewer than 8192 of its 20480 bytes stay unused (the room left for the TLS handshake, which QEMU
cannot run).

## Living spec delta

### docs/living-spec/README.md, "Updates itself from a signed file"

- Changed: the routes' bullet: `GET /api/v1/system/version` answers the last check's `latest`,
  `updateAvailable` and `releaseUrl` (R2), no longer always `null`/`false`; `POST
  /api/v1/system/update` takes the signed file as its body **or** an empty body that pulls the
  latest release (R4).
- New bullet "**From a release**": R1 to R6 in business words (the manifest, the check route and
  its refusals, where the box may fetch from, the pull and its bounds and failures, the clock, the
  automatic check, off by default).
- Changed: the box's bullet: the `update-check` and `update-pull` threads of 20 KB, TLS with the
  common certificate bundle, one session at a time (R8); the host's bullet: http by itself, https by
  `curl` (R9).

### docs/living-spec/README.md, "Keeps the home's settings"

- Changed: "six keys" → eight, with `update.manifestUrl` and `update.autoCheck` (R7).

### docs/living-spec/README.md, "Publishing a release"

- Changed: the manifest's paragraph says the box reads it (spec 039), no longer "step 2 reads it"
  as future.

### docs/living-spec/README.md, "Stays within a memory budget" and the QEMU paragraph

- New: the pull's stack check in QEMU (R10).

### No longer true

Search: `latest`, `updateAvailable`, `always null`, `takes the signed file`, `takes no body`,
`release feed`, `step 2`, `six keys`, `pulls a release`, `D53` in `docs/` (roadmap and decisions
excepted), `README.md`, `CLAUDE.md`, `.claude/`:

- `docs/living-spec/README.md` "Updates itself from a signed file": `"latest": null,
  "updateAvailable": false` and "a deviation from Sowel's route, which takes no body and pulls a
  release itself" → R2 and R4.
- `docs/living-spec/README.md` "Keeps the home's settings": "six keys" → eight keys.
- `docs/sowel-subset.md`: "`latest` (always `null`), `updateAvailable` (always `false`)" and
  "takes the signed file as its body ... Sowel's route takes none" → the two forms (D65); the route
  `POST /api/v1/system/version/check` added to the routes table; the two settings keys added to the
  settings table and to the deviations.
- `crates/core/src/api/system.rs` module comment "`latest` always null" → the cache.
- `docs/release.md`, `docs/living-spec/README.md` "Publishing a release": "the box will read",
  "step 2 reads it" → present tense.
- `docs/living-spec/README.md` "Updates itself from a signed file": "on the box a missing one is
  411 (a chunked upload too)" → on the update route a missing length with no chunked body is the
  pull; a chunked upload stays 411 on the box; the backup restore keeps its 411
  (`firmware/src/http_server.rs` changes for the update route only).
- `README.md` routes table ("the signed update file as the body"; the check route missing) and its
  error list (`Update check failed: ...`, `Download failed: ...`, `No update available` missing).
- `README.md` and `docs/living-spec/README.md` "Supervises its engine", `docs/sowel-subset.md`: the
  safe mode's exceptions gain `POST /system/version/check`.
- Code comments "six keys" / "six values": `crates/core/src/settings/mod.rs`,
  `crates/core/src/api/settings.rs`, `crates/core/src/home/mod.rs`, `crates/core/src/home/cursor.rs`
  → eight.
- `crates/core/tests/memory_worst_case.rs`: "the six settings at their longest" → eight, the
  512-character `update.manifestUrl` included.
- Part B (firmware and docs), found and corrected: `docs/architecture.md` "Home settings" row "the
  six keys" → eight; its "Updates" row (no release code, "the route streamed from the socket" only)
  and "Firmware" row (the update thread only, the sim image's check only, `pair-watch` only the
  pairing) → the release code, `fetch.rs` in the host and the firmware, the two new threads, the
  pull check and the automatic check's ask; `README.md` "A box already running Solite" (the upload
  as the only way) → the pull; `docs/living-spec/README.md` QEMU paragraph and "Stays within a
  memory budget" (the update check was three installations only) → the pull's check, its 8 KB
  margin; the box bullet (one `update` thread) → the two new threads and TLS; `docs/memory-budget.md`
  "TLS" row (a plain estimate) → the HTTP client's use, and a flash section (the image's size had
  no figure). The README has no routes table or error list (those are the living spec's, done in
  part A).
- Any other sentence the implementer's search finds, listed in the report and here at the close.

## Minor items of the spec review left as they are

- Round 2, applied in place: the check route is answered before `dispatch`'s JSON check (no
  `Content-Type` needed, any body ignored); in the bare safe mode (D58, no file read) the check and
  the pull use the default manifest URL; an `http://` IPv4 literal is exactly four decimal octets
  of 0 to 255 (no `127.1`, no octal or hex); the living spec's "a body sent without a length is
  answered 411" (box) and its error list are corrected with the delta.

- The 16 KB manifest bound and spec 038's unbounded notes: an inbox line asks `release.sh` to
  refuse a manifest over 12 KB; `docs/release.md` says it.
- The interface's 170 KB gzip budget is unaffected (no interface change).
- The scope is large; R6 is kept (a small core function and one call per adapter).

## Acceptance criteria

- [x] `POST /system/version/check` against a test server serving a newer manifest answers 200 with
      `latest`, `updateAvailable: true` and `releaseUrl`; `GET /system/version` then answers the same.
- [x] `POST /system/update` with an empty body installs the release's `.sup` (host: written to the
      inactive slot, its line printed), through one and two redirects.
- [x] Each refusal of R2, R3, R4, R5 answers its status and message, and leaves the inactive slot
      not bootable (the boot record unchanged): a cut connection, a wrong hash, a wrong signature,
      a version not newer, a `minVersion` too high, a manifest too large, a download larger than
      the manifest says, a slow server (idle and total timeouts), too many redirects, plain http to
      a public address, an https-to-http redirect, the clock not set.
- [x] The upload form of spec 032 answers as before: its tests green, the few that sent an empty
      body (`crates/core/tests/update.rs` 415 and `Reply::Update` cases, `supervisor_api.rs`'s
      safe-mode update case) moved to a declared non-zero length, and the matching empty-body cases
      added as pulls.
- [x] The settings `update.manifestUrl` and `update.autoCheck` are accepted, checked and kept.
- [x] The automatic check runs when on and due, and never when off; it never installs.
- [x] `sh scripts/validate.sh` green, QEMU included, with the pull's stack check (R10).
- [x] The memory budget, decisions D65 to D69, the roadmap (on-box checks, inbox lines), the
      specs index and the README updated.

## Edge cases

- A manifest that is not JSON, or JSON of another shape: `Update check failed: Invalid manifest: <field>`.
- A manifest fetched in pieces, or an answer without `Content-Length`: read to the end, bounded at
  16 KB + 1.
- A redirect loop: `too many redirects` at the sixth.
- The server's `Content-Length` larger than `image.size`: refused before any write; no
  `Content-Length` and a longer body: the count catches it (R4).
- The clock jumps during a download: the total time is on the monotonic clock.
- Two empty POSTs at once: the second 409 `Update already in progress`; a check during a pull: 409.
- A check during an upload of a file: allowed (the upload holds no TLS session); a pull during an
  upload: spec 032's 409 `Update already in progress`.
- The setting `update.manifestUrl` pointing at a manifest whose `image.url` is plain http to a
  public address: refused by R3.
- A `minVersion` equal to the running version: allowed (not newer).
- The thread cannot start on the box (no internal block of 20 KB): 503 `Busy, try again`, as D59 and
  D60.
- Safe mode: the check and the pull are let through (R2).

## Sowel contract

- `VersionInfo` of `src/core/version-checker.ts` (current, latest, updateAvailable, releaseUrl;
  `dockerAvailable` and `composeManaged` left out, D53).
- `POST /api/v1/system/version/check` and `POST /api/v1/system/update` of `src/api/routes/system.ts`:
  the check's 429 message, its 500 with `error`, the update's 409 `Update already in progress` and
  400 `No update available`. Differences of value (D65): the check's interval 60 s (Sowel 10 s), no
  `Retry-After` header; the update route also takes a body (the upload of spec 032); the pull answers
  when it is installed, not at once with progress events; its success message is spec 032's;
  `updateAvailable` is false when `minVersion` is above the running version (Sowel's `isNewer`
  has no such rule).
- Settings keys `update.manifestUrl`, `update.autoCheck`: Solite's own, deviations (D67).
- No event added (`system.update.*`: an inbox line).

## How the user will check it

On the Mac (the demo of this run): the host built with a test key, a test server in the tests, and
`cargo run -p solite-host -- --sim scenarios/garage.json --serve` with `update.manifestUrl` set to a
local manifest: `POST /system/version/check`, `GET /system/version`, then an empty `POST
/system/update`, and the host's `update: ... written to slot` line.

On the box (run by the main session afterwards, listed in the roadmap's "Tests to run with the box
in hand"): (1) a Mac HTTP server on the LAN (`python3 -m http.server`) serving a manifest and a
`.sup` signed with a test key (`solite-host --update-sign`), `update.manifestUrl` set to it: the
check, the pull, the restart and the probation; the `update-check` and `update-pull` stack lines and
the `heap:` line's internal RAM minimum during the pull. This needs the box's image built with that
test public key, which is a **USB flash: the user's explicit yes** (D6, D55), and the box flashed
back afterwards with an image of the release key, or it refuses real releases; (2) the real GitHub
address (default setting; the repository must be public): until a release exists GitHub answers
404 at once (no redirect), `Update check failed: ...manifest.json: 404`, after the TLS handshake,
which proves the certificate bundle and the TLS memory; once a release exists, the redirects too;
the stack line read again.
