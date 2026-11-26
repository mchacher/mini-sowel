# Architecture: Spec 039

## Overview

```
API request (host worker / box worker)
  └─ core handle_guarded ── under the home's lock: gate, safe mode, clock, form of the update route
       ├─ Reply::VersionCheck(ReleaseTicket)  POST /system/version/check
       └─ Reply::UpdatePull(ReleaseTicket)    POST /system/update, empty body
caller, lock released (box: on an `update-check` / `update-pull` thread, 20 KB internal RAM)
  └─ core update::release
       ├─ ReleaseTicket::check(fetcher, pacing)           -> Response (VersionInfo or error)
       └─ ReleaseTicket::pull(updates, fetcher, adapter, pacing) -> Response
            manifest (cache or fetch) ─ rules ─ fetch image.url ─ CountingHasher reader
            └─ Updates::install_expecting(source, adapter, Some(Expected{size, sha256}))
                 (spec 032's pipeline; the expected size and digest checked before finish)
Fetcher (adapter trait, one request, no redirect followed):
  host: http by TcpStream + httparse, https by the `curl` program
  box:  EspHttpConnection + esp_crt_bundle_attach (common bundle), FollowNone
```

The core follows redirects, applies the URL rule, bounds sizes and times, parses the manifest and
holds the cache; the adapters only move bytes.

## Data

- `update::release::Releases` (in `Home`, behind an `Arc`, like `BackupState`): a `Mutex` of
  `{ cache: Option<Checked>, busy: bool, last_api_check_ms: Option<u64>, next_auto_ms: u64 }`.
  `Checked { at_ms (monotonic), latest: Version, min_version: Option<Version>, url: String (≤ 2048),
  size: u64, sha256: [u8; 32], release_url: Option<String> }`. One entry: about 2.2 KB at most.
- `ReleaseTicket`: the `Arc<Releases>`, the manifest URL (from the settings, read under the lock),
  holding `busy` until dropped (a guard, so every path out clears it).
- Settings: two keys, `update.manifestUrl` (≤ 512 characters), `update.autoCheck`; `KEYS` 6 → 8.
- `api::Request` gains `content_length: Option<u64>` (the declared length; `Some(0)` also for a
  request with neither `Content-Length` nor `Transfer-Encoding`; `None` for a chunked body).
- No file format changes besides `settings.json`'s two optional keys.

## Adapter trait (core, `update::release`)

```rust
pub enum Fetched<'a> {
    /// 200: the declared length if any, and the body.
    Body { length: Option<u64>, body: Box<dyn Read + 'a> },
    /// 301, 302, 303, 307, 308: the `Location` header, if any.
    Redirect(Option<String>),
    /// Any other status.
    Status(u16),
}
pub trait Fetcher {
    /// One GET of `url` (already checked by the core), each read bounded by `idle_ms`.
    /// Errors: a message (connection refused, TLS failure, timed out...).
    fn get(&mut self, url: &str, idle_ms: u64) -> Result<Fetched<'_>, String>;
}
```

`Limits { idle_ms, total_ms }` with `Limits::CHECK` and `Limits::DOWNLOAD`, passed explicitly so
the tests use short ones. `backup::Pacing` is reused for the monotonic time and the pause.

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/src/update/release.rs` (new) | Manifest parsing (R1), URL rule (R3), redirects, `Releases`, `ReleaseTicket::{check, pull}`, the counting-hashing reader, `version_info()`, `auto_due`, constants |
| `crates/core/src/update/install.rs` | `install` keeps its signature; `install_expecting(source, adapter, Option<Expected>)` checks the count and digest before `finish`; `Updates::running()`; a pull marks the state `Installing` from its start (the manifest and the download included) |
| `crates/core/src/update/mod.rs` | `pub mod release` re-exports |
| `crates/core/src/api/mod.rs` | `Request.content_length`; `Reply::VersionCheck`, `Reply::UpdatePull`; the update route's two forms; the clock refusal; safe-mode exception for `/system/version/check`; `ROUTES` + `POST /api/v1/system/version/check`; `private_ipv4` shared with the URL rule |
| `crates/core/src/api/system.rs` | `version()` from the cache; 405 for other methods of `version/check` |
| `crates/core/src/home/*` | `Home` holds `Arc<Releases>`; `release_ticket()`, `auto_check_ticket(wall_ms, mono_ms)` |
| `crates/core/src/settings/mod.rs` | Two keys and their rules |
| `crates/core/tests/release.rs` (new) | Core logic with a fake `Fetcher` (scenarios of the plan) |
| `crates/core/tests/*` | `content_length` added to every `Request` literal |
| `crates/host/src/fetch.rs` (new) | `HttpFetcher` (http, https by `curl`); its tests with a test HTTP server in the test thread |
| `crates/host/src/serve.rs` | `content_length` from the request; `Reply::VersionCheck` / `UpdatePull` in `answer_streamed`; the automatic check in the watch thread's loop (a thread per check) |
| `crates/host/tests/update.rs` | End-to-end: the keyed host, a test server, check then pull |
| `firmware/src/fetch.rs` (new, `radio`) | `EspFetcher` on `EspHttpConnection`, the bundle, buffers 4096/1024 |
| `firmware/src/http_server.rs` | `content_length` (0 when absent and not chunked); `VersionCheck` / `UpdatePull` on `update-check` / `update-pull` threads; restart after a successful pull |
| `firmware/src/job_thread.rs` | `FETCH_STACK_BYTES` (20 KB); a `spawn` variant that does not wait (the automatic check) |
| `firmware/src/radio_engine.rs` | Asks `auto_check_ticket` about once a minute; spawns the check thread |
| `firmware/src/update_check.rs` (`sim`) | The pull from an in-memory fetcher on an `update-pull` thread (R10) |
| `firmware/sdkconfig.radio.defaults` | `CONFIG_MBEDTLS_CERTIFICATE_BUNDLE=y`, `..._DEFAULT_CMN=y` |
| `scripts/qemu.sh` | The `update-pull` stack and status check |
| Docs | living spec, `docs/sowel-subset.md`, `docs/memory-budget.md`, `docs/decisions.md` (D65 to D69), `docs/architecture.md`, `docs/roadmap.md`, `docs/specs-index.md`, `docs/release.md`, `README.md` |

## Reused

- Spec 032's `Updates::install` pipeline (header check, streamed hash, abort, finish) and its
  `UpdateAdapter`s (host files, box OTA thread).
- D59/D60's `job_thread::run`, `Lent`, `after_job_thread`; the restart after an update.
- Spec 035's `BackupTicket` pattern (a ticket taken under the lock, an `Arc` state, a guard) and
  `backup::Pacing` (monotonic time, pause; `BoxPacing`, `HostPacing`).
- `CLOCK_SET_AFTER_MS` (home), `api::private_ipv4`.
- The host's test style of `crates/host/tests/update.rs` (the keyed host built with a run-time key).
