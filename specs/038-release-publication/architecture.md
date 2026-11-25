# Architecture: Spec 038

## Overview

```
tag vX.Y.Z pushed ──► release.yml
                        job firmware (contents: read, no environment, no secret)
                          toolchains (as ci.yml firmware job)
                          scripts/release.sh X.Y.Z --build-only --out $RUNNER_TEMP/build
                            checks (version · public key · notes · out folder, no key)
                            build: ui → radio ELF (public key compiled in and checked,
                                     target/release-radio) → ELF checks → app.bin (fw_elf2image)
                                     → table, bootloader → flash.bin (esptool merge_bin, at 0x0)
                          upload-artifact firmware (app.bin, flash.bin)
                        job sign (needs firmware, contents: read, environment release: tags v* only)
                          secret present? (then unset) public key committed? tag on main? ── no ──► fail
                          cargo build -p solite-host --release (no ESP toolchain, no npm)
                          download-artifact firmware
                          scripts/release.sh X.Y.Z --key-env SOLITE_UPDATE_KEY --prebuilt <folder>
                                                  --out $RUNNER_TEMP/release
                            0 SOLITE_UPDATE_KEY read with printenv into an unexported variable, unset
                            1 checks: version (3 files) · key · public key · notes · out folder
                              · app.bin carries the committed public key
                              · probe: 16 bytes signed and verified (key matches?)
                            3 sign: solite-host --update-sign  → solite-X.Y.Z.sup
                            4 verify: solite-host --update-verify --public-key <committed>
                            5 manifest.json · SHA256SUMS · notes.md · key-text search (staging, log)
                            6 staging → out (all or nothing)
                          upload-artifact
                        job publish (needs sign, contents: write, tag only, no secret)
                          download-artifact → gh release create vX.Y.Z <4 assets> --notes-file notes.md

workflow_dispatch ──► job dry-run only (no environment, no secret), throwaway key,
                      artifact dry-run-<sha> = manifest.json + SHA256SUMS, nothing published
```

The release's signature is checked by the same core code the box runs (`check_header`, the
digest), through the host's new `--update-verify`.

## Data

| Name | Type | Where |
| ---- | ---- | ----- |
| `release/update-key.pub` | 64 hex + newline, the Ed25519 public key | committed by the maintainer (absent until the first real key) |
| `SOLITE_UPDATE_KEY` | the key file's text (64 hex) | secret of the GitHub environment `release` (tags `v*`), set by the user |
| `docs/release-notes.md` | one `## x.y.z (date)` section per release | repository |
| `manifest.json` | R2 of the spec | release asset, `releases/latest/download/manifest.json` |
| `SHA256SUMS` | `<hex>  <name>` lines | release asset |
| `solite-X.Y.Z.sup` | spec 032's update file | release asset |
| `solite-X.Y.Z-flash.bin` | bootloader + table + application, from `0x0` | release asset |

## Files changed

| File | Change |
| ---- | ------ |
| `scripts/release.sh` | New: R1 (sources `scripts/lib/firmware.sh` for `fw_find_tools`, `fw_build_table`, `fw_find_bootloader`, `fw_elf2image`, `fw_size_line`) |
| `.github/workflows/release.yml` | New: R3 (jobs `firmware`, `sign`, `dry-run`, `publish`) |
| `.github/workflows/ci.yml` | `shared-key: firmware` on the firmware job's rust-cache (R3.2) |
| `.github/actions/firmware-toolchain/action.yml` | New: the toolchains of `ci.yml`'s firmware job as a composite action, caches restored only (R3.2) |
| `crates/host/src/update_cli.rs` | `Command::Verify { file, public_key }` and `verify()` (R4), with `check_header` and `image_digest` of the core |
| `crates/host/src/main.rs` | `--update-verify <file>` and `--public-key <hex>` parsed, usage line |
| `crates/host/tests/update_cli.rs` | The `--update-verify` scenarios |
| `crates/host/tests/release_script.rs` | New: the script's scenarios with a fake builder (as `box_session_script.rs`'s `SOLITE_SESSION_BUILDER`) and a workflow text check |
| `docs/release.md` | New: key custody (R5), first flash from a release |
| `docs/release-notes.md` | New: the notes' file, one section per version (empty of versions until the first release) |
| `docs/living-spec/README.md` | "Signing a release" → "Publishing a release"; `--update-verify` |
| `docs/decisions.md` | D63 (the release flow), D64 (custody and rotation) |
| `docs/architecture.md` | The release script and workflow, `release/` |
| `docs/roadmap.md` | J5b row (three steps, step 1 built), inbox lines removed or narrowed, log line |
| `docs/specs-index.md` | Row 038 |
| `README.md` | "Install from a release" |
| `CLAUDE.md` | One command line: `sh scripts/release.sh` |
| `.claude/skills/solite-release/SKILL.md` | R6 |
| `release/README.md` | New: what the folder holds (the public key, committed by the maintainer) |

## Reused

- `scripts/lib/firmware.sh`: the image, the table, the bootloader, the size line (as `box.sh flash`
  and `box-session.sh`'s `own_builder`).
- `box.sh`'s radio ELF checks (the markers and the sdkconfig line), repeated in the script's own
  builder.
- `box-session.sh`'s `SOLITE_SESSION_BUILDER` seam and its tests' fake builder.
- `crates/host/tests/update.rs`'s `keyed_host()` (a host built again with a public key) for the
  demo's install check.
- `ci.yml`'s `firmware` job steps, copied into `release.yml` (the same keys of cache).
