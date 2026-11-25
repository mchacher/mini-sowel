# Architecture: Spec 035

## Overview

```
GET /system/backup ─ gate (token, busy) ─ Reply::BackupExport(ticket) ─ server reads X-Backup-Password
   └─ backup::ArchiveReader::new(password, files, random)  (PBKDF2 once, lock released)
        impl Read: header → entries (BackupFiles::open, 4 KB chunks, ChaCha20 for secrets,
        SHA-256 + HMAC running) → end (manifest + MAC) ──► host: tiny_http chunked body
                                                         ──► box: send_chunk loop

POST /system/backup ─ gate (token, 415, Presence, pairing, busy) ─ Reply::BackupRestore(ticket)
   └─ server: length check, password header, body reader (Content-Length bounded)
        backup::restore(password, wifi, &mut dyn Read, files) → Response
          header + key check → entries staged (BackupFiles::stage) with hash/MAC/decrypt
          → end: manifest + MAC checked → BackupFiles::commit (marker) | discard
   └─ box: restart after the answer; host: message "restart the host"

start (host serve/pair, firmware radio engine, before load_home):
   backup::apply_pending(files) → renames staged over live, removes marker, logs line
network start (coordinator): no network + parsable backup file → form with its PAN and keys
```

## Data

- Archive (R3): never stored by the box, only streamed.
- Staged files `<name>.restore` and the marker `restore.pending` (text, one name per line) in
  the data folder (`/data` on the box, `solite-data/` on the Mac), present only between a restore
  and the next start.
