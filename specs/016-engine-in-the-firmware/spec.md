# Spec 016: The engine in the firmware

## Context

Roadmap increment E1, second half (the first is spec 015). The engine runs on the Mac only: its
loop, its storage and its clock live in `crates/host`, and the firmware shows a banner and its
memory. Before the box is flashed (E3), the whole engine runs in the ESP32 firmware under
Espressif's QEMU, with the simulated radio and a realistic home, and its memory is measured on the
32-bit target: D20 says the 300 KB figure is "read again on the box (QEMU, then the Dongle Max)",
and spec 015's budget flags the internal RAM as the tight point. Approved by the user on
2026-11-21 (the engine extracted, data in a real flash partition, the scenario in the image),
built in an autonomous run.

## Goals

1. One engine loop, shared by the Mac and the ESP32; the Mac's behaviour unchanged.
2. The firmware mounts its data partition, reads its network setting and runs the engine.
3. In QEMU, the realistic home runs with the simulated radio, and its memory is measured and
   checked against D20's 300 KB.

## Out of scope

- The real radio over the serial link, Wi-Fi, Ethernet, the access point, HTTP, WebSocket and the
  interface served by the box, TLS: E3.
- Any write to the Dongle Max.

## Requirements

### R1: One engine loop

1. The loop of `crates/host/src/serve.rs` (`radio_loop`: poll the radio, apply activity, routes
   and measures, recompute, tick, run the recipes, run the orders, save what is due) and the
   saving of `store.rs` (`Saver`, its write policy and retries) move to the core, module
   `engine`, generic over the radio transport and over a new adapter trait for storage.
2. The storage trait reads and writes the engine's named files (`devices.json`, `zones.json`,
   `equipments.json`, `recipes.json`), a file written chunk by chunk as today (D15). The clock
   (wall and monotonic milliseconds) and the random source reach the engine through the core's
   existing closures, or a small trait when a closure does not fit.
3. The host implements the trait with its files as today (permissions, folder), and keeps its
   HTTP, WebSocket and command-line code. Every existing test passes unchanged, except imports;
   the host's output lines are unchanged.

### R2: The simulated radio on the ESP32

1. `crates/sim` builds for the ESP32: its Unix-only parts (file permissions) are behind
   `cfg(unix)`; its memory file keeps working through `std::fs` on the data partition.
2. Decision D26 replaces spec 007's "never built for the ESP32" (R1 of that spec): the simulator
   is built into the firmware for QEMU only (R3.4).

### R3: The firmware

1. At boot, after the banner and the memory line, the firmware mounts the `storage` partition
   (FAT, wear levelling) at `/data`. A partition that cannot be mounted is formatted, with a log
   line `storage: formatted (<reason>)`, and the engine starts empty; mounted, it logs
   `storage: /data, <used> KB used of <size> KB`.
2. It reads `/data/network.json` with `core::network::parse` and logs the choice with no cable
   detected: `network: wifi (setting auto), not started`, or the fallback reason of spec 015 R5.1.
3. Without the `sim` feature (the image for the box), it starts no radio and logs
   `radio: none`; the engine is not started.
4. With the cargo feature `sim` (the image QEMU runs), the firmware seeds `/data` from the
   realistic home embedded in the image when `/data` holds no `devices.json`, then runs the engine
   with the simulated radio playing the embedded scenario, as `--sim <scenario> --serve` does on
   the Mac but without HTTP. At the end of the scenario it logs the engine's memory and stops the
   loop.
