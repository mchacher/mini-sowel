# Spec 038: release-publication

## Context

The user's priority 1, "the box updates itself from a GitHub release", cut in three steps: (1)
**the CI publishes a signed release** (this spec), (2) the box reads the release's manifest and
downloads the update itself, (3) key rotation. Today there is no tag and no GitHub release;
`.github/workflows/ci.yml` builds nothing signed, and the signed update file of spec 032 (D52 to
D54: Ed25519 over a 128-byte header carrying the image's SHA-256, `--update-keygen` and
`--update-sign` in `solite-host`, the public key compiled in from `SOLITE_UPDATE_PUBLIC_KEY`) is
made by hand, following the living spec's "Signing a release". The true north asks for "signed
binaries" and "Sowel users who flash their Dongle Max from a public release": both need a release
that exists, is signed, and is reproducible from a tag.

This run is autonomous (the user is away): every open question is decided here and recorded
(decisions D63 and D64). Nothing is flashed, no tag is pushed, no release or secret is created: the
first real release is the user's, with `/solite-release`.

## Goals

1. One script, `scripts/release.sh`, turns a version, a signing key and the public key into the
   files of a release: the signed update file, a full-flash image for a first USB flash, a
   checksum file and a manifest the box will read. It refuses to make a release that is not signed,
   not signed by the key matching the public key, or whose version is not the tree's.
2. A workflow, `.github/workflows/release.yml`, runs it on a pushed tag `vX.Y.Z` and publishes a
   GitHub release with those files; it refuses to publish without the signing secret. Run by hand
   (`workflow_dispatch`), it is a dry run with a throwaway key that publishes nothing.
3. The key's custody (making it, keeping it, giving it to the CI, what a rotation means) is
   written down, and `/solite-release` matches the new flow.
4. `solite-host --update-verify` checks a signed file against a public key, so the script proves
   its own output before anything is published.

## Out of scope

- The box reading `manifest.json` and downloading the update (step 2): the roadmap row of J5b.
- Key rotation (step 3): the update file's format 1 has one key per build and no field for a next
  key (its reserved bytes 18-19 and 56-63 must be zero, `check_header`), so a rotation is designed
  in step 3, not here (D64).
- Secure Boot V2 and flash encryption: inbox (D52).
- A release of the host program (the Mac) as a binary: the release is the box's firmware.
- Running `validate.sh` in the release workflow: the release commit goes through a pull request
  whose CI is green, and the tag is put on its merge commit on `main` (R6), which the workflow
  checks (R3.3).

## Requirements

### R1: `scripts/release.sh`

```
sh scripts/release.sh <version> (--key-file <path> | --key-env <NAME>)
                      [--public-key-file <path>] [--out <folder>] [--notes <file>]
                      [--min-version <x.y.z>] [--repo <owner/name>]
```

- R1.1 **Version.** `<version>` is `x.y.z` (or the tag `vx.y.z`), numbers up to 65535. It must
  equal the `version` of `Cargo.toml`'s `[workspace.package]`, of `firmware/Cargo.toml`'s
  `[package]` and of `ui/package.json`; else exit 2 naming the file and its version. Nothing is
  built.
