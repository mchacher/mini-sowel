# Solite

A minimal [Sowel](https://github.com/mchacher/sowel) in an off-the-shelf Zigbee coordinator, the
Sonoff Dongle Max (about 43 $). It automates an annex of a house (garage, pool house, workshop) or
a small home on its own, without a server, Docker or the cloud, and becomes an extension of Sowel
when there is one. It keeps Sowel's concepts (devices, equipments, zones, recipes, modes) and the
format of Sowel's API.

What the tool does today: [docs/living-spec/](docs/living-spec/README.md). Where it is going:
[docs/roadmap.md](docs/roadmap.md).

## What is in the repository

| Folder | What |
| ------ | ---- |
| `crates/core` | The Sowel core, in Rust, without any hardware dependency |
| `crates/host` | Solite on a Mac (or a Linux or Windows computer) |
| `crates/sim` | A simulated radio and devices, played from `scenarios/`, for working without the dongle |
| `firmware/` | Solite on the ESP32 of the Dongle Max (Rust on ESP-IDF) |
| `ui/` | The web interface (Preact, TypeScript), in Sowel's design system |
| `tools/` | Node tools run on the Mac, not shipped in the box: `z2m-coverage` measures how many of Zigbee2MQTT's device definitions the box can reuse |
| `docs/`, `specs/` | What the tool does, how it is built, every change |

## Install

1. **Rust**: install [rustup](https://rustup.rs). The repository pins its Rust version; `rustup`
   installs it on the first `cargo` command.
2. **Node.js** 24 or newer, then the packages of the interface and of the tools:
   `cd ui && npm ci`, and `cd tools/z2m-coverage && npm ci`.
3. **The ESP32 toolchain**, to build the firmware:
   - macOS: `brew install cmake ninja`; Linux: install `cmake`, `ninja-build`, `python3`,
     `python3-venv` and `libudev-dev` with the system's package manager; Windows: follow the
     [espup prerequisites](https://github.com/esp-rs/espup#requirements);
   - then `cargo install espup ldproxy --locked` and `espup install --targets esp32`;
   - QEMU's system libraries, to boot the firmware in an emulated ESP32: macOS:
     `brew install libgcrypt glib pixman sdl2 libslirp`; Debian or Ubuntu: `libgcrypt20
     libglib2.0-0 libpixman-1-0 libsdl2-2.0-0 libslirp0`. QEMU itself is downloaded into
     `firmware/.embuild/` the first time `sh scripts/qemu.sh` (or the validate command) runs;
   - before building the firmware, in each new terminal: `. "$HOME/export-esp.sh"` (macOS, Linux)
     or `. "$HOME\export-esp.ps1"` (Windows PowerShell).

   The first firmware build downloads ESP-IDF (a few GB) into `firmware/.embuild/` and takes
   several minutes (about 7 on an Apple-silicon Mac). The builds after it take seconds.
4. The git hooks: `git config core.hooksPath .githooks`.

## Run

```
cargo run -p solite-host                      # on the computer
cargo run -p solite-host -- --radio tcp://<address>:6638   # identify the Dongle Max's radio
cargo run -p solite-host -- --radio tcp://<address>:6638 --pair   # start the network, pair
cargo run -p solite-host -- --radio tcp://<address>:6638 --listen # show the measures
cargo run -p solite-host -- --sim scenarios/garage.json --pair 30 --speed 10  # without the dongle
cargo run -p solite-host -- --sim scenarios/garage.json --serve --speed 10 --recipe-script recipes/scripts/motion-light.js    # API and interface on :8080, with the motion-light recipe
SOLITE_PASSWORD=<password> sh scripts/demo-garage.sh [port | address[:port]]    # the demo garage, through the API
SOLITE_PASSWORD=<password> sh scripts/measure-api.sh <address>[:port] [--keep]  # the 17 requests of an equipment with 8 + 8 bindings (Mac sim; box: once per milestone)
cd ui && npm run dev                              # the interface in development, :5173 (host on :8080)
cd firmware && cargo build --release              # the ESP32 firmware (box image; --features sim: the QEMU image)
cd firmware && CARGO_TARGET_DIR=target/radio ESP_IDF_SDKCONFIG_DEFAULTS="sdkconfig.defaults;sdkconfig.radio.defaults" cargo build --release --features radio   # the radio image (build the interface first: cd ui && npm run build)
sh scripts/e2e-sim.sh                             # the simulated path driven in headless Chromium (cd tools/e2e && npm ci first)
sh scripts/qemu.sh                                # the engine run in an emulated ESP32: its `engine:` line
sh scripts/box.sh identify                        # the Dongle Max over USB (also: backup, restore, flash [--keep-storage], log [--budget], wifi, pair, saves; --image box|sim|radio)
sh scripts/box-session.sh --yes-flash --backup <file>   # the whole box session in one pass (typed by the user: it flashes, D55); --api-only <port> on the Mac
cd tools/z2m-coverage && npm run report           # the Zigbee2MQTT coverage report
sh scripts/release.sh <x.y.z> --key-file <file>   # a signed release, locally (the CI's release.yml does it on a tag; docs/release.md)
sh scripts/validate.sh                            # every check, the slow tests and QEMU included: must be green before a commit
```

The firmware is flashed only through `sh scripts/box.sh flash`, run by the user. A backup of the original
firmware is optional (`--backup <file>`, checked when given): the way back is Sonoff's own reflash (see
[docs/decisions.md](docs/decisions.md), D6, D31 and D35, and [docs/hardware.md](docs/hardware.md)).
Without a flash, the Dongle Max keeps its original firmware.

## Install from a release

A release (the GitHub Releases page) holds `solite-<version>-flash.bin` (the whole image),
`solite-<version>.sup` (the signed update file), `SHA256SUMS` and `manifest.json`.

- **A Dongle Max never flashed with Solite**: download `solite-<version>-flash.bin` and `SHA256SUMS`
  into one folder and check the image (`shasum -a 256 -c --ignore-missing SHA256SUMS`, on Linux
  `sha256sum -c --ignore-missing SHA256SUMS`: the line of the file you did not download is
  skipped), then
  `sh scripts/box.sh identify`, `esptool.py --chip esp32 erase_flash` and
  `esptool.py --chip esp32 write_flash 0x0 solite-<version>-flash.bin`. This replaces the box's
  original firmware: optionally back it up first (`sh scripts/box.sh backup --out <folder>`).
  Details: [docs/release.md](docs/release.md).
- **A box already running Solite**: never flashed this way (it would lose its home); upload the
  `.sup` in Settings, the Firmware update card, or let the box pull the latest release itself:
  the same card's "Check for updates" then "Update to vX" (it reads the release's `manifest.json`),
  or through the API `POST /api/v1/system/version/check` then `POST /api/v1/system/update` with an
  empty body. A download cut by a network blip resumes from the byte written. The box checks the signature either way; the manifest's address is the setting `update.manifestUrl`
  and `update.autoCheck` turns on a daily check.

## How the project is worked on

With Claude Code, spec first: `/solite-roadmap`, `/solite-feature`, `/solite-fix`,
`/solite-release`. Rules: [CLAUDE.md](CLAUDE.md).

## License

[GNU Affero General Public License v3.0 only](LICENSE) (`AGPL-3.0-only`), as Sowel.