5. The engine's memory line:
   `engine: <devices> devices, <zones> zones, <equipments> equipments, <recipes> recipes; heap
   <used> KB (peak <peak> KB); internal <free> KB free (min <min> KB); psram <free> KB free
   (min <min> KB)`, where *used* is the heap taken since just before the engine started and
   *peak* the largest of it (from the heap's minimum free figures, internal and PSRAM together).

### R4: The realistic home and its check

1. The realistic home is D20's: 50 devices (6 data, 1 order each), 10 zones, 50 equipments of 6
   bindings, 3 `motion-light` instances. Its files (`devices.json`, `zones.json`,
   `equipments.json`, `recipes.json`) and a scenario that makes its devices report (at least one
   measure per device, motion and orders for the recipes) live in `firmware/fixtures/`, synthetic.
2. A core test builds the same home as `memory_ceiling.rs` and checks the fixture files are its
   output; with `UPDATE_FIXTURES=1` it writes them. The fixtures cannot drift from the test.
3. `scripts/qemu.sh` boots the `sim` image, waits for the `engine:` line (timeout raised as
   needed, printed), and fails, with the last lines of the log, when the peak is above 300 KB
   (D20), when the internal minimum free is below 16 KB, on a panic or reboot, or without the
   line. It prints the `engine:` line.
4. `sh scripts/validate.sh` builds both images (with and without `sim`) and runs the QEMU check.

## Living spec delta

### docs/living-spec/README.md

- Changed, "Starts and shows its name", ESP32: after the memory line, the firmware mounts its data
  partition (formats it when it cannot), reads its network setting and logs its choice (no link
  started), and, in the image for the box, logs `radio: none`.
- Changed, "On the Mac, in an emulated ESP32": `sh scripts/qemu.sh` runs the engine in the
  emulated ESP32 with the simulated radio and a realistic home, prints the `engine:` line, and
  fails above 300 KB of heap or below 16 KB of internal RAM free.
- Changed, "Runs without the dongle": the simulated radio runs on the Mac and in the emulated
  ESP32.
- History: row 016.

### No longer true

- `docs/architecture.md`: the simulator "Mac only" → Mac and the QEMU image; the engine's loop
  in the host → in the core; the firmware block gains storage and the engine.
- `crates/sim/Cargo.toml`: "never built for the ESP32" → built for the QEMU image.
- `specs/007-simulated-radio/spec.md` R1: left as history; D26 records the change.
- `docs/memory-budget.md`: the ESP32 rows marked "estimated" that the QEMU line measures → the
  measured figure, with "QEMU".
- Searched: "Mac only", "never built", "radio_loop", "Saver", "FileStore", "qemu", "engine".

## Acceptance criteria

- [ ] `cargo test --workspace` passes; the host's lines and files are unchanged.
- [ ] `sh scripts/qemu.sh` shows the `engine:` line for the realistic home, under 300 KB.
- [ ] The box's image (without `sim`) builds and boots in QEMU, logs storage, network and
      `radio: none`.
- [ ] `docs/memory-budget.md` holds the QEMU figures.
- [ ] `sh scripts/validate.sh` is green.

## Edge cases

- The data partition is blank (first boot) or corrupt: formatted, logged, the engine starts empty
  (the `sim` image seeds it).
- The data partition is full: a write fails, is logged and retried, as on the Mac (`Saver`).
- No `network.json`: `auto`, reason missing.
- The engine's peak above 300 KB, or internal RAM exhausted: `scripts/qemu.sh` fails with the
  figures; the fix is a setting of spec 015 R4 (`SPIRAM_MALLOC_ALWAYSINTERNAL`), recorded, or an
  engine optimisation, never a raised ceiling without a decision.
- A fixture edited by hand: the fixture test fails.

## Sowel contract

None new: the files are the ones the host already writes in Sowel's format.

## Decided in autonomy

- The engine moves to the core (module `engine`), not to a new crate: the core already holds every
  type it uses, and has no OS dependency once storage is a trait.
- One firmware crate with a `sim` feature, rather than two firmwares.
- No HTTP in the firmware yet: the API's memory is in the Mac test; the box's server is E3.
- Internal RAM minimum of 16 KB as the QEMU floor: below it ESP-IDF's tasks and Wi-Fi fail to
  allocate; spec 015's budget keeps 64 KB in reserve on top.

## How the user will check it

`sh scripts/qemu.sh`: the banner, the memory line, `storage:`, `network:`, then the `engine:` line
of the realistic home, under 300 KB; `cargo run -p solite-host -- --sim scenarios/garage.json
--serve --speed 10` and `sh scripts/demo-garage.sh` behave as before.
