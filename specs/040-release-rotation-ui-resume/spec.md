# Spec 040: Key rotation, the update card, and a cut download resumed

## Context

Step 3 of 3 of the roadmap's J5b ("the box updates itself from a GitHub release"). Step 1 (spec
038, D63, D64) publishes a signed release and its `manifest.json`; step 2 (spec 039, D65 to D69)
makes the box check the manifest and pull the `.sup` it names. Three things are left, and this spec
builds them:

1. **Key rotation.** D64 left it here: the public key is compiled into the image
   (`SOLITE_UPDATE_PUBLIC_KEY`, committed as `release/update-key.pub`) and the update file's format
   1 has no field for a next key. A rotation is a **bridging release**: built with the new public key
   compiled in and signed with the old private key, which the running (old) image accepts; once it
   is installed the box trusts the new key only. Spec 032's format already allows it (the key is in
   the image, not in the file); what is missing is the release tooling, its checks, the procedure and
   the compromised-key case in `docs/release.md`.
2. **The interface.** The Settings page's "Firmware update" card (spec 032) only uploads a file.
   The API now checks and pulls a release (spec 039): the card gets the check, the latest version, a
   link to its notes, "Update to x.y.z", the wait for the restart, and the automatic check's setting.
3. **A cut download resumed.** Spec 039 starts a cut download over at the next attempt. A Wi-Fi blip
   during the 35 s pull of a 2 MB file should not waste it: the pull asks for the rest with an HTTP
   `Range` request and continues the same installation.

It also settles two observations from the first run of spec 039 on the box (see "Observations from
the box").

Decided in an autonomous run (the user away): every choice is recorded here and in decisions D70 to
D72, for the user's review.

## Goals

1. A maintainer rotates the signing key with one bridging release that `scripts/release.sh` and
   `release.yml` make and check; a normal release is unchanged.
2. `docs/release.md` says what a leaked key lets an attacker do, what the maintainer does, and what is
   not protected.
3. From the Settings page a user checks for a release, reads its notes (on GitHub), updates with one
   confirmed click, waits while the box installs and restarts, logs in again and sees the new version.
4. A download cut mid-file resumes from the byte written, at most a few times, never installing a file
   that was not verified whole.

## Out of scope

- Several public keys per build, a revocation list (D64's alternatives; not needed by a bridge).
- A manifest field for a bridge (the box needs none: R1.5).
- Exposing the release notes through the API (R4.2: a link to the release page instead).
- Sowel's `system.update.*` events and a progress figure during the pull (inbox line kept).
- Restarting a pull from byte 0 inside the same request (R6.5: the pull fails cleanly and the next one
  starts over).
- Running the rotation or the resume on the box in this spec's validation (listed for the main
  session, "How the user will check it").

## Requirements

### R1: The bridging release (key rotation)

1. **The declaration.** A bridging release is declared by a committed file
   `release/update-key.bridge`, in the release's pull request, beside the new `release/update-key.pub`:

   ```
   # A bridging release (docs/release.md): the image carries release/update-key.pub,
   # the file is signed with the private key of the public key below.
   version 0.3.0
   previous 0123...cdef        (64 hex characters: the old public key)
   ```

   Lines starting with `#` and blank lines are ignored; `version` and `previous` each exactly once;
   anything else: refused `release/update-key.bridge: <reason>`. The file is read from the release's
   tree (`$SOLITE_RELEASE_TREE/release/update-key.bridge`, the repository by default; `--bridge-file
   <path>` overrides it, for the tests).
2. **`scripts/release.sh`, normal mode** (no bridge file): unchanged. `--next-key-file` or
   `--next-key-env` given without a bridge file: refused `--next-key-* is for a bridging release only
   (release/update-key.bridge)`.
3. **Bridge mode** (the file exists), every refusal before the build, nothing written:
   - its `version` must equal the release's version, else `release/update-key.bridge is for <v>, not
     <version>: a bridge is one release; remove the file after it (docs/release.md)`;
   - `previous` must be 64 hex characters and differ from `release/update-key.pub`, else
     `release/update-key.bridge: previous is not 64 hex characters` / `... previous is the current
     public key: nothing to rotate`;
   - in a signing run (not `--build-only`), the signing key (`--key-file` / `--key-env`, as today) must
     match `previous` (the probe of spec 038, verified against `previous`), else `the signing key does
     not match the bridge's previous key: nothing is published`;
   - the **next key** (`--next-key-file <path>` or `--next-key-env <NAME>`, the same rules as the
     signing key: outside a git tree, read with `printenv` then unset before the script assigns
     anything, 64 hex characters) is required, else `a bridging release needs the next key
     (--next-key-file or --next-key-env): boxes must not move to a key nobody holds`; it must match
     `release/update-key.pub` (a probe signed with it and verified against the committed public key),
     else `the next key does not match release/update-key.pub: nothing is published`. The next key
     never signs a published file;
   - `--build-only` checks the file's form, version and `previous`, takes no key (unchanged rule; a
     `--next-key-*` with it is refused like a key: `--build-only takes no key: the signing is the next
     step (--prebuilt)`).
   - Every comparison with "the committed public key" above and in R1.4 uses the public key file the
     script was given (`--public-key-file`, `release/update-key.pub` by default), never the literal
     path.
