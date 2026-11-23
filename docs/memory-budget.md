# Memory budget

How the Dongle Max's ESP32 memory is shared out: 2048 KB of PSRAM (in the package) and the
internal RAM (about 359 KB free at boot in QEMU). One row per item, with its bound, its worst
case, where it lives, and whether the figure is **measured** or **estimated** (and from which
source). The box is in one of two states, never both: the realistic home (350 KB in QEMU, decision D27; 300 KB on the Mac, decision D20)
or the worst case (1 MB, decision D23); the figures of the two are shown side by side, not added.

Marks: **M** measured, **E** estimated. A figure measured on the Mac has 64-bit pointers: it is an
**upper estimate** of the ESP32's 32-bit one. A figure from QEMU is the emulated box's: it has no
Wi-Fi or Ethernet driver loaded, so its internal RAM is an upper estimate too.

## What is measured

| Figure | Value | Where it comes from |
| ------ | ----- | ------------------- |
| Engine, worst case (every bound at its maximum) | peak 925 KB, 987 KB with every recipe log entry at 200 bytes | M, Mac: `cargo test -p solite-core --test memory_worst_case -- --nocapture` |
| Engine, realistic home | peak 182 KB, 194 KB with every log entry at 200 bytes | M, Mac: `cargo test -p solite-core --test memory_ceiling -- --nocapture` |
| Share of small allocations in the worst case at its peak | 673 KB in allocations of 4 KB or less, 267 KB above, 940 KB in all: the live bytes of the whole test process at the moment of its peak, which is the 925 KB measured plus the 14 KB the process holds before the test starts | M, Mac, a counting allocator split by size at the peak, run once and not kept |
| Free memory at boot, in QEMU | `memory: internal 359 KB free (largest block 160 KB), psram 2045 KB free of 2048 KB` | M, `sh scripts/qemu.sh` |
| Engine, realistic home, on the 32-bit target (QEMU, with the simulator inside) | `engine: 50 devices (50 reported), 10 zones, 50 equipments, 3 recipes; heap 170 KB (peak 312 KB); internal 221 KB free (min 107 KB); psram 1985 KB free (min 1916 KB)`: 312 KB at the highest, 170 KB held at the end, the internal RAM never below 107 KB free | M, `sh scripts/qemu.sh` (spec 016, decision D26) |

