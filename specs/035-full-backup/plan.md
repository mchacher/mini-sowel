# Plan: Spec 035

## Steps

- [x] 1. Living spec delta applied (`docs/living-spec/README.md`), architecture, memory budget,
      Sowel subset, decisions D56 and D57, roadmap (inbox lines of the full backup and of D42's
      alignment removed; frame counter line added; box check added to "Tests to run with the box
      in hand"; a log line), specs-index row.
- [x] 2. Core: `backup/` module (format, crypto, export `ArchiveReader`, `restore`,
      `apply_pending`, `BackupFiles` trait, bounds); `chacha20` dependency.
- [x] 3. Core: home backup state and ticket; Saver held while exporting; API: `/system/backup`,
      `is_streamed_request`, the two `Reply` variants, the gate's checks; old `/backup`,
      `settings/backup.rs`, `export_backup`/`import_backup` removed with their tests.
- [x] 4. Core: `radio/backup::parse`, `start_network` with a restored network.
- [x] 5. Host: `BackupFiles` adapter, `serve.rs` streaming both ways, `apply_pending` at start,
      restored network passed at the radio start.
- [x] 6. Firmware: adapter, HTTP streaming both ways, restart after a restore, `apply_pending`
      at start (radio and sim images), restored network; `cargo build --release` for every image
      `validate.sh` builds.
- [x] 7. Interface: API calls and the Backup card; tests; budget.
- [x] 8. `scripts/box-session.sh` home step and its fake.
- [x] 9. `sh scripts/validate.sh` green, QEMU included.

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| backup (core, `tests/full_backup.rs`, MemBackupFiles fake) | Round trip: home files + zigbee + wifi exported, restored with `wifi=true` into a fresh fake, applied | Every live file byte-identical to the source |
| backup | Round trip without `wifi=true` | `wifi.json` of the target kept, the others replaced |
| backup | Wrong password | 400 `Wrong backup password`; nothing staged, live files unchanged |
| backup | Truncated archive (cut at several offsets: in the header, in an entry, in the manifest, in the MAC) | 400 `Invalid backup: truncated`; staged copies discarded, live unchanged |
| backup | One byte flipped (in a plain file's chunk, in an encrypted chunk, in the manifest, in the MAC) | 400 `Invalid backup: ...`; live unchanged |
| backup | Bytes after the MAC | 400; live unchanged |
| backup | Newer format (2) | 400 `Backup made by a newer version of Solite`; nothing staged |
| backup | Not a Solite file (bad magic) | 400 `Invalid backup: not a Solite backup` |
| backup | Archive over `MAX_ARCHIVE_BYTES` (stream) | 413 `Backup too large`; live unchanged |
| backup | A file over `MAX_FILE_BYTES`; an unknown name; a name twice; a secret stored plain; a chunk over 4096; iterations out of range | 400 each; live unchanged |
| backup | Export: password under 8 or over 128 characters | 400 `Backup password must be 8 to 128 characters` |
| backup | Export of an empty home | An archive of the manifest only; its restore removes the home files, keeps zigbee and wifi |
| backup | Encrypted entries: the archive's bytes do not contain the network key nor the Wi-Fi password | Not found in the archive |
| backup | `apply_pending` after an interruption (marker present, some files already promoted) | Finished; marker gone; files as the archive |
| backup | A staged copy without marker at start | Removed; live untouched |
| backup | Bounds: the worst-case home saved (every `MAX_*`) exported | Under `MAX_ARCHIVE_BYTES`, each file under `MAX_FILE_BYTES` |
| backup | Restore: password header missing (GET and POST); invalid percent-encoding; not UTF-8 | 400 `Backup password required` |
| backup | Round trip with an accented and emoji password | Identical files |
| backup | Format 0 | 400 `Invalid backup: ...` |
| backup | Export that fails mid-stream (a file unreadable after the first chunk): the partial bytes restored | 400 `Invalid backup: truncated`; live unchanged |
| backup | The same archive restored twice (applied between) | Same files |
| backup | `?wifi=true` with an adapter that has no Wi-Fi (the Mac) | Ignored, `"wifi": false` |
| backup | Commit order: a fake that records calls | Staged copies synced, then the marker, then the pending state |
| backup | A truncated or unparsable marker; a marker naming a file outside R2 | Discarded whole: staged removed, live untouched |
| backup | `zigbee.json` promoted over a live file of another network | The live one renamed `zigbee-<PAN>.json` first; same network: replaced in place |
| backup | Export: live file missing, `<name>.new` present | The `.new` content exported |
| backup | Export started while the Saver is mid-write (`writing` mark) | Waits; mark cleared within 5 s: exported; not cleared: 503 `Busy saving, try again` |
| backup | Export past `MAX_EXPORT_MS` (fake clock) | Ends without the MAC; the Saver writes again |
| api | Either route while a restore waits for the restart | 409 `A restore waits for the restart` |
| api | Restore with a declared `Content-Length` over `MAX_ARCHIVE_BYTES` (host server) | 413 before reading |
| radio | Mark `zigbee.restore` + radio on another network | `leaveNetwork`, network down, then `formNetwork` with the file's values; mark removed once up |
| radio | Mark + radio already on the restored network | Resumed, no leave; mark removed |
| radio | No mark + radio on another network | Resumed, never left (unchanged) |
| radio | Mark + start fails after the leave | Mark kept |
| backup | Restore's application with `zigbee.json` promoted | Mark `zigbee.restore` written |
| backup | Sliced PBKDF2 | Same 32 bytes as `auth::pbkdf2_sha256` for several iteration counts; the pause called every 5,000 |
| backup | Marker removes X, `X.new` present | X absent after `load_home` |
| saver | The Saver tries to start a write between the export's check and its hold | Refused: check and hold are one critical section |
| radio | Extended PAN id order | The `formNetwork` frame holds the file's id reversed (frame order) |
| firmware (no tests) | 411 without `Content-Length`; restart after a restore | Same code path as the update (`answer` streaming branch), checked by review and by the box check |
| backup (`tests/full_backup_memory.rs`, `peak_alloc`) | A 1 MB archive exported then restored through counting fakes | Peak under 32 KB of heap, each way |
| api (`tests/full_backup.rs`) | No token on GET and POST | 401, nothing read |
| api | Restore without Presence | 403 `confirmRequired`; body not read |
| api | Restore while pairing; a second job while one runs | 409 each |
| api | Restore with another content type | 415 |
| api | PUT/DELETE `/system/backup` | 405 |
| api | `GET` and `POST /api/v1/backup` | 404 |
| api | Plain `handle` on the two streamed routes | 501 |
| saver (core) | A due file while exporting | Not written; written after the ticket drops |
| radio (core, simulated EZSP) | Radio not joined + parsable backup | `formNetwork` with the file's PAN, extended PAN and channel, `setInitialSecurityState` with its keys; `network:` formed (restored) |
| radio | Radio not joined + unparsable backup | Random network formed as today, a warning |
| radio | Radio already on a network | Resumed as today (unchanged behaviour) |
| sowel_contract (core) | The JSON of the other routes; the route list | Unchanged except the backup rows |
| host (`crates/host/tests/`) | `--serve` on a sim: GET then POST over HTTP, restart, same files | Round trip over the real server; a wrong password 400 and the connection closed |
| host adapter | `network.json` mapped to `zigbee.json`; `.restore` files mode 0600 | As stated |
| box-session fake (`box_session_script.rs`) | Home step: export and wrong-password restore | Reported `ok`; the fake's paths updated |
| ui (`SettingsPage.test.tsx`) | Export disabled until the two passwords match and are 8+; export calls the API with the password and saves the blob | As stated |
| ui | Restore: file + password + confirm → API called with the file, password, wifi flag; success message | As stated |
| ui | Restore refused with `confirmRequired` | The hint shown |
| ui | Restore refused (wrong password) | The host's text shown |

## Validation

- [x] Validate command green (QEMU included)
- [x] Agent review done, blocking findings fixed
- [ ] Demo seen and approved by the user (autonomous run: the main session presents it)

## Changes while building

Core (steps 2 to 4), where the build refined the design; the semantics of R1 to R9 are unchanged.

- `BackupFiles` (core, `backup/mod.rs`) is not the trait of `architecture.md` word for word:
  `open` and `open_staged` return owned readers (`Box<dyn Read>`), `stage` returns a `StagedFile`
  (a `Write` with `finish`, which closes and syncs), and the trait gains `has_wifi`, `restarts`
  (the answer's message and the Mac's `wifi: false`), `open_staged`, `set_aside` (the live
  `zigbee.json` of another network kept under its PAN id: the adapter names the file) and
  `restore_mark` / `set_restore_mark` / `clear_restore_mark` (the one-shot `zigbee.restore`). The
  marker's last line is `end`, so a marker cut short is discarded whole.
- `Pacing` (new trait): the caller's clock and pause (`now_ms`, `pause`), for the 5,000-iteration
  pause of R8, the export's 5 s wait for the Saver and `MAX_EXPORT_MS`.
- The jobs are methods of the ticket: `BackupTicket::export(password_header, files, pacing,
  random) -> Result<ArchiveReader, Response>` and `BackupTicket::restore(wifi, content_length,
  password_header, source, files, pacing) -> Response`, so that the servers only move bytes
  (the declared-length 413, the percent-decoded password and the 400 are in the core).
- The backup state has its own lock (`BackupState`, an `Arc` shared by the home, the tickets and the
  Saver), so the export's check-and-switch and the Saver's check-and-mark are one critical section
  of that lock, not of the home's.
- A stop (`Saver::tick(.., stopping = true)`) writes whatever an export does: the program ends and
  data not written would be lost.
- `radio::Coordinator::start_network(settings, restored, replace, random, on_formed)`: the mark
  is the `replace` flag; the caller removes the mark once the call returns `Ok`, and keeps it on
  `Err`. `restored` is `radio::backup::parse(text)` of the backup file. `leaveNetwork` takes an
  options byte (0) from EZSP 14.
- `docs/sowel-subset.md`: the routes and deviations rows are changed (the contract test reads
  them); they cite D56, which `docs/decisions.md` must hold.