4. **What a bridge produces**: the image is built with `release/update-key.pub` (the new key, as
   always) and the `--prebuilt` check is unchanged (the image carries the committed public key). The
   `.sup` is signed with the signing key, then `--update-verify` must accept it against `previous`
   **and refuse it** against `release/update-key.pub` (a bridge that verifies with the new key is not
   a bridge: `the bridge's file verifies with the new key: nothing is published`). The script prints
   `release: key bridge: signed with <previous8>, the image carries <pub8>`. The key's text checks of
   spec 038 cover both keys (output and log). The files and their names are a normal release's.
5. **No manifest field.** A box accepts the bridge by its signature (its compiled key, the old one) and
   from then on trusts the key its new image carries; it needs nothing in the manifest. The release
   notes' section says it is a bridging release (a sentence the procedure asks for, not checked).
6. **`release.yml`**: the `sign` job, when `release/update-key.bridge` exists in the tagged tree,
   requires the environment secret `SOLITE_UPDATE_KEY_PREVIOUS` (the old private key) as well as
   `SOLITE_UPDATE_KEY` (by then the **new** private key) and runs `release.sh ... --key-env
   SOLITE_UPDATE_KEY_PREVIOUS --next-key-env SOLITE_UPDATE_KEY`; without the file, the command is the
   current one. An empty `SOLITE_UPDATE_KEY_PREVIOUS` with the file present: `::error::` and no
   release. Both secrets are read by the `sign` job only, in the steps that read `SOLITE_UPDATE_KEY`
   today. The `firmware` job is unchanged (it builds with the committed public key). **The dry run**,
   when `release/update-key.bridge` exists (a maintainer dry-runs a merged bridge before tagging it),
   makes a second throwaway key and its own bridge file (the tree's version, the first throwaway
   public key as `previous`) in its temporary folder and runs the script with `--key-file <first>`,
   `--next-key-file <second>`, `--public-key-file <second's public key>` and `--bridge-file <its
   file>`; it still reads no secret and publishes nothing. Without the file, the dry run is unchanged.
7. **The procedure** (`docs/release.md`): (1) make the new key (`--update-keygen`, outside the
   repository); (2) in GitHub, just before tagging the bridge: `SOLITE_UPDATE_KEY_PREVIOUS` ← the old
   private key, `SOLITE_UPDATE_KEY` ← the new one (a normal tag in between fails safely: the probe
   finds the new private key against the old committed public key); (3) a release pull request: the new public key in `release/update-key.pub`,
   `release/update-key.bridge` with the release's version and the old public key, a notes section that
   says "bridging release"; (4) tag as usual; (5) in the next release's pull request, remove
   `release/update-key.bridge` (the script refuses to reuse it), then delete
   `SOLITE_UPDATE_KEY_PREVIOUS`; keep the old key offline until every box you care about is past the
   bridge (a box that rolled back from it needs it again: R1.8).
