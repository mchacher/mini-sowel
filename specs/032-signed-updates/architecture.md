# Architecture: Spec 032

## Overview

```
browser / curl ── POST /api/v1/system/update (application/octet-stream, bearer)
      │
host serve.rs / firmware http_server.rs: is_update_request? ── yes ── body NOT read yet
      │                                                               │
      └── api::handle_guarded (Host check, auth gate, content type) ── Reply::Update
                                                                      │
                    update::Updates::install(body reader, adapter)  ◄─┘
                      header 128 B ─ magic, format, Ed25519 over bytes 0..64 (build key),
                                     version > VERSION, length ≤ capacity
                      begin(len) ─ loop { read ≤ 4096 B → sha256.update → write } ─ digest ==?
                      finish() → boot record: inactive slot, pending          → 200 {success,message}
                      else abort() → boot record unchanged                    → 4xx/5xx {error}

boot ── adapter.pending()? ── Probation::tick(monotonic, healthy) each second (Updates::watch)
          healthy 60 s → confirm()      not confirmed at 300 s → roll_back()
```

Adapters: `crates/host/src/update_files.rs` (files under `<data>/update/`), `firmware/src/ota.rs`
(`EspOta`, calls proxied to one internal-RAM thread), a fake in `crates/core/tests/support/`.

## Data

| Name | Type | Where |
| ---- | ---- | ----- |
| Update header | 128 bytes (R1) | first bytes of the `.sup` file |
| `HEADER_BYTES` | 128 | `crates/core/src/update/mod.rs` |
| `CHUNK_BYTES` | 4096 | same: the one buffer of an installation (bound) |
| `CONFIRM_AFTER_MS`, `ROLLBACK_AFTER_MS` | 60 000, 300 000 | same |
| `Version` | (u16, u16, u16), `Ord` | same |
| `Updates` state | idle / installing / installed, plus `on_probation` | in RAM, one per program |
| `boot.json` (host) | `{"boot": 0\|1, "pending": bool}` | `<data>/update/` |
| `slot-<n>.bin` (host) | the image | `<data>/update/`, at most 4 MB |
| `SOLITE_UPDATE_PUBLIC_KEY` | 64 hex, build-time env | read by `option_env!` in host and firmware |

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/Cargo.toml` | `ed25519-dalek` (verification only, `default-features = false`, no precomputed tables); `sha2` already there |
| `crates/core/src/update/mod.rs` (new) | `Version`, header parse and check, `signed_bytes` (bytes 0..64 to sign), `parse_public_key`, `UpdateAdapter` trait, `Updates` (state, `install`, `watch`), `Probation`, error → `Response` mapping |
| `crates/core/src/lib.rs` | `pub mod update;` |
| `crates/core/src/api/mod.rs` | `Reply::Update`; `is_update_request(method, path)`; route of `POST /system/update` before the 16 KB and JSON checks in `handle_guarded` (after the gate) and in `dispatch` (501 there); `ROUTES` += the two routes |
| `crates/core/src/api/system.rs` | `GET /system/version`; module comment corrected |
| `crates/core/tests/update.rs` (new) | the test plan's core scenarios, with a fake adapter and key pairs made at run time |
| `crates/core/tests/support/` | the fake adapter (in memory, with a "power cut after n bytes" and failing-call switches, and a counting mode that keeps no bytes for the memory test) |
| `crates/core/tests/sowel_contract.rs`, `docs/sowel-subset.md` | the two routes, the deviation (D53) |
| `crates/host/Cargo.toml` | `ed25519-dalek` (signing) |
| `crates/host/src/update_files.rs` (new) | the file adapter (R7) |
| `crates/host/src/main.rs` | `--update-keygen`, `--update-sign` |
| `crates/host/src/serve.rs` | build `Updates` from `option_env!`; stream `Reply::Update` from tiny_http's reader without reading the body first; probation ticks in the radio loop's thread or a small thread |
| `firmware/Cargo.toml` | nothing new (`esp-idf-svc` has `ota`) |
| `firmware/src/ota.rs` (new) | `EspOta` adapter behind an internal-RAM thread |
| `firmware/src/http_server.rs` | the update path streamed before the 16 KB check |
| `firmware/src/main.rs`, `radio_engine.rs` | `Updates` built from `option_env!`; the probation thread; restart after an installation (engine stop, final save, `esp_restart`) |
| `firmware/build.rs` | `cargo:rerun-if-env-changed=SOLITE_UPDATE_PUBLIC_KEY` |
| `firmware/sdkconfig.defaults` | `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y` |
| `ui/src/api/system.ts` (new or in `settings.ts`) | `getSystemVersion`, `installUpdate(file)` |
| `ui/src/components/settings/SettingsPage.tsx` (+ test) | `UpdateSection` after `BackupSection` |
| `ui/src/i18n/` | labels fr/en |
| `.gitignore` | `*.update-key`, `*.sup` |
| Docs | living spec, `docs/architecture.md`, `docs/decisions.md` (D52 to D54), `docs/roadmap.md`, `docs/specs-index.md`, `docs/memory-budget.md` (the installation's bound) |

## Reused

- The gate and `handle_guarded` of spec 029 (`crates/core/src/api/mod.rs`, `auth.rs`); the
  `Reply::Order` pattern: a reply the caller completes without the home's lock.
- `sha2` (already a dependency for PBKDF2).
- The host's `SystemRandom` (`crates/host/src/random.rs`) for the keygen.
- The file write pattern of `crates/host/src/store.rs` (temporary file then rename).
- `BackupSection` of the Settings page (file input, a POST, a message) for the update card.
- `peak_alloc` (already a dev-dependency) for the memory bound test.
- `firmware/src/http_server.rs`'s `Raw::receive` and its 5 s timeout.