- R1.2 **Signing key.** Exactly one of `--key-file <path>` (a file holding the 64 hex characters
  `--update-keygen` writes) or `--key-env <NAME>` (the name of an environment variable holding
  that text: the CI's secret). Never the key itself as an argument. Neither given: exit 2, "a
  release is always signed: give --key-file <path> or --key-env <NAME>". Both given: exit 2.
  `<NAME>` not matching `^[A-Za-z_][A-Za-z0-9_]*$`: exit 2. `--key-env` naming a variable that is
  unset or empty: exit 2, "the signing key is missing: <NAME> is empty". A key file that does not
  exist, cannot be read, or lies inside a git working tree (a `.git` in its folder or above, as
  `--update-keygen` refuses) or inside the output folder: exit 2.
  **Who sees the key** (the build runs many tools: `build.rs` scripts, ESP-IDF's cmake and pip,
  `npm`): with `--key-env`, the script reads the variable with `printenv <NAME>` (the environment
  only, never one of its own shell variables: a name the script uses itself, `VERSION` or `OUT`, is
  no trouble), into a shell variable that is **not exported**, and `unset`s the environment
  variable at once, before it assigns any variable of its own, so no child process inherits it. In
  the CI the key is also in no job that runs third-party build code (R3.3, R3.4). The key is written to a file only while `solite-host` needs it: under
  `umask 077`, in a key folder (`mktemp -d`) outside the output and the log, for the probe of R1.7
  (removed right after it) and again for the signing (removed right after it); the key folder is
  removed when the script ends, also on failure or interruption (`trap` on EXIT, INT, TERM). The
  pattern file of R1.8 holds the same text: it is written in the key folder only for each search
  (the staging folder, and the log, on success and on a failure) and removed right after it; while
  the build runs, the key folder holds no file. A signal (INT, TERM) makes the script send TERM to
  its whole process group before it cleans up (background jobs of a non-interactive shell ignore
  SIGINT, so the builder's children, `npm`, `cargo`, would otherwise outlive it), then exit 130 or
  143. The
  key's text is never printed, never written into the output folder or the log, never passed on a
  command line (the search of R1.8 reads it from a pattern file).
- R1.3 **Public key.** `--public-key-file <path>`, by default `release/update-key.pub` (64 hex
  characters, a trailing newline or spaces trimmed, D63). Missing or not 64 hex characters: exit 2
  naming the file and how to make it (`docs/release.md`, D64).
- R1.4 **Notes.** The section of `docs/release-notes.md` whose heading is `## <version>` exactly,
  or `## <version> ` followed by more text (a date: `## 0.2.0 (2026-12-01)`; `## 0.2.01` is not
  `0.2.0`'s), up to the next `## ` heading, trimmed; `--notes <file>` reads that file whole
  instead. No section, or an empty one: exit 2, "no release notes for <version> in
  docs/release-notes.md".
- R1.5 **Other options.** `--out <folder>`, by default `target/solite-release/<version>`: refused
  (exit 2) when it exists and is not empty (an empty one is accepted). `--min-version <x.y.z>`
  (default none: `null`) must be a version lower than `<version>`, else exit 2. `--repo
  <owner/name>`, by default `$GITHUB_REPOSITORY`, else `mchacher/solite`. A version with a suffix
  (`0.2.0-rc1`) is refused by R1.1: a release is `x.y.z`.
- R1.6 **Build** (every check of this point runs before it: R1.1 to R1.5 and the probe of R1.7).
  Two options split the run in two, for the CI (R3): `--build-only` (no key option allowed, exit 2
  otherwise, nor `--prebuilt`; every check that needs no key: version, public key file, notes,
  output folder; the builder; no probe, no signing; the output folder holds only `app.bin` and
  `flash.bin`; the line `release: built, not signed (--build-only)`) and `--prebuilt <folder>`
  (instead of the builder: `<folder>/app.bin` and `<folder>/flash.bin`, a missing file exit 2; a
  key is required; the rest of the run is as without it; exit 1, "the prebuilt image does not carry
  <public key file>'s key", when the public key's 64 hex characters are not in `app.bin`
  (`grep -aqF`)). The script's own builder also checks that the ELF carries the public key's text
  (`grep -aqF`), same message. The script's own builder, all inside it: the interface built (`cd ui && npm run build`,
  `ui/node_modules` must exist), the radio image (`--features radio`,
  `ESP_IDF_SDKCONFIG_DEFAULTS="sdkconfig.defaults;sdkconfig.radio.defaults"`) with
  `SOLITE_UPDATE_PUBLIC_KEY` set to the public key, in its own target folder
  `firmware/target/release-radio` (D63: a release's key never lands in `target/radio`, which
  `box.sh flash` takes); the ELF checked as `box.sh flash --image radio` checks it (the `radio:
  uart ` marker, the `solite-ui ` marker, `CONFIG_UART_ISR_IN_IRAM=y` in its sdkconfig);
  `fw_find_tools`, the partition table (`fw_build_table`), the bootloader (`fw_find_bootloader`),
  the application image (`fw_elf2image`, refused above the `ota_0` slot), and the full-flash image:
  `$ESPTOOL --chip esp32 merge_bin -o <flash.bin> --flash_mode keep --flash_freq keep --flash_size
  keep 0x1000 <bootloader> 0x8000 <table> <ota_0 offset> <app>` (esptool v4.x, which ESP-IDF 5.5
  ships; `merge_bin` pads from `0x0` with `0xFF`, so `nvs` and `otadata` are blank in it). The
  builder writes `<folder>/app.bin` and `<folder>/flash.bin`. A failure: exit 1 with the reason,
  nothing in the output folder. The size line of R1.9 reads `ota_0`'s size from
  `firmware/partitions.csv` with `awk` (no ESP-IDF tool), so it works with the fake builder too.
- R1.7 **Sign, then verify.** Before the build, a **probe**: 16 bytes signed as `<version>` and
  verified against the public key (`--update-sign`, then `--update-verify`); a mismatch fails in
  seconds: exit 1, "the signing key does not match <public key file>: nothing is published".
  After the build: `solite-host --update-sign <app> --version <version> --key <file> --out
  <staging>/solite-<version>.sup`, then `solite-host --update-verify <that file> --public-key
  <hex>` (R4); the same message on a mismatch, nothing in the output folder. With the script's own
  builder (the test seam of R1.10 unset: the script's own build or a prebuilt image), the line
  `signed: no application descriptor` of the signing after the build is refused
  (exit 1; the probe's bytes have none, and its line is expected): a published image always has
  its descriptor's version checked.
- R1.8 **Output.** Made in a staging folder, moved to the output folder only when every step
  passed. It holds exactly:
  - `solite-<version>.sup`: the signed update file (spec 032);
  - `solite-<version>-flash.bin`: the full-flash image for a first USB flash, written at `0x0`
    after a whole erase (R5, `docs/release.md`);
  - `manifest.json` (R2);
  - `SHA256SUMS`: one line `<sha256 hex>  <name>` for each of the three files above, sorted with
    `LC_ALL=C` (`manifest.json`, `solite-<version>-flash.bin`, `solite-<version>.sup`), the format
    `shasum -a 256 -c` and `sha256sum -c` read;
  - `notes.md`: the notes of R1.4 (the release's text on GitHub; not an asset).
  Before the move, the staging folder and the log are searched for the key's text with `grep
  -rqiF -f <pattern file>` (the pattern file holds the 64 trimmed hex characters only, in the key
  folder): found, exit 1 and nothing is kept (it cannot happen by construction; the check proves
  it).
- R1.9 **Lines and log.** One line per step on the standard output: `release: version <v>
  checked`, `release: key matches the public key <first 8 hex of the public key file>`, `release:
  radio image built (<n> KB, <p> % of the <s> KB ota_0 slot)`, `release: signed and verified
  (public key <first 8 hex of the public key file>)`, `release: <output folder>`. Every tool's full
  output goes to a log in a log folder (`mktemp -d`, apart from the key folder). On success the log
  folder is removed; on a failure it is kept, its path printed, and its last 40 lines printed
  after the search of R1.8 shows the key is not in it (the CI cannot read a file left on the
  runner); if the key is found in it, the log is removed and nothing of it is printed.
- R1.10 **Seams for the tests.** `SOLITE_RELEASE_BUILDER` replaces the builder of R1.6: it is run
  as `sh <builder> <version> <public key hex> <folder>` and writes `<folder>/app.bin` and
  `<folder>/flash.bin`; the script prints `release: builder <path> (a test seam)`, and refuses it
  (exit 2) when `GITHUB_ACTIONS` is `true`. `SOLITE_HOST_BIN` is the host program (printed the same
  way); without it the script runs `cargo build -p solite-host --release` (quick when up to date,
  so a stale binary is never used) and takes `target/release/solite-host`. `SOLITE_RELEASE_TREE` is
  the folder the version files and `docs/release-notes.md` are read from (else the repository).
  `SOLITE_RELEASE_TIME` replaces the time of `releasedAt`.

### R2: `manifest.json`

What the box reads (step 2) at `https://github.com/<repo>/releases/latest/download/manifest.json`:

```json
{
  "format": 1,
  "version": "0.2.0",
  "releasedAt": "2026-12-01T09:30:00Z",
  "notes": "<the notes of R1.4>",
  "minVersion": null,
  "image": {
    "name": "solite-0.2.0.sup",
    "size": 2183424,
    "sha256": "<64 hex of the .sup file>",
    "url": "https://github.com/mchacher/solite/releases/download/v0.2.0/solite-0.2.0.sup"
  }
}
```

`format` is the manifest's format (1); `releasedAt` is UTC, ISO 8601 to the second, `Z`;
`minVersion` is a string `x.y.z` when `--min-version` is given: a box below it goes through an
intermediate release (step 2 reads it); `size` and `sha256` are the `.sup` file's (what is
downloaded), `url` the release asset's stable address. The keys are camelCase as Sowel's JSON
(no Sowel type mirrors it: a manifest is Solite's own, D63). Written by `python3`'s `json`, two
spaces of indent, ending with a newline.

### R3: `.github/workflows/release.yml`

- R3.1 **Triggers.** `push` of a tag `v*`, and `workflow_dispatch` (a dry run). Top-level
  `permissions: contents: read`; `concurrency: { group: release-${{ github.event_name }}, cancel-in-progress: false }`:
  two runs of a kind never overlap (GitHub keeps one pending run per group, so a third started
  while one waits replaces it; a release is tagged one at a time), and a dry run (`workflow_dispatch`)
  never cancels a pending tag run (another group).
- R3.2 **Shared steps.** The toolchains of `ci.yml`'s `firmware` job (system packages, rustup from
  `rust-toolchain.toml`, Node 24, `ui` `npm ci`, the ESP32 toolchain cache and `espup`, `ldproxy`,
  the ESP-IDF download cache, `Swatinem/rust-cache` on `firmware -> target` and `. -> target`) in
  a local composite action `.github/actions/firmware-toolchain/action.yml`, used by the jobs
  `firmware` and `dry-run` below. The caches are **restored, never saved**, by a release run
  (`actions/cache/restore`, rust-cache's `save-if: false` and the same `shared-key`/cache keys as
  `ci.yml`'s jobs, whose caches on `main` are the ones found; `ci.yml`'s rust-cache steps get
  `shared-key: firmware` (firmware job) and `shared-key: rust` (rust job) so both sides share
  their keys, and the same pinned `Swatinem/rust-cache` commit, `d1527c8f…` v2.9.3, so keys are
  computed by one version). `setup-node`'s `cache: npm` is not used: it saves `~/.npm` in its post
  step on a miss; no npm cache is restored either (its key and path would not match setup-node's;
  `npm ci` downloads the packages). The action's espup version and
  Xtensa cache key are `ci.yml`'s (a test compares them). A cache written by a run that held the
  secret is never reused. The espup download's token is `${{ github.token }}` (a composite action
  cannot read `secrets.*`). Third-party actions (`Swatinem/rust-cache`) are pinned by their commit SHA in this
  workflow and the composite action; GitHub's own (`actions/*`) by their major tag, as `ci.yml`.
- R3.3 **Jobs `firmware` and `sign`** (tag push only: `if: github.event_name == 'push' &&
  startsWith(github.ref, 'refs/tags/v')`; `permissions: contents: read`), on `ubuntu-24.04`. The
  secret never shares a job with third-party build code (on Linux a build tool of the same user
  reads `/proc/<pid>/environ` of the step's shell), so the run is two jobs:
  - **`firmware`**: no environment, no secret. Checkout with `fetch-depth: 0`, the refusals that
    need no secret before the build (`release/update-key.pub` absent, the tag's commit not on
    `main`, each a `::error::`), the shared steps, then `sh
    scripts/release.sh "${GITHUB_REF_NAME#v}" --build-only --out "$RUNNER_TEMP/build"`; uploads
    `app.bin` and `flash.bin` as the artifact `firmware`.
  - **`sign`** (`needs: firmware`, `environment: release`), checkout with `fetch-depth: 0`:
    1. **Refusals, before the host is built** (the two of `firmware` repeated), each a `::error::` and exit 1: the secret empty
       ("the secret SOLITE_UPDATE_KEY is not set in the environment release: an unsigned release is
       never published (see docs/release.md)"; the secret reaches this step through `env:`, tested
       with `[ -z "$SOLITE_UPDATE_KEY" ]`, then `unset SOLITE_UPDATE_KEY` at once, since
       `secrets.*` cannot be read in `if:`); `release/update-key.pub` absent; the tag's commit not
       on `main` (`git merge-base --is-ancestor "$GITHUB_SHA" origin/main`).
    2. `rustup toolchain install` and a restore-only rust-cache (`shared-key: rust`), then
       `cargo build -p solite-host --release`: the only thing built here (no ESP32 toolchain, no
       npm).
    3. The artifact `firmware` downloaded, then `sh scripts/release.sh "${GITHUB_REF_NAME#v}"
       --key-env SOLITE_UPDATE_KEY --prebuilt <folder> --out "$RUNNER_TEMP/release"`, the secret in
       that step's `env:` (and the refusal step's), nowhere else; the script takes it out of the
       environment before assigning any variable (R1.2). It refuses an image that does not carry
       the committed public key (R1.6).
    4. The output uploaded as the artifact `release` (`actions/upload-artifact@v4`).
  The secret is an **environment secret** of the GitHub environment `release`, whose deployment
  rule allows only tags `v*` (R5): a branch's workflow or a pull request cannot read it.
- R3.4 **Job `dry-run`** (`workflow_dispatch` only, no environment, `contents: read`): the shared
  steps, the host built, a throwaway key made in `$RUNNER_TEMP` (`--update-keygen`), its public key
  written to a file there, a notes file "Dry run, nothing is published.", then, in this one job
  (the throwaway key protects nothing), `release.sh <the version of Cargo.toml> --key-file <throwaway> --public-key-file <its public key> --notes <file>`.
  The secret is never read. It uploads only `manifest.json` and `SHA256SUMS` as the artifact
  `dry-run-<sha>` (no flashable image whose key is lost).
- R3.5 **Job `publish`** (`needs: sign`, the same `if:` as `sign`), the only job with
  `contents: write`, no environment, no secret: it downloads the artifact, refuses (exit 1) when
  `solite-<version>.sup`, `solite-<version>-flash.bin`, `SHA256SUMS` or `manifest.json` is missing,
  and runs `gh release create "$GITHUB_REF_NAME" <the four files> --repo "$GITHUB_REPOSITORY"
  --title "Solite <version>" --notes-file notes.md --verify-tag`, with `GH_TOKEN` the workflow's
  `GITHUB_TOKEN`. The release is neither a draft nor a pre-release, so
  `releases/latest/download/manifest.json` points at it. A release that exists for the tag makes
  `gh release create` fail: a release is never replaced.

### R4: `solite-host --update-verify <file> --public-key <64 hex>`

Checks a signed file as the box does, without installing it: the header (`check_header` with a
running version of `0.0.0` and the slot's capacity), then the image's length and SHA-256 against
the header. Prints `verified: <file>, version <x.y.z>, <n> bytes, key <first 8 hex of the public
key>`, exit 0. A public key that is not 64 hex characters or not a valid Ed25519 key, a file that
cannot be read, a file shorter than the 128-byte header, a refused header (its message), an image
shorter or longer than the header says, or a digest that differs: exit 2 with `update-verify:
<reason>`. `--update-verify` without `--public-key` (or the reverse): usage, exit 2.

### R5: Key custody and first flash, written in `docs/release.md`

- How the maintainer makes the key (`--update-keygen` outside the repository), commits the public
  key as `release/update-key.pub` (through a pull request), keeps the private key (a password
  manager entry and an offline copy).
- The GitHub environment `release` and its secret, typed by the user: the environment with a
  deployment rule for tags `v*` (`gh api` calls written out), then `gh secret set
  SOLITE_UPDATE_KEY --env release < <file>`. A required reviewer on the environment is optional
  (the tag push is already the user's yes; one click more before each publication if wanted).
  Without the environment set up first, GitHub creates it unprotected at the first run: the
  setup comes before the first tag.
- What a leak means (a new key; boxes built with the leaked key accept the attacker's files until
  they are moved to the new key by a release signed with the old one, or by USB) and what a
  rotation means today (D64: the key is compiled into the image; a release signed with the old key
  carries the new public key, and from it on releases are signed with the new one; the format and
  the order are step 3's).
- What the `main` check guards against: a mistake, not a person with push rights (the check runs
  from the tagged commit's own workflow file, and the environment's rule looks at the tag's name
  only): a tag ruleset limiting who creates `v*` tags is recommended (the `gh api` call written
  out).
- What `SHA256SUMS` proves: the files are those of the release, not that the release is genuine
  (it is not signed; the `.sup` is, and a box checks it). The full-flash image is as trustworthy as
  the GitHub release; signing `SHA256SUMS` is in the inbox.
- **First flash from a release** (a Dongle Max never flashed with Solite; D6's yes is the user's):
  `sh scripts/box.sh identify` (its refusals: secure boot, flash encryption, revision, flash size),
  `sh scripts/box.sh backup --out <folder>` (optional, D35), then `esptool.py --chip esp32
  erase_flash` (the whole flash: `storage` too, which the image does not cover) and `esptool.py
  --chip esp32 write_flash 0x0 solite-<v>-flash.bin` (`esptool` without `.py` and `erase-flash`,
  `write-flash` with esptool v5 from `pip install esptool`); `shasum -a 256 -c --ignore-missing
  SHA256SUMS` before, with only the flash image and `SHA256SUMS` downloaded (`sha256sum -c
  --ignore-missing` on Linux). A box
  already running Solite is never flashed this way (it would lose its home and its network): it
  takes the `.sup` (the Firmware update card).

### R6: `/solite-release`

The skill: version bump in the three files, the notes at the top of `docs/release-notes.md` under
`## <version> (<date>)`, `cargo check` for `Cargo.lock`, commit `chore(release): vX.Y.Z` on a
branch `chore/release-vX.Y.Z`, through a pull request whose CI is green, merged with the user's
yes; then an annotated tag `vX.Y.Z` on that merge commit of `main`, whose message is the notes;
the user's explicit yes before the tag is pushed; then the CI's `release.yml` builds, signs and publishes; the skill watches the run (`gh run
watch`) and gives the release's address. A first-time check: `release/update-key.pub` committed and
the environment's secret set (`gh secret list --env release` shows `SOLITE_UPDATE_KEY`), else it stops and points at
`docs/release.md`.

## Living spec delta

### docs/living-spec/README.md

- Changed: "Signing a release" → "Publishing a release": the steps are the tag and the CI
  (`release.yml` runs `scripts/release.sh`); what a release holds (the `.sup`, the full-flash
  image, `SHA256SUMS`, `manifest.json` at its stable address), the manifest's fields, the
  refusals of the script (version, key, public key, notes, output folder, key mismatch), the dry
  run; `--update-keygen` and `--update-sign` stay as they are; `--update-verify` is new; "where
  the signed file is hosted and its manifest are not covered" is removed; step 3's `espflash
  save-image` becomes the script's `esptool elf2image`.

### No longer true

Search: `release`, `Signing a release`, `save-image`, `manifest`, `update-sign`, `solite-release`
in `docs/` (roadmap and decisions excepted), `README.md`, `CLAUDE.md`, `.claude/`.

- `docs/living-spec/README.md`: "Publishing the signed file (where it is hosted, a manifest) is not
  covered." → removed (it is covered).
- `docs/living-spec/README.md` step 3 "`espflash save-image --chip esp32` of the radio image makes
  the application image." → the script's `esptool elf2image` (the same as `box.sh`).
- `.claude/skills/solite-release/SKILL.md` Phase 3 step 3 "The tag is the release while the
  firmware is not distributed ... where the signed file is hosted and its manifest are not written
  yet" → the CI publishes (R6).
- `README.md`: nothing says how a user gets a release → a short "Install from a release" part.
- `docs/living-spec/README.md` update-file section: "as `espflash save-image` writes it" → `esptool
  elf2image` (`box.sh` and `release.sh` make it so).
- `docs/architecture.md`: the update CLI "`--update-keygen` and `--update-sign`" lacked
  `--update-verify`; the `scripts/lib/firmware.sh` sharing lacked `scripts/release.sh`.
- `docs/roadmap.md`: the inbox lines "Publishing a release", "Fetch updates from a release feed"
  and "Key rotation" → folded into the J5b row.
- `CLAUDE.md` and `README.md` command lists: no line for a release → `scripts/release.sh`.
- `docs/living-spec/README.md` table of specs: a row 038.

## Acceptance criteria

- [x] `sh scripts/release.sh` with a fake builder and a throwaway key writes the five files; the
      manifest has the fields of R2 with the `.sup`'s size and SHA-256; `SHA256SUMS` checks with
      `shasum -a 256 -c`.
- [x] It refuses a version that is not the tree's, no key, an empty key variable, a missing public
      key, a key that does not match the public key, missing notes, a non-empty output folder;
      nothing is left in the output folder.
- [x] The key's text is in no output file, in none of the script's output lines, in no log, and
      in no environment the builder sees.
- [x] `solite-host --update-verify` accepts a file signed by the matching key and refuses another
      key, a changed byte, a truncated file.
- [x] One real run on the Mac, real radio build, throwaway key: the `.sup` is accepted by
      `--update-verify` and installed by `POST /api/v1/system/update` of a Mac host built with the
      throwaway public key at a lower version.
- [x] `release.yml`: `contents: write` on the `publish` job only; the secret an environment
      secret of `release`, read only by the `sign` job's refusal step and its `release.sh` step;
      publishing refused without the secret or off `main`; the dry run publishes nothing.
- [x] `docs/release.md` holds the key's custody; `/solite-release` matches; D63 and D64 written.

## Edge cases

- The tag `v0.2.0` pushed on a commit whose `Cargo.toml` says `0.1.0`: R1.1 refuses, the workflow
  fails, nothing is published.
- The secret missing, or holding the wrong key (not the committed public key's): the first step
  fails, or R1.7's verification fails; no release.
- The secret holding a trailing newline or spaces (pasted): accepted (trimmed before it is written
  to the key file).
- A tag pushed on a branch's commit (not on `main`): refused before the build, by the `firmware`
  job (R3.3).
- A workflow of a branch or a pull request: it cannot read the secret (environment `release`, tags
  `v*` only).
- `release/update-key.pub` with a trailing newline: accepted (trimmed); with anything else than
  64 hex characters: refused.
- A tag pushed twice, or a release that already exists for the tag: `gh release create` fails, the
  existing release is unchanged (a release is never replaced: a new version is).
- The notes section missing for the version: refused before the build (20 minutes saved).
- The output folder already holding a previous run: refused, never mixed.
- The script interrupted (Ctrl-C) during the build: the temporary folder and the key file in it are
  removed (`trap`), the output folder is not made.
- The application image above the `ota_0` slot: refused by `fw_elf2image`.
- `--min-version` equal to or above the version: refused.

## Sowel contract

None: the manifest and the release files are Solite's own (Sowel's update check reads Docker
images; D53 already records `POST /api/v1/system/update` as a deviation). Nothing of the API
changes.

## How the user will check it

The demo, on the Mac, the box untouched:

1. `cargo test -p solite-host --test release_script` and `--test update_cli`: the refusals and the
   output, with a fake builder.
2. One real run with a throwaway key made in the scratchpad: the radio image built with its public
   key, the five files listed, `manifest.json` printed, `shasum -a 256 -c SHA256SUMS` OK,
   `--update-verify` OK. Then a Mac host at a lower version: `git archive HEAD` into the scratchpad,
   its `Cargo.toml` `[workspace.package] version` set to `0.0.9` (as `box-session.sh`'s
   `set_version`), `cargo build -p solite-host` there with `SOLITE_UPDATE_PUBLIC_KEY` the throwaway
   public key; it serves the garage, an account is set up, and the `.sup` is accepted through `POST
   /api/v1/system/update` (200 "Installed <version>, restarting"), its `slot-1.bin` equal to the
   application image.
3. The workflow read: the two jobs, the permissions, the refusal step.

In the morning, the user: makes the real key, commits `release/update-key.pub`, creates the
environment `release` (tags `v*`) and sets its secret (`docs/release.md`), runs the dry run once
(Actions › Release › Run workflow), then releases with `/solite-release`.

### Result

2026-11-25, on the Mac, the box untouched, a throwaway key in the session's scratchpad:
`sh scripts/release.sh 0.1.0 --key-file <throwaway> --public-key-file <its .pub> --notes <file>`
built the radio image in `firmware/target/release-radio` (its ELF carries the public key's hex,
the `radio: uart ` and `solite-ui ` markers) and wrote `solite-0.1.0.sup` (2 190 784 bytes),
`solite-0.1.0-flash.bin` (2 321 728 bytes: `0xFF` to `0x1000`, bootloader, table at `0x8000`,
application at `0x20000`), `manifest.json`, `SHA256SUMS` and `notes.md`; `shasum -a 256 -c
SHA256SUMS` OK for the three files; the key's text in no output file; `--update-verify` printed
`verified: solite-0.1.0.sup, version 0.1.0, 2190656 bytes, key d4e39450`. A host built from `HEAD`
with its version set to `0.0.9` and the throwaway public key, serving the garage, answered `POST
/api/v1/system/update` with 200 `{"message":"Installed 0.1.0, restarting","success":true}`; its
log `update: 0.1.0 installed, 2139 KB`, and `slot-1.bin` equal to the `.sup`'s image byte for
byte.

After the code review's split, the CI's two steps were run the same way on the Mac:
`release.sh 0.1.0 --build-only` wrote `app.bin` and `flash.bin` (no key given), then `release.sh
0.1.0 --key-env DEMO_KEY --prebuilt <that folder>` printed `key matches the public key d4e39450`,
`radio image built (2139 KB, 52 % of the 4096 KB ota_0 slot)`, `signed and verified (public key
d4e39450)`; `SHA256SUMS` OK, the key in no output file, the full-flash image the one built.
`sh scripts/validate.sh`: `validate: OK` (415 s, 1475 tests, QEMU included: peak 311 KB, `update
check: valid 200, image changed 400, signature changed 400`).

## Decisions taken in this spec

- The public key is a committed file, `release/update-key.pub`, not a repository variable: it is
  not a secret, a file makes the build reproducible from the tag, and the box's docs and step 2
  can name it (D63).
- The notes' source is `docs/release-notes.md` (the release skill already writes it), not the
  tag's message: a file is read the same way on the Mac and in the CI, and is reviewed in the
  release commit; the annotated tag's message carries the same text.
- One full-flash image (`merge_bin`, written at `0x0`) rather than three files with offsets: one
  command for the user, and the offsets cannot be mistyped.
- The release is built in `firmware/target/release-radio`, apart from `target/radio`.
- `--update-verify` is added to the host so the script checks its output with the core's own
  checks, the box's.
- The secret is an environment secret (`release`, tags `v*` only) and the tag must be on `main`
  (spec review round 1): a branch cannot read it nor publish. No required reviewer by default: the
  tag push is the user's yes (they can add one).
- The release commit goes through a pull request; the tag is put on its merge commit once `main`
  is green (R6).
- `SHA256SUMS` is not signed (the `.sup` is): signing it goes to the inbox.
- Decision numbers D61 and D62 are taken by spec 037 (its pull request is pending): this spec
  writes D63 and D64.

## Spec review leftovers

Code review round 1 (applied): the secret moved out of the job that builds (jobs `firmware`
without secret, `sign` with it, `--build-only` and `--prebuilt` in `release.sh`, the public key
checked in the image); no `cache: npm` (setup-node saves it), npm's cache restored under
setup-node's key; the script kills its process group on a signal; the key read with `printenv`
before any variable is assigned; the log searched for the key on success; `unset` right after the
refusal test; concurrency group per event; one `rust-cache` pin and `shared-key`s with `ci.yml`;
`--ignore-missing` in the checksum command; the heading rule of the notes spelled out. The
`signed: no application descriptor` refusal is tested with a wrapper host program (the seam of
`SOLITE_HOST_BIN`), not with the real firmware build.
Code review round 2 (no blocking): the signal handler sends TERM to the script's own descendants
only (`kill -TERM 0` killed the program that launched it), a test sends TERM to the script alone
and checks its parent survives; the `firmware` job refuses a tag off `main` or a missing public key
before its build; the npm cache restore is dropped (it could not match); D63's wording on what runs
next to the secret corrected.
Round 2: one blocking item (the pattern file's lifetime) and five minor ones, all applied.
Round 1 (7 blocking, all applied; 14 minor, applied but these): the third-party action pin by
SHA is limited to `Swatinem/rust-cache` (GitHub's own actions keep their major tag, as `ci.yml`).
