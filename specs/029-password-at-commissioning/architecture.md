# Architecture: Spec 029

## Overview

```
HTTP request (host serve.rs / firmware http_server.rs)
  └─ api::handle_guarded(&Mutex<Home>, request, now, random)
       ├─ Host check (403), interface files (public, as today)
       ├─ /api/v1/auth/*  → auth routes: prepare (lock) → hash (no lock) → finish (lock)
       ├─ gate (lock): no account 403 Setup required; bearer missing/invalid 401
       └─ handle_stream(&mut Home, ...) as today (system/factory-reset among its routes)

/ws upgrade (host ws.rs / firmware live_ws.rs)
  └─ live::session::bearer_token(subprotocol lines, authorization) → Home::access_valid(token, now)
       → close 4001 when missing or invalid

Saver (core engine/saver.rs) ── auth.json (account + hash) ── Storage (Mac folder / FAT /data)
Presence adapter (core trait) ── host: AlwaysConfirmed; firmware: BootPresence (10 min after power-on)
```

## Data

| Name | Type | Where | Bound |
| ---- | ---- | ----- | ----- |
| `Account` | id, username, displayName, language, createdAt, `PasswordHash` | `Home` (one `Option`), `auth.json` | one |
| `PasswordHash` | scheme `pbkdf2-sha256`, iterations (u32, 1 000..=1 000 000), salt `[u8; 16]`, hash `[u8; 32]` | in `Account` | fixed size |
| `Session` | access `[u8; 16]`, refresh `[u8; 16]`, accessUntil, refreshUntil, openedAt (ms) | `Home`, RAM only | `MAX_SESSIONS` = 8 |
| attempts | the times of the last attempts (ms) | `Home`, RAM | `MAX_ATTEMPTS` = 10 |
| `lastLoginAt` | `Option<u64>` | `Home`, RAM | one |
| presence | `Box<dyn Presence + Send>` | `Home` | one |
| `auth_changed` | change flag | `Home` | — |

## Files changed