8. **What a box does across a bridge**: on the old image it accepts the bridge (old key) and boots it
   on probation; confirmed, it trusts the new key only: the old key's files are refused `Bad
   signature` (spec 032's text). A bridge that fails its probation rolls back to the old image, which
   still trusts the old key: such a box takes the bridge's `.sup` again (upload or pull while the
   bridge is the latest release), never a later release signed by the new key. A box that skips the
   bridge (below its version, offline) is refused every later release (signature) and needs the
   bridge's `.sup` uploaded by hand, or a USB flash. The manifest names only the latest release, so
   once a newer release is out, the bridge is fetched from its GitHub page (spec 039's `minVersion`
   path). The maintainer can set the release after a bridge to `--min-version <bridge>` so that a box
   below it is told to take the bridge first (spec 039 R4's message) rather than failing on the
   signature.

### R2: The compromised-key case (`docs/release.md`)

A section "A leaked key" replaces D64's short bullet and says plainly:

- **What an attacker with the old private key can do**: sign an update file that every box still built
  with its public key accepts, with any version number. The no-downgrade rule does not stop them: they
  pick a version above every real one (up to 65535.65535.65535), after which such a box refuses every
  genuine release (not newer) until a USB flash. The probation and rollback stop only an image that
  fails to start or to confirm within 300 s; a malicious image that runs and confirms stays, and it can
  carry the attacker's own key.
- **What they still need**: to get the file to the box. The upload needs the box's account (the local
  network and the password); the pull needs the manifest the box reads to name it: the GitHub release
  (write access to the repository, or the release workflow), the box's `update.manifestUrl` setting
  (the account again), or a forged TLS certificate for GitHub. The key alone does not reach a box.
- **What the maintainer does**: rotate at once with a bridging release (R1.7), signed with the leaked
  key (the only key the boxes accept), carrying a new key; tell the users to update now; set the next
  release's `minVersion` to the bridge. A box that installed the bridge no longer accepts the leaked
  key.
- **What is not protected**: a box that never updates (the automatic check only reports and is off by
  default: a box nobody updates trusts the leaked key for ever); a box the attacker reaches first (a
  high version installed before the bridge wins, and only a USB flash recovers it); a box that rolled
  back from the bridge; and the manifest itself, which is not signed (its source is trusted through
  TLS and the GitHub account, and the `.sup` it names is checked against the box's key).
- **A lost key** (unchanged meaning): no bridge can be signed; every box needs a USB flash with an
  image of the new key.

### R3: A Mac test of the real chain (host program)

The host's update route installs into a slot file the bytes of the signed file's image; the host
does not run the image it stores (`crates/host/src/update_files.rs`), and its own binary (about 14 MB
in a test build) is over the 4 MB slot, so the chain is checked with two builds and a small image. A
test (slow, `#[ignore]`, run by the full validate and the nightly CI) builds the host twice, with key A
and key B (`--update-keygen`, run-time pairs, as `crates/host/tests/update.rs` does: its `keyed_host`
made a function of the key, each build in its own target folder), and checks, through the real server:

1. host A (the box before the bridge) accepts a file signed by A (200, the slot holds exactly the
   file's image) and refuses a file signed by B (400 `Invalid signature`): a release signed with the new
   key does not reach a box that skipped the bridge;
2. host B (the box after the bridge: the image built with the new key) accepts a file signed by B
   (200) and refuses a file signed by A (400 `Invalid signature`): the old key no longer reaches it;
3. the bridge's own file made by `release.sh` in bridge mode (R1, fake builder) is accepted by host
   A's route (its signature is A's), which is what an old box receives.

