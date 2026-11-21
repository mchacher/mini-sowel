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
| Engine, realistic home | peak 187 KB, 198 KB with every log entry at 200 bytes | M, Mac: `cargo test -p solite-core --test memory_ceiling -- --nocapture` |
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
| The engine: devices, zones, equipments, recipes, events, live queues, orders, largest answer, save path | the core's `MAX_*` (`devices::MAX_DEVICES` 64, `zones::MAX_ZONES` 32, `equipments::MAX_EQUIPMENTS` 64, `recipes::MAX_INSTANCES` 16, ...) | 1024 KB (ceiling; measured 987 KB). Realistic home: 300 KB (ceiling; measured 198 KB), shown beside, not added | Large allocations in PSRAM; small ones first in internal RAM, see below | M |
| Transport buffers outside the core: the HTTP server's and the WebSocket's read and write buffers | 4 HTTP workers, `live::MAX_CLIENTS` 4 clients, about 8 KB each | 64 KB | PSRAM | E, from the ESP-IDF HTTP server's default buffer sizes |
| One TLS session at a time (mbedTLS, buffers in PSRAM by `CONFIG_MBEDTLS_EXTERNAL_MEM_ALLOC`) | 1 session; default 16 KB input and 16 KB output records | 64 KB (about 40 to 50 KB steady, more during the handshake that parses a certificate) | PSRAM | E, ESP-IDF and mbedTLS documentation figures |
| Update buffers: the download's chunk buffer and the flash write buffer | 1 update at a time | 32 KB | PSRAM | E |
| Margin | | the rest | PSRAM | |

Arithmetic, worst case: engine 1024 + transport 64 + TLS 64 + update 32 = 1184 KB, margin
2048 − 1184 = **864 KB**. Realistic home: 300 + 64 + 64 + 32 = 460 KB, margin 1588 KB. The totals
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
| mbedTLS, its small internal allocations | 8 KB | E |
| Left for the engine's small allocations and the rest, before the reserve | 359 − 70 − 64 − 8 = 217 KB | |
| Reserve kept for DMA and the network stack | 64 KB | settings |
| **Left for the engine's small allocations** | 217 − 64 = **153 KB**, pessimistic by up to 64 KB: Wi-Fi's own internal allocations (the 70 KB above) draw from the reserve, so the reserve and the network line are partly counted twice | |

This is the tight resource. About 70 percent of the worst case's bytes at its peak (673 of 940
KB live at the peak) are allocations of 4 KB or less; applied to the realistic home's 187 KB, about 130 KB would
go to the internal RAM first (**E**, by that ratio). It fits the 153 KB, barely, with the
estimates above; the worst case does not fit, and its small allocations spill into PSRAM (the
fallback above), which has room. Consequences, to read again when the box measures its own
figures (the engine run in the firmware under QEMU, then the Dongle Max):

- the stack, TLS and network figures are estimates until the box reports them;
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
  with simulator" and measured again on the box (E3).
- **Internal RAM: 107 KB free at the lowest**, against the 64 KB floor of `scripts/qemu.sh`. This
  run starts no network: the 70 KB the budget sets for Wi-Fi are not in the figure, and the
  margin that decides is the box's, measured in E3.
- **`SPIRAM_MALLOC_ALWAYSINTERNAL`**: kept at 4096. At 512 the same run read a peak of 359 KB (sum
  of the whole run's watermarks, against 329 KB at 4096 by the same reading, in the first run) and 149 KB of
  internal RAM at the lowest: more of the engine goes to PSRAM, the total does not shrink. The
  knob moves bytes between the two memories; the way to lower the total is to stream the
  loading of the home.
- **The emulator's flash is slow, and the run does not wait for it.** The engine saves
  `equipments.json` (71 KB) after the measures that change it, about 20 s each in the emulator
  (far less on the box). The saves of the scenario's 49 turns are walked to their end and their
  text dropped (`scenario: 49 saves run without the emulator's flash` in the log); the last save,
  on stop, goes to the flash for real. The simulated radio's own memory stays in RAM once read.
- **The scenario plays at its own pace.** The scenario is held until the radio is identified and
  listening and the loop has turned once; the simulator's clock then counts only the time it
  waits, not the emulator's slow flash writes, so the events come one loop turn after the other.
  The radio drops no callback while it plays (`scripts/qemu.sh` fails if one is logged after
  `scenario: started`), and the `engine:` line counts the devices that got at least one measure
  into the engine (`50 devices (50 reported)`; the script fails when fewer than all did).
  `MAX_QUEUED_CALLBACKS` (16) is a real bound of the radio, but this run does not reach it.
- **Orders at the start**: 3 `motion-light` instances of 16 lights each queue 48 orders at start
  against `orders::queue::MAX_UNATTENDED` (32); the 16 beyond it are logged `recipe order ...
  dropped: the queue of recipe orders is full` and not sent. The bound holds, and the warning is
  seen at every start of the realistic home (also on the Mac).
- **Orders on the target**: motion sensors 10, 20 and 30 of the realistic home report an alarm
  at 60 s of scenario time; `motion-light` switches the lights, and the orders go through the
  simulated radio, which answers with each light's state.
