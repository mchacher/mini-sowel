# Architecture: Spec 043

## Overview

- Restart: the core's route answers `Reply::Restart` (beside `Reply::Update`); the core checks the
  guards (update, update check, backup) and the probation's state through what the API already
  reads for updates. The firmware's HTTP server sends 202, then runs `ota::restart_after_update()`
  on the worker after `raw.complete()` (the pattern of `http_server.rs:831-865`), behind a static
  flag so two requests restart once. The host maps it to 501. `safe_mode_refusal`
  (`api/mod.rs:416`) lets the route through.
- PSRAM: `crates/core/src/update/probation.rs` takes `Health`; `Updates::watch` too, with the
  reason's change logged once; `firmware/src/ota.rs`'s `healthy()` fills it; `ota::start()` starts
  the thread without its chunk buffer when PSRAM is absent (calls answer 503).

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/src/api/system.rs`, `api/mod.rs` | the route, `Reply::Restart`, the safe mode's exception |
| `crates/core/src/update/probation.rs`, `install.rs` | `Health`, the reason logged on change, the probation state the route reads |
| `crates/host/src/serve.rs` | 501 for `Reply::Restart`; `Health { psram: true, .. }` |
| `firmware/src/http_server.rs` | `Reply::Restart`: 202 then `restart_after_update`, once |
| `firmware/src/ota.rs` | `healthy()` with the PSRAM; the thread without a buffer |
| `ui/src/components/settings/RestartCard.tsx` (new), its test, `SettingsPage.tsx`, `ui/src/api/`, the locales | the card |
| `docs/` | living spec, sowel-subset, D79, roadmap (two inbox lines done) |

## Reused

`restart_after_update` and `supervisor::asked_restart` (specs 032, 036), the guards of the update
and the backup, the probation, Settings' cards.
