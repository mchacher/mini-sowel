# Architecture: Spec 040

## Overview

```
Rotation (tooling only; the box's code is unchanged)
  release/update-key.pub (new key)  ─┐
  release/update-key.bridge          ├─ scripts/release.sh (bridge mode)
     version, previous (old pub)     │     build with update-key.pub ─ sign with --key-* (old)
  --next-key-* (new private, probe) ─┘     verify: accepted by previous, refused by update-key.pub
  release.yml sign job: SOLITE_UPDATE_KEY_PREVIOUS signs, SOLITE_UPDATE_KEY probes

Interface
  Settings › Firmware update card ── GET /system/version, POST /system/version/check,
     POST /system/update (no body), poll GET /system/version every 3 s, PUT /settings (whole map)

Resume (core)
  ReleaseTicket::pull ─ fetch(image.url) ─ Download reader ──► Updates::install_claimed
                                      ▲  on a cut: wait 2/4/8 s, fetch again with Range: bytes=<count>-
                                      └─ Fetched::Partial checked (Content-Range, Content-Length, ETag)
                                         same hash context, same count, same slot
```

## Data

- `Fetched::Body` gains `etag: Option<String>`; new `Fetched::Partial { content_range: Option<String>,
  length: Option<u64>, etag: Option<String>, body: Box<dyn Read> }` (206). ETag at most 256 characters
  (a longer one is dropped by the core, not compared).
- `Fetcher::get_range(&mut self, url, from: u64, idle_ms) -> Result<Fetched, String>`, default
  `self.get(url, idle_ms)`.
- `Pacing::wait_ms(&mut self, ms: u64)`, no default (the host and the box sleep, the fakes advance their clock).
- `Download` (the reader) holds the fetcher, the manifest's `url`, the first answer's ETag, the
  attempts used. Constants `RESUME_ATTEMPTS = 3`, `RESUME_WAIT_MS = 2000`, `MAX_ETAG_CHARS = 256`.
- `Releases::forget_cache()` (a changed file drops the cached manifest).
- No file format, no setting, no route changes. `VersionInfo` unchanged.
- Release tree: `release/update-key.bridge` (only during a bridging release).

## Files changed

| File | Change |
| ---- | ------ |
| `scripts/release.sh` | `--bridge-file`, `--next-key-file`, `--next-key-env`; the preamble reads a second key from the environment; bridge checks (R1.3), the next key's probe, the double verify, the key-text checks for both keys |
| `.github/workflows/release.yml` | `sign` job: the bridge file's test, `SOLITE_UPDATE_KEY_PREVIOUS` in the refusal and signing steps, the bridge command |
| `crates/host/tests/release_script.rs` | Bridge mode with the fake builder (R1) |
| `crates/host/tests/release_workflow.rs` | The secret rules extended to the second secret, the normal command unchanged |
| `crates/host/tests/update.rs` (or a new `rotation.rs`) | R3's chain test (`#[ignore]`, slow), R5's settings test, the resume end-to-end through the real server |
| `crates/core/src/update/release.rs` | `Fetched::Partial`, `etag`, `get_range`, the resume in `Download`, `forget_cache`, the log line with the URL |
| `crates/core/src/backup/mod.rs` | `Pacing::wait_ms`, no default; every implementation updated |
| `crates/core/tests/release.rs` | The resume scenarios with the fake fetcher; the fresh-cache check (R7 a) |
| `crates/core/tests/release_memory.rs`, `firmware/src/update_check.rs` | The new `Body` field; QEMU's cut and 206 |
| `crates/host/src/fetch.rs` | `get_range` for http and curl; 206, `Content-Range`, `ETag`; its unit tests |
| `firmware/src/fetch.rs` | `get_range`: the `Range` header, 206 and its headers |
| `scripts/qemu.sh` | The pull's line still checked (a status 200 with the cut) |
| `ui/src/api/settings.ts` | `VersionInfo` complete, `checkSystemVersion`, `pullUpdate`, `saveSettings` (GET then PUT whole) |
| `ui/src/components/settings/SettingsPage.tsx` | The Home card saves through `saveSettings`; `UpdateSection` rewritten (R4); the upload kept |
| `ui/src/components/settings/SettingsPage.test.tsx` | The card's tests with fake timers; the whole-map tests |
| `ui/src/i18n/messages.ts` | The card's texts, English and French |
| Docs | living spec, `docs/release.md`, `docs/design-system.md`, `docs/decisions.md` (D70 to D72), `docs/roadmap.md`, `docs/specs-index.md`, `README.md`, `docs/architecture.md` if a row names the card or the fetcher |

## Reused

- Spec 038's probe (`--update-sign` then `--update-verify --public-key`) for both keys, its key-text
  checks, its fake builder and test fixture.
- Spec 039's `fetch` (redirects, URL rule), `Download` reader, `Audit`, `download_failed`, the Mac test
  server of `crates/host/tests/update.rs` (`serve_release`) and the fake fetcher of
  `crates/core/tests/release.rs`.
- `crates/host/tests/update.rs`'s `keyed_host` build, made a function of the key for R3.
- Sowel's `VersionBadge` and `UpdateOverlay` (commit `2d360726`) for the card; Solite's `Modal`,
  `BUTTON_PRIMARY`, `BUTTON_SECONDARY`, `FIELD_LABEL`, `backupErrorText` pattern for server texts.
