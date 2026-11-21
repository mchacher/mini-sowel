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
   checked against D27's 350 KB (D20's 300 KB before the review).

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
   `equipments.json`, `recipes.json`), a file written chunk by chunk as today (D15). The wall
   clock, the monotonic clock and the random source are closures given to the engine; what the
   loop shows (the measures' lines with `--verbose`, a write failure) goes to a sink closure, never
   to `println!` in the core.
3. What stays in the host: the radio's identification and its printed lines, the network backup
   check (`backup::matches`), the `network:` and `serving:` lines, HTTP, WebSocket and the command
   line. The host implements the trait with its files as today (permissions, folder). Every
   existing test passes unchanged, except imports, and the host's output lines are unchanged.
4. The loop runs until a stop flag is set; the caller sets it (Ctrl-C on the Mac, the end of the
   scenario in the firmware, R3.4).

### R2: The simulated radio on the ESP32

1. `crates/sim` builds for the ESP32: its Unix-only parts (file mode 0600) are behind
   `cfg(unix)`; elsewhere the file is created with the default mode (FAT has no permissions). Its
   memory file keeps working through `std::fs` on the data partition.
2. The simulated transport tells when its scenario has played its last event
   (`finished()`), so a caller can stop the loop.
3. Decision D26 replaces spec 007's "never built for the ESP32" (R1 of that spec): the simulator
   is built into the firmware for QEMU only (R3.4).

### R3: The firmware

1. At boot, after the banner and the memory line, the firmware mounts the `storage` partition
   (FAT, wear levelling) at `/data`. A partition that cannot be mounted is formatted, with a log
   line `storage: formatted (<reason>)`, and the engine starts empty; mounted, it logs
   `storage: /data, <used> KB used of <size> KB`. In QEMU the partition is blank at every run
   (the image holds bootloader, table and application only), so every QEMU run formats it.
2. It reads `/data/network.json` with `core::network::parse` and logs the choice with no cable
   detected: `network: wifi (setting auto), no link started`, or the fallback reason of spec 015
   R5.1.
3. Without the `sim` feature (the image for the box), it starts no radio and logs
   `radio: none`; the engine is not started.
4. With the cargo feature `sim` (the image QEMU runs), the firmware seeds `/data` from the
   realistic home embedded in the image when `/data` holds no `devices.json`, then runs the engine
   with the simulated radio playing the embedded scenario, as `--sim <scenario> --serve` does on
   the Mac but without HTTP. The engine runs in its own thread with an explicit stack of 32 KB
   (`std::thread::Builder::stack_size`); the main task's stack setting of spec 003 is unchanged.
   When the transport reports its scenario finished, the firmware stops the loop, lets the saver
   write, and logs the engine's memory.
5. The engine's memory line:
   `engine: <devices> devices, <zones> zones, <equipments> equipments, <recipes> recipes; heap
   <used> KB (peak <peak> KB); internal <free> KB free (min <min> KB); psram <free> KB free
   (min <min> KB)`. *used* is the heap free (internal and PSRAM together) just before the engine
   thread starts, minus the heap free after the scenario; *peak* is the heap free before the
   engine minus the lowest free seen while it runs, read with ESP-IDF's local-minimum monitor
   (`heap_caps_monitor_local_minimum_free_size_start`/`_stop`) when the pinned ESP-IDF (v5.5.5) has
   it, otherwise by sampling `heap_caps_get_free_size` after every turn of the loop. The storage
   mount happens before, so FAT and wear-levelling buffers are not counted. The simulator's own
   memory is counted: the figure is an upper bound of the engine's, labelled "QEMU, with
   simulator".

### R4: The realistic home and its check

1. The realistic home is D20's: 50 devices (6 data, 1 order each), 10 zones, 50 equipments of 6
   bindings, 3 `motion-light` instances. Its files (`devices.json`, `zones.json`,
   `equipments.json`, `recipes.json`) and a scenario that makes its devices report (at least one
   measure per device, motion and orders for the recipes, 120 s of scenario time at most, played
   at speed 10) live in `firmware/fixtures/`, synthetic.
2. A core test builds the same home as `memory_ceiling.rs` and checks the fixture files are its
   output; with `UPDATE_FIXTURES=1` it writes them. The fixtures cannot drift from the test.
3. A host test plays the same scenario with the same home through `--sim` and checks the run ends
   with every device reported and a light order sent (the motion sensors' alarms make `motion-light`
   switch the lights): one scenario serves the Mac and the box.
4. `scripts/qemu.sh` boots the `sim` image, waits for the `engine:` line within 240 s (formatting
   the blank partition, then 12 s of scenario on a slow emulator; the scenario is held until the
   radio is identified and listening and the loop has turned once, then plays at its own pace),
   and fails, with the last lines of the log, when the peak is above 350 KB (D27, after D20's
   300 KB), when a device of the home got no measure into the engine (the `engine:` line reads
   `50 devices (50 reported)`), when the radio logs `callback queue full` after `scenario: started`, when the internal minimum free is below 64 KB
   (spec 015's internal reserve, kept for the network stack that QEMU does not start), on a panic
   or reboot, or without the line. It also checks the `storage:` and `network:` lines. It prints
   the `engine:` line.
5. `sh scripts/validate.sh` builds both images (with and without `sim`) and runs the QEMU check.

## Living spec delta

### docs/living-spec/README.md

- Changed, "Starts and shows its name", ESP32: after the memory line, the firmware mounts its data
  partition (formats it when it cannot), reads its network setting and logs its choice, starts no
  link, and, in the image for the box, logs `radio: none`.
- Changed, "On the Mac, in an emulated ESP32": `sh scripts/qemu.sh` runs the engine in the
  emulated ESP32 with the simulated radio and a realistic home, prints the `engine:` line, and
  fails above 350 KB of heap, when a device did not report, when a callback was dropped, or below
  64 KB of internal RAM free.
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
- `README.md` and the comments of `scripts/validate.sh` and `scripts/qemu.sh`, where they say
  what the QEMU run shows → the `engine:` line.
- `CLAUDE.md` commands: `sh scripts/qemu.sh` comment, if it says "the firmware, booted" only.
- `docs/decisions.md` D20 ("read again on the box") and D25: history, not edited; D26 added.
- Step, not living spec: `docs/roadmap.md` E1 set to built when specs 015 and 016 are.
- Searched: "Mac only", "never built", "radio_loop", "Saver", "FileStore", "qemu", "engine".

## Acceptance criteria

- [ ] `cargo test --workspace` passes; the host's lines and files are unchanged.
- [ ] `sh scripts/qemu.sh` shows the `engine:` line for the realistic home, under 350 KB (D27), with every device reported and no callback dropped.
- [ ] The box's image (without `sim`) builds and boots in QEMU, logs storage, network and
      `radio: none`.
- [ ] `docs/memory-budget.md` holds the QEMU figures.
- [ ] `sh scripts/validate.sh` is green.

## Edge cases

- The data partition is blank (first boot) or corrupt: formatted, logged, the engine starts empty
  (the `sim` image seeds it).
- The data partition is full: a write fails, is logged and retried, as on the Mac (`Saver`).
- No `network.json`: `auto`, reason missing.
- The engine's peak above 350 KB, or internal RAM exhausted: `scripts/qemu.sh` fails with the
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
- Internal RAM minimum of 64 KB as the QEMU floor: spec 015's reserve, which the network stack
  needs on the box and QEMU does not use.
- The spec review's questions, answered with its recommendations: the QEMU figure with the
  simulator inside is an acceptable gate for D20 (D27 after the review), labelled as such and measured again on the box
  in E3; one fixture scenario serves the firmware and a host test.
- After the review: the scenario's clock starts after the first loop turn; the simulator's clock counts only the time it waits (the emulator's slow flash would otherwise play the scenario in one burst); the saves of the run are walked and dropped, the last one goes to the flash; the radio's own memory stays in RAM; motion sensors 10, 20 and 30 of the realistic home report an alarm at 60 s; a save cut by a power loss is recovered at the next read (`<name>.tmp`); the QEMU ceiling is 350 KB (D27).
- No second-boot check: every QEMU run starts from a blank partition; the reuse of `/data` is the
  Mac's tested behaviour and E3's run on the box.

## How the user will check it

`sh scripts/qemu.sh`: the banner, the memory line, `storage:`, `network:`, then the `engine:` line
of the realistic home, under 350 KB; `cargo run -p solite-host -- --sim scenarios/garage.json
--serve --speed 10` and `sh scripts/demo-garage.sh` behave as before.