| File | Change |
| ---- | ------ |
| `Cargo.toml` (workspace) / `crates/core/Cargo.toml` | `sha2` 0.11, `hmac` 0.13, `pbkdf2` 0.13 (`default-features = false`, `hmac` feature); dev profile: these three at `opt-level = 3` so the debug tests hash quickly |
| `crates/core/src/auth/mod.rs` (new) | `Account`, `PasswordHash` (hash, verify, constant-time compare), `Sessions` (open, access_valid, refresh, close, shift_before_clock, bound), `Attempts` (rate limit), `Presence` trait, `AlwaysConfirmed`, `NeverConfirmed`, `BootPresence`, `ButtonPresence`, the file's parse and write, constants (`PBKDF2_ITERATIONS`, `ACCESS_TTL_S`, `REFRESH_TTL_S`, `MAX_SESSIONS`, `MAX_ATTEMPTS`, `CONFIRM_WINDOW_MS`, `BOOT_CONFIRM_MS`) |
| `crates/core/src/lib.rs` | `pub mod auth;` |
| `crates/core/src/home/auth_ops.rs` (new) | `Home`'s side: the account, sessions, attempts, presence fields; `load_auth`, `take_auth_cursor`, `mark_auth_changed`, `auth_changed`, `access_valid`, `set_presence`, `factory_reset` (R7, through `delete_recipe_instance`, `delete_equipment`, `remove_device`, `delete_zone`, the settings and root reset); `observe_wall` (`home/mod.rs:232`) also shifts the sessions |
| `crates/core/src/home/mod.rs` | the new fields in `Home` (`:89`) and `from_parts` |
| `crates/core/src/home/cursor.rs` | `ListCursor::saved_auth` (one chunk), as `saved_settings` (`:152`) |
| `crates/core/src/engine/storage.rs` | `AUTH_FILE = "auth.json"`, `load_auth`, read in `load_home` |
| `crates/core/src/engine/saver.rs` | `auth.json` in the definitions' files (`:252`) and in `definitions_unsaved` |
| `crates/core/src/api/mod.rs` | `Request.authorization: Option<&str>`; `handle_guarded`; `ROUTES` gains the five `/auth` routes and `POST /api/v1/system/factory-reset`; `dispatch` routes `auth` (unguarded `handle`, for tests and the contract check) through the same three phases; the `Reach::Lan` 404 of `integrations` goes (`:372`) |
| `crates/core/src/api/auth.rs` (new) | the five routes split in `prepare` / `hash` / `finish`; the gate; `AuthTokens` and `User` JSON |
| `crates/core/src/api/system.rs` | `POST factory-reset` (presence, 409 during pairing, `Home::factory_reset`); `route` takes `&mut Home` |
| `crates/core/src/live/session.rs` | `bearer_token(protocol lines, authorization) -> Option<String>` (R5.1) |
| every `Request { .. }` literal (about 30, core tests, host, firmware) | `authorization: None` (or the header) |
| `crates/host/src/serve.rs` | reads `Authorization`, calls `handle_guarded` instead of lock + `handle_stream` (`:161`); sets `AlwaysConfirmed` on the home |
| `crates/host/src/ws.rs` | `Context.home`; after the upgrade, 4001 when the token is missing or invalid (`serve_request`, `:160`) |
| `crates/host/tests/*.rs` (serve, ws, static_files, listen, demo_script, measure_script) | the helpers set the account up and send the token; new tests of R3 to R5 over HTTP |
| `firmware/src/http_server.rs` | `Head.authorization`, `handle_guarded` (`:412`) |
| `firmware/src/live_ws.rs` | `Live.home`; `after_handshake` reads `Sec-WebSocket-Protocol` and `Authorization` whole, 4001 (`:191`) |
| `firmware/src/radio_engine.rs` | `home.set_presence(BootPresence::new(reset_reason() == PowerOn, uptime clock))` after `load_home` (`:298`); `esp_log_level_set("httpd_ws", ESP_LOG_ERROR)` before `/ws` is registered |
| `scripts/e2e-sim.sh` | waits for `/api/v1/auth/status` (`:58`) |
| `scripts/demo-garage.sh`, `scripts/measure-api.sh` | R10 |
| `ui/src/api/auth.ts`, `ui/src/store/useAuth.ts` (new) | Sowel's, cut to R9 |
| `ui/src/api/client.ts` | the bearer and the refresh on 401 (Sowel's) |
| `ui/src/store/useLive.ts` | the two subprotocols, 4001 |
| `ui/src/components/auth/SetupPage.tsx`, `LoginPage.tsx` (new) | Sowel's pages |
| `ui/src/App.tsx` | the gate: status → setup / login / home |
| `ui/src/components/layout/Sidebar.tsx` | "Log out" |
| `ui/src/components/settings/SettingsPage.tsx` | the factory reset card |
| `ui/src/i18n/messages.ts`, `ui/src/types.ts` | texts; `User`, `AuthTokens` |
| `tools/e2e/src/*` | the browser test goes through the first-start screen |
| docs | living spec, decisions D47 to D49, roadmap, specs-index, sowel-subset, architecture, memory-budget (sessions, attempts), README, CLAUDE.md |

## Reused

- The settings' file pattern (spec 026): change flag, `take_*_cursor`, saver list, `load_*`.
- `observe_wall` and D45's shift for the sessions' dates.
- `api::input` for the bodies; `api::failure` for `{error, message}`; `Failure` codes.
- `static_reply`'s precedent for answering without the home's lock.
- `orders().set_clock(Arc<...>)`'s precedent for an adapter held by the home (`set_presence`).
- The firmware's `reset_reason()` (`radio_engine.rs:156`) and D32's power-on reasons.
- Sowel's `SetupPage`, `LoginPage`, `useAuth`, `client.ts` (UI).