The worst case counts, in its peak, the HTTP bodies (4 workers of the host, 16 KB each,
`api::MAX_BODY_BYTES`), the live clients' queues (`live::MAX_CLIENTS` of 4, `MAX_QUEUED_BYTES`
of 8 KB each, one batch in flight each), the orders queues, and the radio's queues by arithmetic
(48 KB, **E**: the radio is not driven by that test). The test's header lists every `pub const
MAX_*` of the core and how it is reached; a test fails when one is missing.

## PSRAM, 2048 KB

| Item | Bound | Worst case | Where | M / E |
| ---- | ----- | ---------- | ----- | ----- |
| The engine: devices, zones, equipments, recipes, events, live queues, orders, largest answer, save path | the core's `MAX_*` (`devices::MAX_DEVICES` 64, `zones::MAX_ZONES` 32, `equipments::MAX_EQUIPMENTS` 64, `recipes::MAX_INSTANCES` 16, ...) | 1024 KB (ceiling; measured 987 KB). Realistic home: 300 KB (ceiling; measured 194 KB), shown beside, not added | Large allocations in PSRAM; small ones first in internal RAM, see below | M |
| Transport buffers outside the core: the HTTP server's and the WebSocket's read and write buffers | 4 HTTP workers, `live::MAX_CLIENTS` 4 clients, about 8 KB each | 64 KB | PSRAM | E, from the ESP-IDF HTTP server's default buffer sizes; the box holds no `tungstenite` buffers (the Mac host's WebSocket library): its WebSocket is ESP-IDF's, inside the server's own buffers |
| The radio image's HTTP body buffers: one per worker, `MAX_BODY_BYTES` + 1 each, allocated once in PSRAM (spec 023) | 4 workers × 16 KB | 64 KB, **not added**: the engine's worst case already counts 4 bodies of 16 KB (above); beside the realistic home, which does not count them, they add 64 KB | PSRAM | E, from the bound |
| The radio image's HTTP workers' stacks (`CONFIG_SPIRAM_ALLOW_STACK_EXTERNAL_MEMORY`, spec 023) | 4 workers × 10 KB | 40 KB | PSRAM | E, from the setting |
| One TLS session at a time (mbedTLS, buffers in PSRAM by `CONFIG_MBEDTLS_EXTERNAL_MEM_ALLOC`) | 1 session; default 16 KB input and 16 KB output records | 64 KB (about 40 to 50 KB steady, more during the handshake that parses a certificate) | PSRAM | E, ESP-IDF and mbedTLS documentation figures |
| Update buffers: the download's chunk buffer and the flash write buffer | 1 update at a time | 32 KB | PSRAM | E |
| Margin | | the rest | PSRAM | |

Arithmetic, worst case: engine 1024 + transport 64 + workers' stacks 40 + TLS 64 + update 32 =
1224 KB, margin 2048 − 1224 = **824 KB**. Realistic home: 300 + 64 + body buffers 64 + 40 + 64 + 32
= 564 KB, margin 1484 KB. The totals
fit with room. The engine's small allocations land in internal RAM first (next section), so the
PSRAM really used is lower than this sum and the margin larger.

## Internal RAM

`CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL=4096` keeps allocations of up to 4 KB in internal RAM, and
`CONFIG_SPIRAM_MALLOC_RESERVE_INTERNAL=65536` keeps 64 KB of it for the network stack and DMA
buffers. When the internal RAM is full, a small allocation falls back to PSRAM; the reserve is
never taken by an allocation that may go to PSRAM.

| Item | Worst case | M / E |
| ---- | ---------- | ----- |
| Free at boot | 359 KB | M, QEMU (upper estimate) |
| Network stack of the larger of Wi-Fi and Ethernet: Wi-Fi station and lwIP, default buffers | 70 KB (50 to 70 KB documented) | E, ESP-IDF |
| Ethernet: EMAC DMA buffers (10 receive and 10 transmit buffers of about 1.6 KB) and driver | about 40 KB, **not added**: only one link is active, the budget takes the larger (Wi-Fi) | E, ESP-IDF defaults |
| Task stacks: main (8 KB, `CONFIG_ESP_MAIN_TASK_STACK_SIZE`), 4 HTTP workers, the radio loop, the WebSocket writer | 64 KB | E, usual ESP-IDF stack sizes; the firmware sets only the main task's stack |
| The radio image: the engine thread's stack (32 KB) and the UART's two rings (4 KB receive, 1 KB transmit), all internal RAM | 37 KB, inside the 64 KB of task stacks above for the stack; measured on the box: engine stack 27 KB unused of 32 KB, receive ring highest use 262 B of 4096 B | E (set in `firmware/src/radio_engine.rs` and `radio_uart.rs`), M for the use (2026-11-22) |
| mbedTLS, its small internal allocations | 8 KB | E |
| Left for the engine's small allocations and the rest, before the reserve | 359 − 70 − 64 − 8 = 217 KB | |
| Reserve kept for DMA and the network stack | 64 KB | settings |
| **Left for the engine's small allocations** | 217 − 64 = **153 KB**, pessimistic by up to 64 KB: Wi-Fi's own internal allocations (the 70 KB above) draw from the reserve, so the reserve and the network line are partly counted twice | |

This is the tight resource. About 70 percent of the worst case's bytes at its peak (673 of 940
KB live at the peak) are allocations of 4 KB or less; applied to the realistic home's 187 KB, about 130 KB would
go to the internal RAM first (**E**, by that ratio). It fits the 153 KB, barely, with the
estimates above; the worst case does not fit, and its small allocations spill into PSRAM (the
fallback above), which has room. Consequences, to read again with the box's own figures (the engine run in
the firmware under QEMU, then on the Dongle Max, "On the box" below):

- the stack, TLS and network figures are estimates: the box's figures ("On the box", below) cover the
  boot and the engine; the network rows stay estimates: the run of spec 023 measured one snapshot with Wi-Fi up ("On the box", below), not the network's cost as a difference of two `heap:` minima; the serial link's rings and the engine thread's stack are the radio image's (spec 021), their figures come from its run on the box (below);
- lowering `SPIRAM_MALLOC_ALWAYSINTERNAL` (for example to 512 bytes) moves more of the engine to
  PSRAM at the price of slower access: the knob to turn if the internal RAM runs short;
- the internal RAM left after the network is what the firmware's memory line shows at boot.

## The interface

The interface is served from flash and is not copied to RAM; its first-load budget (170 KB gzip)
is unchanged by this budget.

## Not counted

The firmware's own code and constants are in flash. FreeRTOS' kernel objects, the log buffers and
the Wi-Fi driver's static data are inside the "network stack" and "task stacks" rows by their
documented orders of magnitude, not item by item. No WebAssembly runtime exists in the tool, so
none is counted.

## What QEMU measured (spec 016)

The firmware runs the engine on the simulated radio with the realistic home of D20 and prints
the `engine:` line above. Reading it:

- **Peak 312 KB** against the 350 KB ceiling of D27 (D20's 300 KB, above which the first run
  went), 170 KB held when the scenario ends. The largest phase is the second (the recipes'
  start and the simulator's build, 312 KB), then the load of the home (258 KB), the run itself
  (197 KB) and the radio's identification (186 KB). The peak is an **upper bound** of the engine's: it counts the engine thread's 32 KB stack, the
  simulator and its memory file, the load of the home from its files (the text of
  `devices.json`, 94 KB, and of `equipments.json`, 71 KB, are read whole and parsed: the largest
  transient, about 260 KB with the home), and the memory is read as the sum of the low
  watermarks of the internal RAM and the PSRAM, phase by phase (see D26). Both readings, the
  whole run's (329 KB at the first run) and the per-phase one (312 KB), are upper bounds of the
  true peak; the per-phase one is the tighter and never below the true peak. The bound holds for
  what the run exercised: the realistic home, the scenario of `firmware/fixtures/`, no network. It is labelled "QEMU,
  with simulator" and measured on the box with the measuring image ("On the box", below).
- **Internal RAM: 107 KB free at the lowest**, against the 64 KB floor of `scripts/qemu.sh`. This
  run starts no network: the 70 KB the budget sets for Wi-Fi are not in the figure, and the
  margin that decides is the box's, with the network up ("On the box", below).
- **`SPIRAM_MALLOC_ALWAYSINTERNAL`**: kept at 4096. At 512 the same run read a peak of 359 KB (sum
  of the whole run's watermarks, against 329 KB at 4096 by the same reading, in the first run) and 149 KB of
  internal RAM at the lowest: more of the engine goes to PSRAM, the total does not shrink. The
  knob moves bytes between the two memories; the way to lower the total is to stream the
  loading of the home.
- **The emulator's flash is slow, and the run does not wait for it.** The engine saves
  `devices.json` (94 KB) once per device when its route is learned (the fixtures carry no route, so
  each device sets one at its first message), about 20 s each in the emulator (far less on the
  box). The 49 saves of the scenario are walked to their end and their
  text dropped (`scenario: 49 saves run without the emulator's flash` in the log); the last save,
  on stop, goes to the flash for real. The simulated radio's own memory stays in RAM once read.