- `Home` gets a backup state (idle, preparing, exporting, restoring, pending; plus the Saver's
  `writing` mark) shared with the ticket (an `Arc` of an
  atomic or a small mutex, as `Updates`' `State` and `Guard`): the gate refuses a second job with
  409; the ticket's drop sets it back to idle; the Saver reads it under the lock it already takes
  and writes nothing while exporting.

## Code

### Core (`crates/core`)

- New module `src/backup/` (`mod.rs`: constants, names, `BackupFiles` trait, errors; `format.rs`:
  header, entry and manifest encoding and parsing; `export.rs`: `ArchiveReader`; `restore.rs`:
  `restore`, `apply_pending`; `crypto.rs`: key derivation, ChaCha20 per entry, HMAC). Reuses
  `auth::pbkdf2_sha256`, `auth::{MIN_ITERATIONS, MAX_ITERATIONS, PBKDF2_ITERATIONS,
  MIN_PASSWORD_CHARS, MAX_PASSWORD_CHARS}`, `auth::ct_eq` for the key check and MAC comparison,
  the hex helpers.
- `BackupFiles` (adapter trait):
  ```rust
  pub trait BackupFiles {
      /// Opens the live file of an archive name for reading; None when absent.
      fn open(&mut self, name: &str) -> Result<Option<Box<dyn Read + '_>>, String>;
      /// Creates (truncates) the staged copy of `name` and returns its writer.
      fn stage(&mut self, name: &str) -> Result<Box<dyn Write + '_>, String>;
      /// Removes every staged copy and the marker.
      fn discard(&mut self) -> Result<(), String>;
      /// Writes the marker naming the staged files and the home files to remove.
      fn commit(&mut self, marker: &str) -> Result<(), String>;
      /// The marker's text, if any.
      fn pending(&mut self) -> Result<Option<String>, String>;
      /// Replaces the live file by its staged copy (no-op when the copy is gone).
      fn promote(&mut self, name: &str) -> Result<(), String>;
      /// Removes the live file of an archive name (absent: no-op).
      fn remove(&mut self, name: &str) -> Result<(), String>;
      /// Removes the marker.
      fn clear_pending(&mut self) -> Result<(), String>;
  }
  ```
  The implementer may refine signatures (record the change in `plan.md`); the semantics of R5 and
  R6 are fixed.
- `api/mod.rs`: `is_streamed_request(method, path)` (update or backup restore: the servers do not
  read the body); `Reply::BackupExport(BackupTicket)` and `Reply::BackupRestore { ticket, wifi }`
  returned by `handle_guarded` after the gate and the checks of R1, the lock released; `handle`
  (plain) answers 501 for both, like the update. The `/backup` route and `api/backup.rs`
  removed; `/system/backup` added to `ROUTES` and the system routes (405 for other methods).
- `settings/backup.rs` and `Home::export_backup` / `import_backup` removed, with their tests.
- `engine/saver.rs`: sets the `writing` mark under the lock for the whole of a file's write; no
  write starts while the backup state is exporting.
- `radio/backup.rs`: `parse(text) -> Option<RestoredNetwork>` (PAN, extended PAN, channel, keys).
  `radio/coordinator.rs`: `start_network` takes `restored: Option<&RestoredNetwork>`; `form` uses
  its values instead of random ones, `Network.formed` true, the `on_formed` callback as today.
- `Cargo.toml`: `chacha20 = { version = "0.10", default-features = false, features = ["cipher"] }`
  (pure Rust, no_std, two small crates more: `cipher`, `inout`; the version already in the
  workspace's lock through `rand`).

### Host (`crates/host`)

- `backup_files.rs` (new): `BackupFiles` over the data folder (`network.json` for `zigbee.json`,
  no `wifi.json` on the Mac); `.restore` files mode 0600, renames.
- `serve.rs`: the body is not read for `is_streamed_request`; `BackupExport`: reads
  `X-Backup-Password`, builds the `ArchiveReader` (error: whole JSON answer), answers it as a
  chunked `tiny_http::Response` with the two headers; `BackupRestore`: like `answer_update`
  (declared length bound, `take(MAX_ARCHIVE_BYTES + 1)`), closes after a refusal, prints the
  restart line after success. every command that reads the data folder or starts the radio (`--serve`, `--pair`,
  `--listen`) calls `apply_pending` first.
- The radio start (`--radio`, `--pair`, `--serve` with a radio) passes the parsed `network.json`
  to `start_network`.

### Firmware (`firmware/`)

- `storage.rs` (or a new `backup_files.rs`): `BackupFiles` over `/data` with `std::fs` (the FAT
  rename rule: remove the destination first); counted in the flash write counter like every
  write.
- `http_server.rs`: the streamed routes skip the 16 KB body read (as the update); export streams
  the `ArchiveReader` with `send_chunk` (4 KB buffer from PSRAM, allocated per request); restore
  reads through `BodyReader` (Content-Length required, 411 otherwise) and restarts after a 200
  (`ota::restart_after_update` or a shared restart).
- `main.rs`: `apply_pending` right after `mount_storage()`, before the network setting, the
  console, the Wi-Fi and the engine, for every image (R6).
- `radio_engine.rs`: `start_network` gets the parsed `zigbee.json`.

### Interface (`ui/`)

- `api/settings.ts`: `exportBackup(password)` (fetch with the token and `X-Backup-Password`,
  returns a Blob), `restoreBackup(file, password, wifi)` (POST octet-stream), removing the old two.
- `components/settings/SettingsPage.tsx`: the `BackupSection` rebuilt (R9), reusing the update
  card's file input and the factory reset's `confirmRequired` handling and Modal.

### Scripts

- Tests that use the old route, changed or removed: `crates/core/tests/{backup,memory_worst_case,
  recipe_settle,settings,lan_api,sowel_contract}.rs`, `ui/src/components/settings/
  SettingsPage.test.tsx`, `crates/host/tests/box_session_script.rs`.
- `scripts/box-session.sh`, home group, step 1: `GET /api/v1/system/backup` with a test password
  (200, the file starts with `SOLITEBK`), then `POST` of it with a wrong password (400 `Wrong
  backup password`, nothing changed, no restart). The fake (`crates/host/tests/
  box_session_script.rs`, `home_fake`) answers the new paths.

## Reused

- The update's streaming (spec 032): `Reply::Update`, `answer_update`, `BodyReader`, the 411 rule,
  the restart after the answer, the `peak_alloc` memory test.
- Spec 029: PBKDF2, `ct_eq`, the gate; D48's Presence check of the factory reset.
- The list streaming (`ChunkReader`, `send_chunk`) for a chunked download.
- `radio/backup.rs`'s text format for `zigbee.json`.

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/src/backup/*` | New: format, export, restore, apply, crypto, trait |
| `crates/core/src/api/{mod,system}.rs`, `api/backup.rs` | New streamed replies; old route removed |
| `crates/core/src/settings/backup.rs`, `home/settings_ops.rs` | Old document removed |
| `crates/core/src/engine/saver.rs`, `home/mod.rs` | Backup state, Saver held while exporting |
| `crates/core/src/radio/{backup,coordinator}.rs` | Parse the backup file; form a restored network |
| `crates/core/Cargo.toml` | `chacha20` |
| `crates/host/src/{serve,backup_files,main,...}.rs` | Adapter, routes, apply at start |
| `firmware/src/{http_server,storage,radio_engine,simulated}.rs` | Adapter, routes, apply at start |
| `ui/src/api/settings.ts`, `ui/src/components/settings/SettingsPage.tsx` (+ tests) | Card |
| `scripts/box-session.sh`, `crates/host/tests/box_session_script.rs` | The new check |
| docs (living spec, architecture, memory budget, sowel subset, decisions, roadmap, index) | Deltas |
