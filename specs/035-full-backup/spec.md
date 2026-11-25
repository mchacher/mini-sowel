# Spec 035: Full backup of the box

## Context

Today (spec 026, D42) `GET/POST /api/v1/backup` carries only `{version, settings, zones}`, a JSON
body of at most 16 KB (`api::MAX_BODY_BYTES`). Equipments, recipe instances, devices
(`devices.json`, about 94 KB for 50 devices) and the Zigbee network's keys (`zigbee.json`) are
not in it: a box replaced, or a `storage` partition lost, means pairing every device again and
building the home again. This spec makes the backup whole: one archive file, downloaded and
restored in a stream, with the secrets encrypted by a password the user chooses at export.

It also settles the roadmap inbox line on D42's Sowel alignment: Solite's own document leaves
Sowel's path `/api/v1/backup` for a path Sowel does not use.

Decided in an autonomous run (the user away): every choice below is recorded in D56 and D57 for
the user's review.

## Goals

1. One file holds everything needed to rebuild the box: the home (devices, zones, equipments,
   recipe instances, settings), the Zigbee network (PAN, channel, keys) and the Wi-Fi settings.
2. The secrets in it are encrypted; the whole file is authenticated by the backup password.
3. A restore checks the whole file before it changes anything: a wrong, truncated, tampered,
   oversized or newer file leaves the box as it was.
4. Neither way holds the file in memory: it is streamed in chunks of 4 KB.
5. A box restored from a backup takes the same Zigbee network (PAN, channel, keys), leaving the
   network its radio ran, so the paired devices can keep talking to it: best effort, see R7 (the radio's
   address, its frame counter and its child table are not carried).

## Out of scope

- The account (`auth.json`): it belongs to the box at hand (see R2); a restore keeps the current
  account. Its sessions live in RAM only, so the box's restart logs the user out: the success
  message says to log in again.
- The network setting (`network.json` of the firmware: `auto`, `wifi`, `ethernet`): a choice of
  the box's hardware link, kept.
- What the radio itself holds of the old network (D57, roadmap inbox line): its NWK frame counter
  (a new radio starts at 0, and a device that remembers the old radio's higher counter may drop
  its commands until it passes it), its own address (EUI64: Zigbee 3.0 devices keep the trust
  centre's address; on EmberZNet radios it is a token written once, never undone, so Solite does
  not write it) and its child table (end devices whose parent was the coordinator rejoin by
  themselves or need pairing again). Zigbee2MQTT carries the first two; Solite does not, today.
- A partial restore (only the rooms, only the devices) and a merge: a restore replaces.
- Sowel's backup shape (a ZIP of SQLite tables): not mirrored (D56).
- Flash encryption of `zigbee.json` and `wifi.json` on the box: unchanged (roadmap line of spec 021).

## Requirements

### R1: Routes

- `GET /api/v1/system/backup`: the archive, `200`, `Content-Type: application/octet-stream`,
  `Content-Disposition: attachment; filename="solite-backup.slb"`, sent chunked (the length is not
  known ahead).
- `POST /api/v1/system/backup`: restores the archive of the body (`application/octet-stream`, a
  `Content-Length` required on the box as for the update, 411 otherwise). Query `?wifi=true` also
  restores the Wi-Fi settings; without it, the current box's Wi-Fi is kept.
- The backup password is sent in the header `X-Backup-Password`, percent-encoded UTF-8 (so any
  character fits a header); never in the query (it would reach logs) and never in a body.