- **The scenario plays at its own pace.** The scenario is held until the radio is identified and
  listening and the loop has turned once; the simulator's clock then counts only the time it
  waits, not the emulator's slow flash writes, so the events come one loop turn after the other.
  The radio drops no callback while it plays (`scripts/qemu.sh` fails if one is logged after
  `scenario: started`), and the `engine:` line counts the devices that got at least one measure
  into the engine (`50 devices (50 reported)`; the script fails when fewer than all did).
  `MAX_QUEUED_CALLBACKS` (16) is a real bound of the radio, but this run does not reach it. The
  engine writes its due files inside the radio loop; on the box a 94 KB write stalls the radio, which
  the emulator no longer shows (the box's stall is in "On the box", below).
- **Orders at the start**: 3 `motion-light` instances of 16 lights each queue 48 orders at start
  against `orders::queue::MAX_UNATTENDED` (32); the 16 beyond it are logged `recipe order ...
  dropped: the queue of recipe orders is full` and not sent, again when the alarms arrive. The bound holds, and the warning is
  seen at every start of the realistic home (also on the Mac).
- **Orders on the target**: motion sensors 10, 20 and 30 of the realistic home report an alarm
  at 60 s of scenario time; `motion-light` switches the lights, and the orders go through the
  simulated radio, which answers with each light's state.

## On the box (spec 020)

Measured on the Dongle Max by `scripts/box.sh`: the box's image for the boot, the measuring image
(feature `flash-saves`: the realistic home with every save written to the flash, timed) for the
engine. The run was made on 2026-11-22 with the box on USB. A save stall above 1 s, or a peak above D27's 350 KB, is written here with its figure
and becomes a line of the roadmap's inbox (or a decision for the ceiling): the limit is never raised.

| Figure | Value | Where it comes from |
| ------ | ----- | ------------------- |
| Free memory at boot, box image | 359 KB internal RAM free (largest block 160 KB), 2045 KB of 2048 KB PSRAM free (2026-11-22) | `sh scripts/box.sh log --image box` (the `memory:` line) |
| Engine, realistic home: heap peak, internal RAM minimum, PSRAM minimum | Heap peak 312 KB (QEMU: 312 KB), internal RAM minimum 107 KB free, PSRAM minimum 1916 KB free; 50 devices, 10 zones, 50 equipments, 3 recipes (2026-11-22) | `sh scripts/box.sh log --image sim` (the `engine:` line) |
| FAT partition: size, use after the realistic home | 7964 KB; 208 KB used after the realistic home, kept at the next boot (2026-11-22) | `sh scripts/box.sh log --image sim` (the `storage-after:` line, logged after the last save; `storage:` is the boot value) |
| Save stall: the longest save (file, KB) and the total | Longest save 3770 ms (`devices.json`, 98 KB); 51 saves in 172 s in all; 3945 ms for the one save of a second boot (2026-11-22). Above the 1 s limit: a line of the roadmap inbox | `sh scripts/box.sh log --image sim` (the `saves:` line) |
| Internal RAM with Wi-Fi up | Measured on 2026-11-22, one device known, 10 s after `serving:`: `memory-network: internal 171 KB free (largest block 62 KB), psram 1904 KB free of 2048 KB`. The boot's `memory:` line showed 289 KB internal free, so Wi-Fi, the server and the engine's start took about 118 KB of internal RAM together (a difference of two snapshots, M). Not computed: the network's cost as a difference of two `heap:` minima (234 KB without Wi-Fi, minus a run's first `heap:` internal minimum) and the projection for the realistic home (107 KB minus that cost); until then, the network's cost is the estimate of the table above (70 KB, E) | `sh scripts/box.sh log --image radio` (the `heap:` and `memory-network:` lines) |
| The 33 requests of an equipment with 16 + 16 bindings | Measured on 2026-11-22 over Wi-Fi, the garage's two devices known, a browser tab open on the box: eight runs, never a 503 `Busy`, totals 14.4, 9.0, 10.5, 10.7, 16.1, 19.1, 22.3 and 20.6 s (slowest request 0.8 to 1.1 s); in two runs (9.0 and 10.5 s) one request got no answer (curl status 000), the other 32 answered 201. Over the 10 s limit in seven of the eight: each creation takes 0.15 to 0.65 s against about 0.05 s for a GET (a line of the roadmap inbox) | `sh scripts/measure-api.sh <box address>` |
| The serial link's buffers | 115200 baud, receive ring 4096 B, highest use 262 B, full 0 times, in 4 minutes with one device (2026-11-22) | `sh scripts/box.sh log --image radio` (the `serial:` line, every 240 s) |
| The radio image's internal RAM: minimum free and the engine stack's high-water mark | Boot of the radio image: internal 352 KB free (largest block 160 KB), PSRAM 2046 KB free. After 4 minutes with one device: internal 303 KB free (minimum 234 KB), PSRAM 2020 KB free (minimum 2016 KB), engine stack 27 KB unused of 32 KB. Nothing against D27: the home holds one device (2026-11-22) | `sh scripts/box.sh log --image radio` (the `heap:` line) |
