# Plan: Spec 038

## Steps

- [x] 1. Living spec updated ("Publishing a release", `--update-verify`)
- [x] 2. Core types: not applicable (the manifest is written by the script; the core's update
      format is unchanged)
- [x] 3. Core logic: not applicable (`check_header` and `image_digest` reused as they are)
- [x] 4. Host: `--update-verify` (`update_cli.rs`, `main.rs`) and its tests
- [x] 5. `scripts/release.sh` and `crates/host/tests/release_script.rs`
- [x] 6. `.github/workflows/release.yml`
- [x] 7. API, interface, firmware code: not applicable
- [x] 8. Docs: `docs/release.md`, `docs/release-notes.md`, `release/README.md`, decisions D63 and D64,
      architecture, roadmap, README, CLAUDE.md, the release skill, specs index; "No longer true"
      searched and corrected
- [x] 9. One real run on the Mac with a throwaway key (the demo)

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| `--update-verify` | A file signed by the matching key | exit 0, `verified: <file>, version 0.2.0, <n> bytes, key <8 hex>` |
| `--update-verify` | Another key's public key | exit 2, `update-verify:` and the signature refusal |
| `--update-verify` | One byte of the image changed | exit 2, the digest differs |
| `--update-verify` | The file truncated in the image | exit 2, shorter than the header says |
| `--update-verify` | One byte appended | exit 2, longer than the header says |
| `--update-verify` | A public key of 63 hex characters, or not hex | exit 2, not a public key |
| `--update-verify` | A missing file | exit 2, cannot read |
| `--update-verify` | `--update-verify` without `--public-key` | exit 2, usage |
| `release.sh` | Nominal, fake builder, `--key-file` | exit 0; exactly the five files; lines of R1.9 |
| `release.sh` | Nominal, `--key-env` | exit 0; same files; the temporary key file is gone after the run |
| `release.sh` | The manifest | `format` 1, `version`, `releasedAt` = `SOLITE_RELEASE_TIME`, `notes` = the section, `minVersion` null, `image.name`, `image.size` and `image.sha256` of the `.sup`, `image.url` with `--repo` and the tag |
| `release.sh` | `--min-version 0.1.0` | `minVersion` "0.1.0"; `--min-version` equal to the version refused |
| `release.sh` | `SHA256SUMS` | three lines sorted by name; `shasum -a 256 -c` passes in the output folder |
| `release.sh` | The `.sup` | accepted by `--update-verify` with the public key; version and image bytes those of the fake build |
| `release.sh` | A tag `v<version>` as argument | accepted, same output |
| `release.sh` | The key never in the output | the key's hex in no output file and not in stdout or stderr, `--key-file` and `--key-env` |
| `release.sh` | Version different from `Cargo.toml` | exit 2 naming `Cargo.toml`; builder not run; output folder absent |
| `release.sh` | `firmware/Cargo.toml` or `ui/package.json` different | exit 2 naming that file |
| `release.sh` | Version not `x.y.z` | exit 2 |
| `release.sh` | No key option (an unsigned run) | exit 2, "a release is always signed"; builder not run |
| `release.sh` | `--key-env` naming an empty or unset variable | exit 2, "the signing key is missing" |
| `release.sh` | Both `--key-file` and `--key-env` | exit 2 |
| `release.sh` | A key file that does not exist | exit 2 |
| `release.sh` | The public key file missing, or not 64 hex | exit 2 naming it |
| `release.sh` | A key that does not match the public key | exit 1, "does not match"; output folder absent |
| `release.sh` | No notes section for the version; `--notes <file>` given | exit 2 before the build; with `--notes`, its text in the manifest |
| `release.sh` | The output folder exists and is not empty | exit 2, its files unchanged |
| `release.sh` | The builder fails (exit 1, no `app.bin`) | exit 1 with the log's path; output folder absent |
| `release.sh` | `--key-env`: the builder's environment (the fake builder writes `env` to a file) | the variable's name and the key's text are not in it |
| `release.sh` | While the fake builder runs | the key folder holds no file (the builder lists it) |
| `release.sh` | A failed run: the log folder | kept, its path printed, the key not in it; the key folder gone |
| `release.sh` | The step lines | the "key matches" and "signed and verified" lines carry the public key's first 8 hex |
| `release.sh` | The key with trailing spaces and a newline; the public key file with a trailing newline | accepted |
| `release.sh` | Notes: a dated heading `## <v> (2026-12-01)` | its section taken; a heading `## <v>1` is not `<v>`'s |
| `release.sh` | An existing empty `--out` | accepted |
| `release.sh` | `--min-version` above the version, or malformed | exit 2 |
| `release.sh` | A version `0.2.0-rc1` | exit 2 |
| `release.sh` | The key file inside `--out`, or inside a git working tree | exit 2 |
| `release.sh` | `--key-env` with a name like `A-B` | exit 2 |
| `release.sh` | Interrupted (SIGINT to the script's process group during a fake build whose child is `sleep 47`) | the key folder and the staging folder gone, no output folder, the sleep gone |
| `release.sh` | `--build-only`: output; with a key option or `--prebuilt`; the checks that need no key | only `app.bin` (holding the public key) and `flash.bin`, the line `built, not signed`, no host program needed; exit 2 with `--build-only` in the message, builder not run |
| `release.sh` | `--prebuilt`: nominal; `app.bin` without the public key; a missing file or folder | five files, builder not run, `.sup` holds the prebuilt image; exit 1 "does not carry"; exit 2 |
| `release.sh` | The signed image has no application descriptor, builder not the seam (a wrapper host prints the line) | exit 1 "no application descriptor" |
| `release.sh` | `--key-env` naming a variable the script uses (`KEY`, `OUT`, `VERSION`); unset | works; unset: exit 2 |
| `release.sh` | The key does not match: the probe | exit 1 before the builder runs |
| `release.sh` | `SOLITE_RELEASE_BUILDER` with `GITHUB_ACTIONS=true` | exit 2 |
| `--update-verify` | A file shorter than 128 bytes | exit 2 |
| `release.yml` | The workflow's text | `contents: write` only under `publish`; `SOLITE_UPDATE_KEY` only in the `sign` job's refusal step and `release.sh` step (`firmware` has no environment and no secret; `sign` builds nothing but the host); `sign` has `environment: release`; `firmware`, `sign` and `publish` only on a tag push; `unset` right after the emptiness test; the concurrency group per event; no `cache:` line (setup-node) and npm's cache key; the action's espup version, Xtensa cache key and `rust-cache` pin equal `ci.yml`'s; `dry-run` only on `workflow_dispatch` and with no secret; the `merge-base --is-ancestor` check; the `::error::` text; `manifest.json` among the assets; no `save` of caches |
| Existing | `--update-keygen`, `--update-sign` and the update tests | unchanged, green |

## Validation

- [x] Validate command green, QEMU included
- [x] Agent review done, blocking findings fixed
- [x] Demo: the real run on the Mac and the install by `POST /api/v1/system/update`
