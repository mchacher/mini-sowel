# Spec 001: walking-skeleton

## Context

The smallest version of mini-Sowel that builds, runs and shows its name, on each of the three
places the tool runs: the Mac, the ESP32 of the Dongle Max, the browser. It proves the environment
and the whole chain (install, build, run, test, checks, commit) before any real feature. Source:
the framing note, `inputs/note-de-cadrage-mini-sowel.md` (Architecture, Development and tests).

## Goals

1. The Rust core, the host program and the ESP32 firmware build from one repository, with the same
   core.
2. The interface builds with Sowel's design system and stays under its budget.
3. `sh scripts/validate.sh` runs every check and is green.

## Out of scope

- Any business feature: devices, equipments, zones, modes, recipes, radio, API, storage. They are
  in the roadmap inbox.
- Flashing the Dongle Max (decision D6): the firmware is built, not flashed.
- Serving the interface from the host or the box.

## Requirements

### R1: The core names the tool

`crates/core` exposes the name `mini-sowel`, the version from its manifest, and the banner
`mini-sowel <version>`.

### R2: The host program starts

`cargo run -p mini-sowel-host` prints the banner on one line and exits with success.

### R3: The firmware builds for the ESP32

`cargo build --release` in `firmware/` produces an ESP32 binary that, when started, initialises
ESP-IDF and writes the banner to the serial log at level info.

### R4: The interface opens on Sowel's layout

The interface shows Sowel's sidebar (logo and MINI-SOWEL) and a page whose title is `mini-sowel`,
in Sowel's Hybrid theme, with the system fonts. Its first load is under 170 KB gzip.

### R5: The checks

`sh scripts/validate.sh` runs: the docs check; Rust format and lint (core, host, firmware); Rust
tests; UI format, lint (with Sowel's design rules and no raw colour), types, tests, build and
budget; the firmware build. The pre-commit hook runs the quick checks; the commit-msg hook and
Claude's commit hook are in place.

## Living spec delta

### docs/living-spec/README.md

- New: Purpose and Users, from the true north.
- New: "Starts and shows its name": the three programs and what each one shows.
- New: History row 001.

### No longer true

None found: first spec, the docs are written with it.

## Acceptance criteria

- [x] `cargo run -p mini-sowel-host` prints `mini-sowel 0.1.0`
- [x] The firmware builds for `xtensa-esp32-espidf` with the banner in its code
- [x] The interface shows the sidebar with the Sowel logo and MINI-SOWEL, and the title `mini-sowel`
- [x] The first load of the interface is under 170 KB gzip
- [x] `sh scripts/validate.sh` is green

## Edge cases

- ESP32 toolchain missing: the validate command stops with a message saying how to install it.
- UI dependencies missing: the validate command stops with `cd ui && npm ci`.

## How the user will check it

The demo: the host program run in the terminal, the firmware build ending with its binary, the
interface opened in the browser, and the validate command green.

Run on 2026-11-20 (setup): host program prints `mini-sowel 0.1.0`; firmware built for
`xtensa-esp32-espidf` (ESP-IDF v5.5.5, first build 6 min 38 s), the banner string is in the
binary, and `espflash save-image --chip esp32` makes a 380 KB application image (9.2 % of its
partition). The firmware has not run on an ESP32: the Dongle Max is not flashed (decision D6);
booting it under QEMU is in the roadmap inbox. Interface first load: 10.2 KB gzip.