That the radio image really compiles the committed public key is the existing `release.sh` check
(the build must hold the key's text) and the on-box run (3) of "How the user will check it".

### R4: The update card (interface)

The card "Firmware update" of the Settings page (spec 032, `ui/src/components/settings/SettingsPage.tsx`),
modelled on Sowel's `VersionBadge` of `ui/src/pages/SettingsPage.tsx` and its `UpdateOverlay` of
`ui/src/components/system/UpdateOverlay.tsx` (Sowel commit `2d360726`), with Solite's tokens and
classes only:

1. **The version.** `GET /api/v1/system/version` (Sowel's `VersionInfo` without the two Docker
   fields): "Running version x.y.z". Below it, from `latest` and `updateAvailable`:
   - `latest` null: "No check since the box started";
   - `updateAvailable`: "Version x.y.z is available", a "Release notes" link (`releaseUrl`, a new tab;
     no link when `releaseUrl` is null) and a primary button "Update to x.y.z";
   - `latest` newer than `current` but `updateAvailable` false (the release's `minVersion` is above the
     running version, spec 039 R2): "Version x.y.z cannot be installed directly from this version: see
     its release notes" and the link, no button;
   - otherwise: "Up to date (latest x.y.z)".
2. **The release notes** are not served by the API (spec 039 keeps none, 16 KB the manifest's notes
   could take in the cache); the link opens the GitHub release page, which shows them. `VersionInfo` is
   unchanged (decision D71).
3. **Check for updates.** A secondary button: `POST /api/v1/system/version/check`, a spinner while it
   runs, the card refreshed with the answer. Every refusal shows the server's own text under the
   buttons: 429 `Please wait <n>s before checking again`, 503 `The clock is not set yet: ...`, 500
   `Update check failed: <reason>`, 409 `Update check already in progress`; a client-side text only
   when the server gave none (a network error: the generic text of `apiErrorText`).
4. **Update to x.y.z.** Opens a confirmation (Solite's `Modal`): title "Update to x.y.z?", text "The box
   downloads and installs version x.y.z, then restarts (about a minute). If the new version does not
   start, the box goes back to the current one by itself. You will need to log in again.", Cancel and
   Update. Update sends `POST /api/v1/system/update` with **no body** (the pull).
5. **While it installs**: the buttons disabled, a spinner and "Downloading and installing… <n> s" with
   the seconds since the click (an elapsed count, not a countdown: the duration is the network's).
   The request waits for the whole install (spec 039).
6. **Its answer**. Polling, in every case below: `GET /api/v1/system/version` every 3 s, never two at
   once (the next one starts 3 s after the previous ended), each with its own 5 s timeout (an
   `AbortController`); a network error, a timeout or a 5xx means the box is down or busy; the polling
   stops when the card unmounts (the 401 below sends the user to the login screen).
   - 200: "Installed x.y.z, the box restarts…", then polling. An answer with a `current` other than
     the one before: "Now running x.y.z", polling stops. An answer with the same `current` keeps the
     polling going (the box answered just before its restart). The restart ends the sessions (spec
     029): the first answer after it is a 401, the client's refresh fails, and the auth store shows the
     login screen; after logging in, the Settings page shows the new version. At **3 minutes**: if the
     box answered at least once with the same `current`, "The box still runs x.y.z: the new version was
     not kept (see the box's log)" when the **last** poll answered with the same `current` (a Mac host
     that does not restart, or an image rolled back); when the last poll got no answer, "The box has
     not answered for 3 minutes: reload the page"; polling stops.
   - a network error (status 0) during the request itself (the connection dropped, the browser gave
     up; the pull may still run in the box, up to its 600 s): "The connection was lost; waiting for the
     box…", polling as above for up to **11 minutes** (the 600 s download and a restart): a new
     `current` gives "Now running x.y.z"; at 11 minutes, the last poll answered with the same
     `current`: "The update did not complete: the box still runs x.y.z"; the last poll unanswered:
     the reload text.
   - any other refusal: the server's own text, the card back to its state: 400 `No update available`,
     400 `Version ... needs ...`, 400 `Download does not match the manifest`, 409 `Update already in
     progress` / `The running image is not confirmed yet`, 413, 429, 500 `Update check failed: ...` /
     `Download failed: <reason>`, 503 `The clock is not set yet: ...` or the no-key text (safe mode
     lets this route through, D65).
   - the probation's confirmation (60 s after the start) is visible in the box's log only: the card
     shows nothing about it.
   - "newer" in R4.1 is a numeric comparison of the three numbers (`compareVersions` in
     `ui/src/lib/version.ts`; 0.10.0 is newer than 0.9.0).
   - During a pull the upload is disabled, and during an upload the pull and the check are.
7. **The automatic check.** A checkbox "Check for a new version once a day" bound to
   `update.autoCheck` (`"true"` / `"false"`), saved at once, its refusal shown.
8. **The manifest's address** (advanced, folded under "Advanced"): a text field for
   `update.manifestUrl` (placeholder: the default GitHub address), Save; `""` returns to the default;
   400 `Invalid value for update.manifestUrl` shown.
9. **Install from a file** (spec 032's upload) stays in the card, below, under its own heading.
10. **The settings are always sent whole.** `PUT /api/v1/settings` changes only the keys it is given
    (spec 026, `a_put_keeps_the_keys_it_does_not_give`), but the interface does not rely on it: every
    save of the Settings page (the Home card, the automatic check, the manifest's address) reads `GET
    /api/v1/settings` just before and sends the whole map with its own keys changed (`saveSettings`
    in `ui/src/api/settings.ts`), so neither card ever drops the other's keys, whatever the server's
    merge rule. Kept on purpose (decision D71): the box run of spec 039 saw keys vanish after such a
    PUT (R7 b, not reproduced), and Sowel's own Settings page sends its whole form; the cost is one GET
    per save, and a value changed in another tab in the same second can be written back.
11. Every fixed text in English and French (`ui/src/i18n/messages.ts`); Sowel's texts where Sowel has
    one (`update.checkNow` "Check for updates", `update.confirmTitle` "Update to v{{version}}?",
    `update.confirmAction` "Update"): the versions are written with Sowel's `v` in the title and the
    button ("Update to v0.3.0").
12. The first load stays under the budget (170 KB gzip; about 75 KB today); its figure after the
    build is written in `docs/design-system.md`.

### R5: The host test of the settings merge

Through the real server (`crates/host/tests/update.rs` style): a `PUT /api/v1/settings` with only
`update.manifestUrl` after one with `home.name`, the latitude and the longitude keeps the three; and
after a restart of the host on the same data folder, `GET /api/v1/settings` still answers the four
(the file read back).

### R6: Resuming a cut download

1. **What resumes.** During a pull (spec 039 R4), after the download's first answer, a read that fails
   (the connection reset, a read timeout of `Limits::DOWNLOAD.idle_ms`, 30 s) or a body that ends
   before `image.size` bytes (`connection lost`) does not fail the pull at once: the core asks for the
   rest. The total time (`took longer than 600 s`, `Limits::DOWNLOAD.total_ms`) is never resumed.
   A read that fails once the count already equals `image.size` (the reader was waiting for its "one
   byte more" check) is the end of the body, not a cut: the hash and signature checks decide as before.
2. **The request.** The cut body is dropped first (its connection closed, so two TLS sessions never
   coexist), then the wait (R6.4), then the core fetches `image.url` again from the start of its
   redirects (a GitHub asset's signed address expires within minutes: the manifest's address gives a
   fresh one), every request of the chain carrying `Range: bytes=<count>-`, `<count>` the bytes the
   installation already read (the reader's count, hashed and written). The adapter sends the header
   and gives back a 206 with its `Content-Range` and `ETag` (new `Fetched::Partial`), and the `ETag` of
   a 200. A 206 to a request that asked no range (the download's first answer, the manifest) is a
   status like any other: `<url>: 206`.
3. **A 206 is accepted** when its `Content-Range` is `bytes <count>-<size - 1>/<size>` exactly
   (`<size>` the manifest's `image.size`), its `Content-Length`, if given, is `size - count`, and its
   `ETag`, when both answers gave one, equals the first answer's (compared as text, at most 256
   characters; a longer one is ignored). The body then continues the same reader: the same SHA-256
   context, the same count, the same installation into the same slot. Nothing new is written to flash
   besides the remaining image bytes; nothing is kept on flash across pulls.
4. **Bounds.** At most `RESUME_ATTEMPTS` (3) resumed requests per pull, waiting 2, 4 then 8 s before
   each (`RESUME_WAIT_MS` = 2000, doubled), on the monotonic clock through the download's pacing
   (`Pacing::wait_ms`, a new method with **no default**: the host and the box sleep, every fake
   advances its clock); all within the pull's total of 600 s (unchanged). A resumed request that
   itself fails (an error of the adapter: connection refused, TLS, timeout), answers a 5xx (a CDN's 502
   or 503 is a blip too), or whose body is cut again, counts as one attempt and the next one is tried;
   a refusal of the URL rule (`invalid URL`, `redirect from https to http refused`, `too many
   redirects`) and any other status fail at once. Each
   resume logs `update: download cut at <count> of <size> bytes (<reason>), resuming (<n>/3)`. Why
   these numbers (decision D72): a Wi-Fi blip or an access point's roam lasts seconds; three tries over
   14 s of waits cover those, and an outage longer than that is better met by a later pull than by a
   request that holds the update thread and the TLS memory.
5. **When it cannot resume, the pull fails cleanly** (the slot dropped as spec 039's failure, the boot
   record unchanged, 500 `Download failed: <reason>`), and the next pull starts over from byte 0:
   - attempts exhausted: the last cut's reason, `... (after 3 resumes)`;
   - a 200 to the `Range` request (the server does not resume): `the server does not resume (200 to a
     range request)`;
   - a 416, a `Content-Range` that is not the one asked for, a total other than `image.size`, a
     `Content-Length` other than the rest, a changed `ETag`: `the file changed on the server`; the
     cached manifest is also dropped, so the next pull reads the manifest again;
   - any other status: `<url>: <status>`, as spec 039.
   Decision D72: starting over inside the same request would erase and rewrite the 4 MB slot a second
   time on a server that just proved unreliable; a clean failure is shorter to reason about and the
   user's next click starts over.
6. **Never an unverified file**: whatever the number of resumes, the reader's count must equal
   `image.size` and its hash `image.sha256`, then spec 032's signature check over the header and the
   image's hash runs, all before the slot is made the next boot (unchanged order).
7. **Adapters.** The host (`crates/host/src/fetch.rs`): `Range` by the standard library's request for
   `http`, `-H "Range: bytes=<n>-"` for `curl`; 206 parsed like 200 with its `Content-Range` and
   `ETag`. The box (`firmware/src/fetch.rs`): the header added to `EspHttpConnection`'s request, the
   206 and its two headers read. The `Fetcher` trait gets `get_range(url, from, idle_ms)` with a default
   that does a plain `get` (an adapter without range support behaves as a server without it: R6.5).
8. **QEMU.** The image QEMU runs adds a cut to its in-memory pull (`firmware/src/update_check.rs`): the
   first answer of the file stops after 40 KB, the `Range` request is answered 206. The resumed request runs
   deeper than the first fetch (inside the installation's read), where a box would run a whole TLS
   handshake: the in-memory fetcher's `get_range` therefore touches an 8192-byte array on the stack
   (through `core::hint::black_box`, standing for the handshake measured by D68's margin), and
   `scripts/qemu.sh` requires the thread to end with status 200 and at least 1024 bytes of its stack
   never used: the resume leaves the handshake's 8 KB at its deepest point. This one rule replaces, for
   the pull thread, D68's "8192 bytes unused" (which counted the same 8 KB without the stand-in); the
   first fetch's depth is shallower than the resume's, so the rule covers both.

### R7: Observations from the box (spec 039's first run), settled

- **(a) A check right after the restart answered `latest` equal to the running version in 0.08 s, the
  test server seeing no request.** The cache is in RAM only (`Releases::default()` at every start), and
  `POST /system/version/check` always fetches; `latest` is never taken from the installed update.
  Not reproduced on the Mac: a test pins it (a fresh home answers `latest: null`; a check always reaches
  the fetcher, the cache filled or not). The success log line now names the address it read
  (`update: check: latest <x.y.z>, running <x.y.z> (<url>)`), so the box's log says which server
  answered; the on-box re-check is in the roadmap's list.
- **(b) `PUT /api/v1/settings` with only `update.manifestUrl` seemed to delete `home.name` and the
  location.** The core changes only the keys given (as Sowel's `upsert` per key; the existing test
  `a_put_keeps_the_keys_it_does_not_give`), and R5 adds the same check through the real server and
  across a restart. The interface sends the whole map anyway (R4.10). Not reproduced; the roadmap's
  storage-loss line (2026-11-25) is the likelier cause and stays open.

## Living spec delta

### docs/living-spec/README.md, "Updates itself from a signed file", "From a release"

- New bullet "**A cut download resumes**": R6 in business words (the `Range` request from the byte
  written, 3 tries waiting 2, 4 and 8 s within the 600 s, the same installation continued; a server
  that does not resume, a changed file, a 416: the pull fails and the next one starts over; the whole
  file verified as before).
- Changed: the download's failure bullet: "The next attempt starts over from byte 0" → after the
  resumes are spent or refused.
- Changed: the success log line names the manifest's address (R7 a).
- Changed: the QEMU sentence: the pull's check includes one cut and its resume (R6.8).

### docs/living-spec/README.md, "Shows the home in the browser" (the Settings page) and "Updates itself from a signed file" (the interface's bullet)

- Changed: the Firmware update card: the running version, Check for updates, the latest version and
  its release-notes link, Update to x.y.z with its confirmation, the wait and the restart, the
  automatic check and the manifest's address; the upload kept below (R4).
- New: every save of the Settings page sends the whole settings map (R4.10).

### docs/living-spec/README.md, "Publishing a release"

- New: the bridging release (R1): `release/update-key.bridge`, the next key, the checks, the two
  secrets, the procedure's pointer to `docs/release.md`.

### No longer true

Search: `rotation`, `bridging`, `step 3`, `starts over`, `resum`, `Range`, `Firmware update`,
`updateDescription`, `the card`, `six keys`, `notes` in `docs/` (roadmap and decisions excepted),
`README.md`, `CLAUDE.md`, `.claude/`:

- `docs/release.md` "What a leak, a loss and a rotation mean": "The format and the order are step 3 of
  the roadmap's J5b" → the bridge's procedure (R1.7) and "A leaked key" (R2).
- `docs/living-spec/README.md` "From a release": "The next attempt starts over from byte 0 (resuming
  is out of scope)" or its equivalent → R6.
- `docs/living-spec/README.md`, the Settings page's description of the update card (an upload only)
  → R4.
- `docs/living-spec/README.md` "Publishing a release": the sign job's "an empty secret" (now the
  bridge's second secret too); the dry run's "the secret is not read" (its bridge behaviour, R1.6);
  the script's usage line (`--bridge-file`, `--next-key-file`, `--next-key-env`); "The signing key is
  never in the environment..." (two keys in a bridge).
- `docs/release.md`: "the secret" in the singular (the environment's paragraph, the tag's checks);
  the dry run's paragraph (its bridge behaviour).
- `ui/src/i18n/messages.ts` `settings.updateDescription` ("Install a signed update file (.sup)") →
  the card's new description.
- `README.md` "A box already running Solite" if it names the upload as the interface's only way → the
  card's Update button.
- Any other sentence the implementer's search finds, listed in the report and here at the close.

Found by the docs pass (corrected in the same change):

- `docs/living-spec/README.md` QEMU sentence (the pull check): `status 200` alone and "fewer than
  8192 bytes unused" -> `resumes 1`, the cut and its resume, 1024 bytes (8336 measured).
- `docs/living-spec/README.md` "Shows the home in the browser": the Home card "sends the six home
  keys" -> the whole settings map; the Firmware update card's description (the upload only) -> R4.
- `docs/living-spec/README.md` "Publishing a release": the Install point names the card's Update
  button as well as the upload; the new point 7 holds the bridge.
- `docs/memory-budget.md` the `update-pull` thread's row: "9412 of 20480 unused, fails below 8192"
  -> 8336 with the resume and the 1024 rule.
- `README.md` "A box already running Solite": "the interface has no button for it" -> the card's
  "Check for updates" and "Update to vX"; the resume mentioned.
- `docs/release.md` "the secret" in the singular in the jobs' paragraph -> two for a bridge.
- `docs/architecture.md` rows Updates, Interface and Release: the resume, the card and the bridge.
- `.claude/skills/solite-release/SKILL.md` Phase 0: a key rotation needs a second secret.
- `docs/decisions.md` D64 ("designed in step 3") and D68 ("fails below 8192") are history and
  stay; D70 and D72 refine them.

## Spec review

Round 1 (Opus): five blocking items applied in place: R3 with two builds and a small image (the host
binary is over the slot and the host never runs its slot); the dry run in the bridge state and every
check against the given public key file (R1.3, R1.6); the card's end states with the same version
and after a dropped connection (R4.6); the cut body closed before the resume and QEMU's 8 KB at the
resume's depth (R6.2, R6.8); a failed read at the full count is the end (R6.1). Minor items applied:
which failures count as an attempt, a 206 nobody asked for, `wait_ms` without a default, the existing
names of the limits, the sentences made false, the 503 text, the numeric comparison, Sowel's `v`, the
upload and the pull exclusive, the secrets swapped just before the tag, `--next-key-*` with
`--build-only`, the whole-map save's reason (D71). Round 2: one blocking item applied (QEMU's one
stack rule for the pull thread, R6.8); minor applied (the 3- and 11-minute messages follow the last
poll).

## Minor items left as they are

- Code review (round 1, no blocking finding): the automatic-check checkbox flips back for a moment
  while its save runs; when the 600 s total is already spent, a resume still logs `resuming` and
  waits 2 s before failing with `took longer than 600 s`; `release.sh` also refuses a next key equal
  to the signing key and both key options at once (not listed in R1.3, harmless). Applied: the
  memory-budget sentence on the pull's deepest point, `wait_ms` without a default in the
  architecture, the 14 s host test marked slow (`#[ignore]`), the Backup card's error rule kept as
  spec 035's (server text for 5xx only), the chain test's exact `Invalid signature`.
- The screenshots (Hybrid and Dark, 1280 and 390 px: available, installing, the confirmation) are in
  the session's scratchpad, not in the repository.

## Acceptance criteria

- [x] `release.sh` in bridge mode, with a fake builder: the image carries the new key, the `.sup` is
      accepted by `--update-verify` with the old key and refused with the new one; each refusal of
      R1.2 and R1.3 answers its text with nothing written; a normal release is byte-for-byte the files
      it was (its tests unchanged and green).
- [x] `release.yml`: the bridge's secret is read by the `sign` job only, the normal command unchanged
      (the workflow's text tests).
- [x] R3's chain test passes on the Mac.
- [x] The update card: check, the four states of R4.1, confirm, pull, the elapsed count, the polling,
      "Now running", the 3-minute message, each refusal's server text, the toggle, the manifest's
      address, the upload kept; Vitest tests with fake timers; screenshots Hybrid and Dark at desktop
      and phone width.
- [x] Every save of the Settings page sends the whole map (a Vitest test per card); R5's host test.
- [x] A cut download resumes and installs (one cut, two cuts); a server without `Range`, a changed
      file, a 416 and exhausted attempts fail cleanly with the slot not bootable; the final hash and
      signature checks still decide.
- [x] `sh scripts/validate.sh` green, QEMU included, with the resume in the in-memory pull.
- [x] Living spec, `docs/release.md`, design system (budget figure), decisions D70 to D72, roadmap,
      specs index, README updated.

## Edge cases

- A bridge file left in the tree for the next release: refused by its version (R1.3).
- A bridge whose `previous` equals the committed public key: refused.
- The new private key unavailable at the bridge: refused (boxes would move to a key nobody holds).
- The old private key given as the next key (swapped secrets): the next key does not match the
  committed public key: refused.
- A box that rolled back from the bridge: R1.8 (documented; the bridge's `.sup` again).
- The pull answers 200 but the box never comes back: the 3-minute message.
- The token expired while polling: the login screen; the new version is shown after logging in.
- Two tabs: a check in one while the other pulls: the server's 409 text.
- `releaseUrl` null (a manifest not on GitHub): no notes link.
- `latest` older than `current` (a manifest behind the running version): "Up to date (latest x.y.z)".
- A cut at byte 0 of the body (the first answer's head arrived, no byte): resumed with `Range: bytes=0-`
  (a 206 from 0 is accepted).
- A cut during the header (fewer than 128 bytes read): resumed like any other byte.
- A 206 whose body is longer than announced: the reader's `size + 1` bound gives spec 032's "Update
  longer than declared".
- The server's ETag absent on one answer and present on the other: not compared (the hash decides).
- A resume when the total time is almost spent: the total bound wins (`took longer than 600 s`).

## Sowel contract

- `VersionInfo` (`src/core/version-checker.ts`): unchanged, no notes field (D71).
- The interface mirrors Sowel's `VersionBadge` (`ui/src/pages/SettingsPage.tsx`) and `UpdateOverlay`
  (`ui/src/components/system/UpdateOverlay.tsx`): the check button, the confirmation, the polling of
  `GET /system/version` every 3 s until `current` changes, the 3-minute fallback. Differences: inside
  the card rather than a full-screen overlay (the box's pull answers when installed, not at once), an
  elapsed count, the login after the restart (Sowel keeps its sessions across a restart), no Docker
  states.
- Settings: no new key (`update.autoCheck`, `update.manifestUrl` are spec 039's, D67).
- The `Range` request is a client behaviour of the box; nothing Sowel exposes changes.

## How the user will check it

On the Mac (the demo of this run): the release script's bridge mode with a fake builder (its tests);
the chain test (two host builds, R3); the host serving the garage with `update.manifestUrl` set to a
local test server and the interface open: Check for updates, Update to x.y.z, the wait, the restart
message (the host does not restart by itself: the card's polling sees the same version and says so
after it answers), screenshots; the resume tests on the Mac test server.

On the box (run by the main session afterwards, listed in the roadmap's "Tests to run with the box in
hand"): (1) **the resume**: a pull from a Mac server on the LAN that cuts the connection once in the
middle of the `.sup` (a small Python server that closes after N bytes on the first request and honours
`Range`), the log's `download cut ... resuming (1/3)` line, the install and the restart; (2) **the
card**: Check for updates and Update to x.y.z from the browser, the login after the restart, the new
version shown; (3) **the rotation** needs two box images and a USB flash (the user's yes, D6): the box
on an image of test key A takes a bridge signed by A carrying key B (from a LAN server), confirms it,
then refuses a file signed by A and accepts one signed by B. The Mac's chain test (R3) proves the same
logic without the box; the on-box run proves the compiled key of the radio image; (4) observation (a):
a check right after a restart, with the log line naming the address.