- Both routes are behind the token (spec 029's gate: 401/403 before anything is read). Other
  methods: 405 `Method not allowed`.
- `/api/v1/backup` (GET and POST) is removed: 404 like any unknown route (D56 replaces D42).
- Order of the checks of the restore, all before the body is read, under the home's lock, in the
  gate: token; content type (415 `Unsupported Media Type`); Presence (403
  `{"error": <hint>, "confirmRequired": true}`, as the factory reset, D48); pairing window open
  (409 `Pairing in progress`); another backup or restore running (409 `Backup already in
  progress`). Then, lock released: declared length over `MAX_ARCHIVE_BYTES` (413 `Backup too
  large`), password missing (400 `Backup password required`).
- Export checks: password missing (400 `Backup password required`), shorter than 8 or longer than
  128 characters (400 `Backup password must be 8 to 128 characters`), another backup or restore
  running (409 `Backup already in progress`). These errors are whole JSON answers sent before the
  status line of the stream.
- While a restore waits for the restart (committed, not yet applied: the Mac host never restarts
  by itself), both routes answer 409 `A restore waits for the restart`, checked in the gate.
- The password is used as the UTF-8 bytes received, with no Unicode normalisation (a password
  typed with a composed accent and one with a combining accent are two passwords).
- Duration: the key derivation (`BACKUP_ITERATIONS`, R3) takes about 5 to 10 s on the box (**E**,
  from D47's rate) before the first byte of an export and before the first file of a restore is
  staged; under a second on the Mac.

### R2: What the archive holds

Fixed list of names, in this order (`backup::ARCHIVE_FILES`), each present only if the box has it:

| Name | Content | Stored |
| ---- | ------- | ------ |
| `devices.json` | devices | plain |
| `zones.json` | zones | plain |
| `equipments.json` | equipments | plain |
| `recipes.json` | recipe instances | plain |
| `settings.json` | home settings | plain |
| `zigbee.json` | the Zigbee network's PAN, extended PAN, channel and keys (the host's `network.json`) | encrypted |
| `wifi.json` | the Wi-Fi name and password (the box only) | encrypted |

The archive's name `zigbee.json` is the firmware's file; the host's adapter maps it to its own
`network.json`. `auth.json`, `network.json` (firmware network setting), `booting`, the update
slots and anything else are never in it.

### R3: Archive format (version 1), all integers little-endian

1. **Header**, 70 bytes: magic `SOLITEBK` (8); format `u16` = 1; PBKDF2 iterations `u32`
   (`backup::BACKUP_ITERATIONS` = 100,000 at export; accepted from `auth::MIN_ITERATIONS` to
   `backup::MAX_BACKUP_ITERATIONS` = 400,000); salt (16 random bytes); nonce (8 random bytes); key check (32 bytes:
   HMAC-SHA256 under the authentication key of the ASCII text `solite backup key check`).
2. **Entries**, one per file: kind `u8` = 1; name length `u8` and the name (ASCII, one of R2);
   flags `u8` (bit 0: encrypted); then chunks of `u16` length (1 to `CHUNK_BYTES` = 4096) and
   their bytes; a chunk of length 0 ends the file.
3. **End**: kind `u8` = 0; manifest length `u16` (at most `MAX_MANIFEST_BYTES` = 4096) and the
   manifest, JSON: `{"version":1,"solite":"<crate version>","files":[{"name","size","sha256",
   "encrypted"}]}`, `size` the bytes stored, `sha256` the lowercase hex SHA-256 of the bytes as
   stored (the ciphertext for an encrypted file, so the manifest says nothing about a secret);
   then the MAC: 32 bytes, HMAC-SHA256 under the authentication key of every byte before it, from
   the magic on.

Keys: `master = PBKDF2-HMAC-SHA256(password, salt, iterations)` (spec 029's `pbkdf2_sha256`);
`encryption key = HMAC-SHA256(master, "solite backup encryption")`; `authentication key =
HMAC-SHA256(master, "solite backup authentication")`. Encryption: ChaCha20 (RFC 8439, the
RustCrypto `chacha20` crate), 12-byte nonce = the header's 8 bytes followed by the entry's index
(`u32`, big-endian, 0 for the first entry), block counter from 0. Encrypt-then-MAC: the MAC covers
the ciphertext and the manifest of its hashes. Iterations: the account's 10,000 (D47) suit a hash
that never leaves the box; the archive leaves it and can be attacked offline, so it takes ten times
more (about 5 to 10 s on the box, once per export or restore), and a restore refuses more than
four times that, which would hold an HTTP worker for minutes.

### R4: Export

The archive is built from the files as last saved (at most `DEFINITION_DELAY_MS`, 2 s, behind the
home in RAM; the devices' values at their write interval), read through the `BackupFiles` adapter
in chunks of 4 KB; the key is derived once before the first byte (about 5 to 10 s on the box, R1,
lock released). While an export reads the files, the Saver starts no write (the files stay due,
written at its next turn after the export ends), so no file is replaced while it is read: the
Saver marks the home's backup state `writing` under the lock for the whole of a file's write (all
its chunks and the rename), and once the key is derived the export waits for that mark to clear
(at most 5 s, polled under the lock, else 503 `Busy saving, try again`); the check that the mark
is clear and the switch to `exporting` happen in one critical section under the lock, so no write
starts between them; only then is the first file opened. `BackupFiles::open` also reads `<name>.new` when the live file is
missing (the FAT save's window between the removal and the rename), as `Storage::read` does. An
export ends after `MAX_EXPORT_MS` (120 s) from its first byte, whatever the client: a client that
stops reading cannot hold the Saver longer. A failure while streaming (a file that cannot be read) ends the response without its end and MAC:
the client keeps a truncated file that a restore refuses.

### R5: Restore, validated whole before any change

One pass over the body, through a 4 KB buffer, into a staging area of the adapter (each file
written as its staged copy, `<name>.restore` beside the live one), checking as it reads:

- magic, then the format: over 1, 400 `Backup made by a newer version of Solite` (nothing read
  after it, nothing staged); not `SOLITEBK`: 400 `Invalid backup: not a Solite backup`;
- iterations outside the accepted range: 400 `Invalid backup: <reason>`;
- the key check against the password given: wrong, 400 `Wrong backup password` before anything is
  staged;
- each entry: a name of R2 (else `Invalid backup: unknown file <name>`), not seen before, at most
  `MAX_FILES` (7) entries, the flag matching R2 (secrets encrypted, home files plain), each file at
  most `MAX_FILE_BYTES` (1 MiB), a chunk length at most 4096;
- the whole body at most `MAX_ARCHIVE_BYTES` (2 MiB): over it, 413 `Backup too large`;
- a declared `Content-Length` over `MAX_ARCHIVE_BYTES`: 413 before any byte is read, so a body
  too large writes nothing; past the key check, a refused archive may have written up to its
  size in staged copies to the flash before its MAC fails (accepted, D39: a restore is a rare,
  manual act);
- the end: the manifest parses, lists exactly the entries read, in order, with their sizes and
  hashes; the MAC matches; no byte after the MAC;
- a body that ends early: 400 `Invalid backup: truncated`.

`wifi.json` is read, hashed and authenticated like the others, but staged only with `?wifi=true`.
Any refusal discards the staged copies: the live files are untouched. Only when everything holds
are the staged files committed (R6).

### R6: Commit and reload

The commit, in this order: every staged copy closed and synced (`sync_all`); then the marker
`restore.pending` written atomically through the `Storage` path (temporary file, `.new`, rename),
one line per archive name, `promote <name>` or `remove <name>` (a home file of R2 absent from
the archive); then the home's backup state set to `pending` (R1's 409 until the restart); then
the answer 200 `{"success":true,"message":"Backup restored: the box restarts, log in again after
it","files":<n>,"wifi":<bool>}` (the Mac: `"Backup restored: restart the host to load it"`). A
marker that does not parse or names a file outside R2 is discarded whole at the start (staged
copies removed, live files untouched, a warning). The
staged files replace the live ones at the next start, before the home is loaded
(`backup::apply_pending`), so the Saver, which writes the home in RAM until the restart, never
overwrites them:

- `apply_pending` runs first at every start, before anything reads the data folder: on the box in
  `main.rs` right after the storage is mounted (before the network setting, the console, the
  Wi-Fi and the engine), for every image; on the Mac before `load_home` and before the radio's
  start;
- the box restarts right after the answer (the update's restart, `ota::restart_after_update`
  pattern: the Saver flushes, then a software reset); at the start the staged files are renamed
  over the live ones (FAT: the live file removed first, then the rename), then the marker
  removed. An interruption at any point is finished at the next start: the marker still names
  the files whose staged copy remains. A staged copy without the marker (a refused or interrupted
  restore) is removed at the start;
- `promote` and `remove` also delete the name's leftover `<name>.new` and `<name>.tmp` (a power
  cut during the Saver's flush before the restart can leave one, which `Storage::read` would
  otherwise bring back);
- a home file of R2 absent from the archive is removed at the commit's application (the home is
  the archive's, not a mix); an absent `zigbee.json` or `wifi.json` leaves the current one;
- before `zigbee.json` is promoted over a live file of another network (a comparison of the PAN
  and extended PAN ids, as `radio::backup::is_network`), the live file is first renamed
  `zigbee-<PAN>.json` (the host: `network-<PAN>.json`), as a network formed over another one is
  today: the keys of the radio's current network are never lost;
- `?wifi=true` on the Mac (no `wifi.json` there) is ignored: the answer says `"wifi": false`;
- the Mac host prints `backup: restored <n> files; restart the host to load them`, like the update,
  and applies them at its next start;
- the start then loads the home as usual (`load_home`, the recipes started, D38/D46 settle as at
  any boot); a restored `wifi.json` is read by the boot's Wi-Fi start; a restored `zigbee.json`
  is used by the network start (R7). The start logs `backup: <n> files restored`.

### R7: The Zigbee network after a restore (best effort, D57)

At the network start, when the radio has no network (`networkInit` answers not joined) and the
backup file (`zigbee.json`, the host's `network.json`) parses (PAN id, extended PAN id, channel
11 to 26, both keys), the radio forms that network with those keys instead of random ones (the
file's extended PAN id is in display order, the reverse of the frame's: `parse` reverses it), so
the devices paired to the old box can keep talking to the new one; what is not carried (Out of
scope) may make some devices need pairing again: the box check says how many. The `network:`
line says `formed (restored from the backup)`. When the restore's application promoted a
`zigbee.json` (R6), it also writes the one-shot mark `zigbee.restore`. At the network start, with
that mark present, a radio that runs another network (the usual case: a box forms its own network
at its first boot, and a factory reset keeps it) **leaves it** (EZSP `leaveNetwork` (0x0020), then
the stack's network-down status) and forms the restored one; the network it left keeps its keys in
`zigbee-<PAN>.json`, set aside at the restore's application (R6). A radio already on the restored
network resumes it. The mark is removed once the network is up (formed or resumed); a start that
fails keeps it for the next. Without the mark, nothing changes: a radio on a network resumes it and
is never left (today's rule; `running_network` still refuses a radio that is only a member). This is
the one case where Solite leaves a network, asked by the user's restore (D57). A file that does not
parse: a new network is formed as today (random keys) and a warning is logged.

### R8: Key derivation in slices (the box's watchdog)

ESP-IDF's task watchdog (5 s) watches the idle task of the HTTP workers' core; a derivation of 5
to 40 s in one loop would starve it. The core derives the 32-byte PBKDF2-HMAC-SHA256 block itself
(one HMAC loop, the same result as `auth::pbkdf2_sha256`, checked by a test) and calls a pause
given by the caller every 5,000 iterations; the servers give a 10 ms sleep (`std::thread::sleep`),
the tests a counter. The box check reads no `task_wdt` line during an export.

### R9: Bounds

`MAX_EXPORT_MS` = 120 s, `BACKUP_ITERATIONS` = 100,000, `MAX_BACKUP_ITERATIONS` = 400,000,
`MAX_ARCHIVE_BYTES` = 2 MiB, `MAX_FILE_BYTES` = 1 MiB, `MAX_FILES` = 7, `MAX_MANIFEST_BYTES` =
4096, `CHUNK_BYTES` = 4096, each written next to its constant with its reason: the worst-case
home's files (every `MAX_*` of the core reached) fit, measured by a test that saves the worst case
and sums its files. One export or restore at a time. The archive is never held whole: the core
holds one 4 KB buffer (on the heap), the hash, MAC and cipher states (under 1 KB, on the stack
of the HTTP worker, inside its 10 KB as the login's PBKDF2 is) and the manifest (under 4 KB).

### R10: Interface (Settings page)

The **Backup** card replaces the old one:

- **Export**: a password field and its confirmation (8 to 128 characters, the two equal, else the
  button stays disabled with the reason under it), an "Export" button; the download is fetched with
  the token and the header, then saved as `solite-backup-<YYYY-MM-DD>.slb`; a refusal shows the
  host's text.
- **Restore**: a file input (`.slb`), the backup's password, a checkbox "Also restore the Wi-Fi
  settings", a "Restore" button that opens a confirmation dialog (what it replaces: devices, rooms,
  equipments, recipes, settings, the Zigbee network; the account kept; the box restarts); then
  "Restoring..." during the upload; success shows the answer's `message`; a refusal shows the
  host's text, and the `confirmRequired` refusal shows its hint as the factory reset does.
- Sowel's tokens and classes only; the first load stays within the 170 KB gzip budget.

## Living spec delta

### docs/living-spec/README.md

- Removed: **Export** (`GET /api/v1/backup`) and **Import** (`POST /api/v1/backup`) of the home
  settings section.
- New, in the home settings section (or a **Backup** paragraph beside the update): the full
  backup of R1 to R8 in business words: what the archive holds, the password, that a restore is
  checked whole first and replaces the home at the box's restart, what happens to the Zigbee
  network (R7) and the Wi-Fi (kept unless asked), the errors.
- Changed: the network start (lines about `zigbee.json` and the host's backup): a radio with no
  network forms the network of the backup file when there is one (R7).
- Changed: the Settings page's **Backup** card (R10).
- Changed: recipe instances settle on a zone "updated (through its route or a backup import)" →
  "updated through its route".
- Changed: box-session's home checks: "the backup exported and imported back" → "the full backup
  exported, and its restore with a wrong password refused".

### docs/architecture.md, docs/memory-budget.md, docs/sowel-subset.md, docs/roadmap.md

- Architecture: the files of `/data` and `solite-data/` (`.restore` copies and `restore.pending`,
  between a restore and the next start); the core's `backup/` module and the `BackupFiles` adapter; `settings/backup.rs`
  removed; the host's and the firmware's adapters.
- Memory budget: a row for the backup (one 4 KB buffer on an HTTP worker, M for the core's part).
- Sowel subset: the two `/api/v1/backup` rows removed; `GET/POST /api/v1/system/backup` in the
  routes and in the Deviations table (D56).

### No longer true

Searched for `backup`, `/backup`, `MAX_BACKUP_BYTES`, `solite-backup`, `export`, `import`,
`zigbee.json`, `D42` in `docs/` (roadmap and decisions excepted), `README.md`, `CLAUDE.md`,
`.claude/`:

- docs/living-spec/README.md: the Export and Import paragraphs → the full backup (above).
- docs/living-spec/README.md: the Settings Backup card (`solite-backup-<date>.json`, 16384 bytes)
  → R10.
- docs/living-spec/README.md: "updated (through its route or a backup import)" → "updated through
  its route".
- docs/living-spec/README.md: box-session's home check "the backup exported and imported back" →
  the new check.
- docs/architecture.md: `settings/` "`backup.rs`, the export and import document", "the API serves
  `/settings`, `/backup`" → the `backup/` module and `/system/backup`.
- docs/roadmap.md:204: spec 026's box check through `POST /api/v1/backup` → the full backup's.
- docs/living-spec/README.md (near the Mac host and box sharing one network): a warning never to
  restore onto a second box while the first still runs.
- docs/sowel-subset.md: the two rows "Export/Import (Solite's document, D42)" → the new route.
- docs/living-spec/README.md: "the one exception is the update route" (`application/octet-stream`)
  and "a body over 16 KB, 413 (except the update route's ...)" → the backup restore added.
- docs/architecture.md: the Interface row ("exports or imports the home") and the firmware row's
  network start (`/data/zigbee.json`) → the full backup and the restored network.
- docs/memory-budget.md: the PSRAM arithmetic (totals 1196 and 536 KB, margins 852 and 1512 KB) →
  with the backup's row (1206 and 546 KB, 842 and 1502 KB).
- docs/decisions.md: D42's title carries "(replaced by D56)", as D3's and D14's do; its text is not
  edited. D43's sentence "the backup document of D42 has Sowel's paths and its own body" is
  superseded by D56 (said in D56).
- docs/roadmap.md: the box checks of spec 026 (`POST /api/v1/backup`, "the backup exported and
  imported back") → the full backup's.

## Acceptance criteria

- [x] `GET /api/v1/system/backup` with a password gives an archive; `POST` of it into a fresh
      store gives byte-identical files (round trip, the Mac host and the core's test).
- [x] A wrong password, a truncated file, one byte changed anywhere, a newer format, an oversized
      body: refused with their message, and no live file changed.
- [x] A 1 MB archive streamed both ways peaks under 32 KB of heap.
- [x] Without `?wifi=true` the current `wifi.json` is kept; with it, replaced.
- [x] A radio with no network forms the backup's network with its keys (EZSP frames checked in a
      test; on real devices, the box check measures it, R7).
- [x] The routes need the token; the restore needs the Presence confirmation.
- [x] `/api/v1/backup` answers 404.
- [x] The Settings page exports and restores, in Sowel's design; budget kept.
- [x] Sowel's JSON of every other route unchanged (`sowel_contract`).

## Edge cases

- Password with accents or emoji: percent-encoded in the header, decoded as UTF-8; not valid
  percent-encoding or not UTF-8: 400 `Backup password required`.
- An empty home (no file yet): the archive has only the manifest; a restore of it removes the home
  files (an empty home) and keeps the network and Wi-Fi files.
- The same backup restored twice: same result.
- A restore while a pairing window is open: 409, nothing read.
- Two exports at once: the second 409.
- A power cut during the commit's application: finished at the next start (R6).
- A staged copy left by a refused restore that crashed: removed at the next start.
- A client that stops reading during the export: the export ends, the Saver writes again.
- A restore while another waits for the restart: 409 (R1).
- Restoring onto a second box while the first still runs: two coordinators with the same keys
  and PAN; the living spec warns never to do it (as for the Mac host and the box sharing a radio
  network).
- `Content-Length` over 2 MiB: 413 before reading; no length on the host: the stream is bounded
  to `MAX_ARCHIVE_BYTES + 1` all the same.

## Sowel contract

No Sowel type changes. Sowel's `GET/POST /api/v1/backup` (`docs/technical/api-reference.md`,
"Backup (Admin)": a ZIP or JSON `{version: 1, tables}` of its SQLite tables) is not mirrored:
Solite's backup lives at `/api/v1/system/backup`, a name Sowel does not use, with its own document
(D56). The gate's errors keep Sowel's `{error}` shape.

## How the user will check it

On the Mac: `cargo run -p solite-host -- --sim scenarios/garage.json --serve --speed 10`, log in,
Settings, export with a password, then restore the file with the password into a fresh data
folder and restart the host: the same devices, rooms, equipments and recipes. A wrong password is
refused with "Wrong backup password". On the box (with the box in hand, roadmap): export, factory
reset or a second box, restore, restart; then count the devices that report again without
pairing (best effort, R7), and write the count in this spec.

## Decisions taken in the user's absence (for review)

- A restore leaves the network the radio runs and forms the restored one (D57; the left network's
  keys are kept in `zigbee-<PAN>.json`): the one exception to "never leave a network", asked by
  the user's restore; the main session's design (the new box resumes the same PAN and keys).
- The radio's address (EUI64) and frame counter are not carried to a new radio (D57): writing
  the address is irreversible on EmberZNet radios, and the counter needs EZSP values Solite does
  not read; the box check measures how many devices follow without pairing.
- The archive's key derivation is ten times the account's (100,000 iterations, about 5 to 10 s on
  the box per export or restore), for a file that can be attacked offline (D56).
- `/api/v1/backup` is removed rather than kept beside the new route (D56): the full archive covers
  what it carried, and Solite no longer uses Sowel's path with another shape.

## Spec review leftovers (minor, not applied)

None: both rounds' minor items are in the spec (round 1: 8 to 16; round 2: 3 to 6).
