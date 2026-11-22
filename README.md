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
cargo run -p solite-host -- --sim scenarios/garage.json --serve --speed 10    # API and interface on :8080
sh scripts/demo-garage.sh [port]                     # the demo garage, through the API
cd ui && npm run dev                              # the interface in development, :5173 (host on :8080)
cd firmware && cargo build --release              # the ESP32 firmware (box image; --features sim: the QEMU image)
sh scripts/e2e-sim.sh                             # the simulated path driven in headless Chromium (cd tools/e2e && npm ci first)
sh scripts/qemu.sh                                # the engine run in an emulated ESP32: its `engine:` line
sh scripts/box.sh identify                        # the Dongle Max over USB (also: backup, restore, flash, log)
cd tools/z2m-coverage && npm run report           # the Zigbee2MQTT coverage report
sh scripts/validate.sh                            # every check: must be green before a commit
```

The firmware is flashed only through `sh scripts/box.sh flash`, with the user's yes for each write: the
command asks, on the terminal, for the end of the box's MAC address, and refuses without a backup of the
original firmware that was restored once (see [docs/decisions.md](docs/decisions.md), D6 and D31, and
[docs/hardware.md](docs/hardware.md)). Without a flash, the Dongle Max keeps its original firmware.

## How the project is worked on

With Claude Code, spec first: `/solite-roadmap`, `/solite-feature`, `/solite-fix`,
`/solite-release`. Rules: [CLAUDE.md](CLAUDE.md).

## License

[GNU Affero General Public License v3.0 only](LICENSE) (`AGPL-3.0-only`), as Sowel.
