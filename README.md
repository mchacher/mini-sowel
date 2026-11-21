# mini-Sowel

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
| `crates/host` | mini-Sowel on a Mac (or a Linux or Windows computer) |
| `firmware/` | mini-Sowel on the ESP32 of the Dongle Max (Rust on ESP-IDF) |
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
   - then `cargo install espup ldproxy espflash --locked` and `espup install --targets esp32`;
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
cargo run -p mini-sowel-host                      # on the computer
cargo run -p mini-sowel-host -- --radio tcp://<address>:6638   # identify the Dongle Max's radio
cargo run -p mini-sowel-host -- --radio tcp://<address>:6638 --pair   # start the network, pair
cargo run -p mini-sowel-host -- --radio tcp://<address>:6638 --listen # show the measures
cd ui && npm run dev                              # the interface, on http://localhost:5173
cd firmware && cargo build --release              # the ESP32 firmware
sh scripts/qemu.sh                                # the firmware, booted in an emulated ESP32
cd tools/z2m-coverage && npm run report           # the Zigbee2MQTT coverage report
sh scripts/validate.sh                            # every check: must be green before a commit
```

The firmware is built, not flashed: the Dongle Max keeps its original firmware during development
(see [docs/decisions.md](docs/decisions.md), D6).

## How the project is worked on

With Claude Code, spec first: `/mini-sowel-roadmap`, `/mini-sowel-feature`, `/mini-sowel-fix`,
`/mini-sowel-release`. Rules: [CLAUDE.md](CLAUDE.md).

## License

[GNU Affero General Public License v3.0 only](LICENSE) (`AGPL-3.0-only`), as Sowel.
