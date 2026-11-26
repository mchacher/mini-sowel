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
| Engine, worst case (every bound at its maximum; the colour object on each light, D51) | peak 891 KB, 1015 KB with every recipe log entry at 200 bytes (spec 027: Sowel's optional equipment fields at their 8192-byte total; `settings.json` 245 bytes at its longest; spec 031: the 16 lights each hold a four-field `color` object, 632 bytes each; on all 64 devices it would read 1045 KB, over the ceiling, but a colour bulb has 4 data where the test's devices have 16, D51) | M, Mac: `cargo test -p solite-core --test memory_worst_case -- --nocapture` |
| Engine, realistic home | peak 170 KB, 193 KB with every log entry at 200 bytes (2026-11-25, unchanged by spec 034) | M, Mac: `cargo test -p solite-core --test memory_ceiling -- --nocapture` |
| Home lock held per batch of measures, realistic home, one live client listening to everything (`measures`, `recompute`, `tick`, `run_recipes`: one turn of the engine's loop) | temperature report: 70 µs on average, 142 µs at most; motion report driving motion-light: 56 µs, 98 µs (spec 027, release build); on the box roughly 20 to 50 times longer, **E**, not measured | M, Mac: `cargo test --release -p solite-core --test lock_time -- --ignored --nocapture` |
| Share of small allocations in the worst case at its peak | 673 KB in allocations of 4 KB or less, 267 KB above, 940 KB in all: the live bytes of the whole test process at the moment of its peak, which was the 925 KB measured then (spec 025) plus the 14 KB the process holds before the test starts | M, Mac, a counting allocator split by size at the peak, run once and not kept |
| Free memory at boot, in QEMU | `memory: internal 359 KB free (largest block 160 KB), psram 2045 KB free of 2048 KB` | M, `sh scripts/qemu.sh` |
| Engine, realistic home, on the 32-bit target (QEMU, with the simulator inside) | `engine: 50 devices (50 reported), 10 zones, 50 equipments, 3 recipes; heap 159 KB (peak 311 KB); internal 238 KB free (min 110 KB); psram 1980 KB free (min 1916 KB)`: 311 KB at the highest, 159 KB held at the end, the internal RAM never below 110 KB free (spec 034; 327, 176 and 95 KB before) | M, `sh scripts/qemu.sh` (spec 016, decision D26) |

The worst case counts, in its peak, the HTTP bodies (4 workers of the host, 16 KB each,
`api::MAX_BODY_BYTES`), the live clients' queues (`live::MAX_CLIENTS` of 4, `MAX_QUEUED_BYTES`
of 8 KB each, one batch in flight each), the orders queues, and the radio's queues by arithmetic
(48 KB, **E**: the radio is not driven by that test). The test's header lists every `pub const
MAX_*` of the core and how it is reached; a test fails when one is missing.

The account (spec 029) is small and not driven by the worst case: at most 8 sessions of 72 bytes
(`auth::MAX_SESSIONS`) and 10 attempt times of 8 bytes (`auth::MAX_ATTEMPTS`), in RAM only, about
700 bytes beside the home, and one account of about 250 bytes. The password hash keeps under 1 KB
on the stack of the HTTP worker that runs it (inside its 10 KB), nothing on the heap that grows; it
takes 1.1 to 1.4 s on the box (**M**, a login timed on the box on 2026-11-25, the request whole;
the CPU runs at 160 MHz, `CONFIG_ESP_DEFAULT_CPU_FREQ_MHZ`).

The backup's key derivation (spec 035) is ten times that (100,000 iterations; under a second on
the Mac), run in slices that pause 10 ms every 5,000 iterations so the task watchdog is fed. On
the box it is about 11 to 14 s (**E**, ten times the login's 1.1 to 1.4 s, the same algorithm,
equal on the Mac). The one box export measured, on 2026-11-25 on an HTTP worker's stack, took 70 to
88 s from the request to the panic at its first file read: a gap with the estimate whose cause is
not known (the login runs on the same kind of PSRAM stack, so the stack's place alone does not
explain it). Each derivation logs `backup: key derived in <n> ms (<iterations> iterations)`,
which splits the derivation from the rest of the request: the box check reads it. Its states
stay on the `backup` thread's stack, inside its 10 KB.

## PSRAM, 2048 KB

| Item | Bound | Worst case | Where | M / E |
| ---- | ----- | ---------- | ----- | ----- |
| The engine: devices, zones, equipments, recipes, events, live queues, orders, largest answer, save path | the core's `MAX_*` (`devices::MAX_DEVICES` 64, `zones::MAX_ZONES` 32, `equipments::MAX_EQUIPMENTS` 64, `recipes::MAX_INSTANCES` 16, ...) | 1024 KB (ceiling; measured 987 KB). Realistic home: 300 KB (ceiling; measured 193 KB), shown beside, not added | Large allocations in PSRAM; small ones first in internal RAM, see below | M |
| Transport buffers outside the core: the HTTP server's and the WebSocket's read and write buffers | 4 HTTP workers, `live::MAX_CLIENTS` 4 clients, about 8 KB each | 64 KB | PSRAM | E, from the ESP-IDF HTTP server's default buffer sizes; the box holds no `tungstenite` buffers (the Mac host's WebSocket library): its WebSocket is ESP-IDF's, inside the server's own buffers; the request head's scratch, `CONFIG_HTTPD_MAX_REQ_HDR_LEN` 4096 (1024 before spec 035, for the backup's password header), held by ESP-IDF's server (one per server as far as the sources are remembered, not per socket): at most 3 KB more, inside the 64 KB (E, not read in the sources here) |
| The radio image's HTTP body buffers: one per worker, `MAX_BODY_BYTES` + 1 each, allocated once in PSRAM (spec 023) | 4 workers × 16 KB | 64 KB, **not added**: the engine's worst case already counts 4 bodies of 16 KB (above); beside the realistic home, which does not count them, they add 64 KB | PSRAM | E, from the bound |
| The radio image's HTTP workers' stacks (`CONFIG_SPIRAM_ALLOW_STACK_EXTERNAL_MEMORY`, spec 023) | 4 workers × 10 KB | 40 KB | PSRAM | E, from the setting |
| One TLS session at a time (mbedTLS, buffers in PSRAM by `CONFIG_MBEDTLS_EXTERNAL_MEM_ALLOC`): the release check and pull of spec 039, through ESP-IDF's HTTP client; the 409s of the core forbid a second session (a check, a pull, the automatic check: one job at a time) | 1 session; default 16 KB input and 16 KB output records | 64 KB (about 40 to 50 KB steady, more during the handshake that parses a certificate) | PSRAM | E, ESP-IDF and mbedTLS documentation figures; the on-box check of spec 039 reads the internal RAM minimum during a check, and this row is filled from it |
| Update: the one 4096-byte chunk buffer of the OTA thread, allocated once (spec 032); the image is streamed and never held whole, and ESP-IDF's flash write copies from PSRAM through its own internal bounce buffer | 1 update at a time (`update::CHUNK_BYTES`) | 4 KB | PSRAM | M for the core's part (1 MB through a counting fake peaks at under 5 KB of heap), E for the box |
| Backup (spec 035): one job at a time (export or restore), each with one 4096-byte chunk buffer on the heap (`backup::CHUNK_BYTES`; the box's export holds two, the archive reader's and the HTTP server's send buffer, `http_server.rs`) and the manifest (under 4 KB, `MAX_MANIFEST_BYTES`); the archive is streamed and never held whole; the hash, MAC and cipher states and the key derivation (under 1 KB in all) are on the stack of the `backup` thread that runs the job (internal RAM, see the internal RAM table) | 1 job (`backup::MAX_FILES` 7 files, 2 MiB at most) | 10 KB | PSRAM | M for the core's part (a 1 MB archive through counting fakes peaks at about 5 KB of heap exported and about 10 KB restored, `full_backup_memory`, bound 32 KB), E for the box |
| Margin | | the rest | PSRAM | |

Arithmetic, worst case: engine 1024 + transport 64 + workers' stacks 40 + TLS 64 + update 4 + backup 10 =
1206 KB, margin 2048 − 1206 = **842 KB**. Realistic home: 300 + 64 + body buffers 64 + 40 + 64 + 4
+ 10 = 546 KB, margin 1502 KB. The totals
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
| The radio image's OTA thread's stack (spec 032): every flash call of an update and the probation's tick, in internal RAM for the whole run because the flash cannot be written from a PSRAM stack; it never runs the signature check (the `update` thread does) | 6 KB, **not added**: one more thread inside the 64 KB of task stacks above, and the 153 KB left below absorbs it; the box run of spec 032 reads the internal RAM free after an update. Its heaviest call, ESP-IDF's image check at the end of an installation (`esp_ota_end` runs `esp_image_verify`), used 2.45 KB of a stack in QEMU (2026-11-25, on the running image) | E, from the setting in `firmware/src/ota.rs`; M in QEMU for the image check |
| The radio image's `update` thread's stack (spec 032, D60): an update's installation runs on it (the body read, the header's Ed25519 check, the image's SHA-256, the answer), because the signature check needs about 9 KB of stack and an API worker has 10 KB with its own frames (the box overflowed on 2026-11-25); started for the request and ended with it; it logs `update: thread stack <n> of 16384 bytes never used` at its end | 16 KB during an update, **not added**: the 153 KB left below absorbs it; a request whose thread cannot get the block answers 503. Simultaneous uploads each take their 16 KB before the second is refused 409 (at most 4, one per worker, 64 KB for a moment). The whole installation used 9.8 KB of an Xtensa stack in QEMU (a bare thread 0.76 KB, the signature check alone 9.2 KB); the QEMU run's check (three installations on this thread) leaves 5940 of 16384 bytes unused and fails below 2048 | E, from the setting in `firmware/src/job_thread.rs`; M in QEMU (2026-11-25) |
| The radio image's `backup` thread's stack (spec 035, D59): an export or a restore runs on it, because every flash access, a read too, turns the caches off, which ESP-IDF refuses from a PSRAM stack (the API workers'); started for the job and ended with it, one job at a time; it logs `backup: thread stack <n> of 10240 bytes never used` at its end | 10 KB during a job, **not added**: the 153 KB left below absorbs it; a job whose thread cannot get the block answers 503 | E, from the setting in `firmware/src/job_thread.rs` |
| The radio image's supervisor record (spec 036): 24 words of 32 bits, in the ESP32's RTC slow memory (`.rtc_noinit`), not in the internal RAM or the PSRAM; no heap, no flash; one `Mutex<Supervisor>` of a few hundred bytes; no new thread (the OTA thread's one-second tick runs the supervisor's tick) | 96 bytes of RTC memory | E, from the bound |
| The radio image's `update-check` and `update-pull` threads' stacks (spec 039, D68): a check, the automatic check or a pull runs on one (the manifest, the redirects, the TLS handshake, the download, then for a pull the installation of D60), because ESP-IDF's TLS runs in the calling task and the handshake and the signature check each need more than a worker's 10 KB; started for the job and ended with it, one at a time; each logs `<name>: thread stack <n> of 20480 bytes never used` at its end | 20 KB during a job, **not added**: one more thread inside the 64 KB of task stacks, the 153 KB left absorbing it; a job whose thread cannot get the block answers 503 (the automatic check logs it). The size is by reasoning (ESP-IDF's HTTPS examples give such a task 8 KB, the installation's Ed25519 check 9.2 KB runs after the handshake, never nested in it); QEMU measures the pull's core part on this thread: **9412 of 20480 bytes unused** (2026-11-26, one pull of a 64 KB file through one redirect, the check fails below 8192; the handshake is not in it), the handshake's share is read on the box | E for the size, M in QEMU for the core's part (2026-11-26), the handshake's on the box |
| The HTTP client's two buffers (spec 039 R8): 4096 bytes to receive (GitHub's redirect `Location`, a signed address of several hundred characters, is over the client's default 512) and 1024 to send, allocated by `esp_http_client` for a check or a pull and freed with it; at or under `CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL`, so in internal RAM | 5 KB for the length of a job, one session at a time, **not added**: the 153 KB left absorbs it | E, from the setting in `firmware/src/fetch.rs` |
| The worst overlap of the job threads' stacks: an automatic check or pull (20 KB), an upload (16 KB) and a backup (10 KB) at once, each answered by its own thread, with the client's 5 KB | 20 + 16 + 10 KB + 5 KB = 51 KB of the 153 KB left (a pull and an upload exclude each other, 409, so the worst case that can happen is a pull or an upload with a backup: 35 KB); the automatic check, an upload and a backup are the worst overlap allowed | E, from the settings |
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

## Flash: the radio image's size

The radio image's application, as `esptool elf2image` writes it (the size line of
`sh scripts/validate.sh`, `sh scripts/box.sh flash`), against the 4 MB slot:

| Tree | Application image | Share of the slot |
| ---- | ----------------- | ----------------- |
| Before spec 039 (`main` at the merge of spec 038, built 2026-11-26) | 2,193,632 bytes (2142 KB) | 52 % |
| With spec 039 (2026-11-26) | 2,328,192 bytes (2273 KB) | 55 % |

The difference, **134,560 bytes (131 KB)**, is the TLS client (ESP-IDF's HTTP client, `esp-tls`, the
client side of mbedTLS), the common certificate bundle (`CONFIG_MBEDTLS_CERTIFICATE_BUNDLE_DEFAULT_CMN`,
the authorities of most of the web in one table in the flash) and the release code of the core and
`fetch.rs`; the split between them is not measured. The bundle is in flash only (a verification
reads it there). The image stays under the 75 percent the size line warns about, and the update's
slot fits it twice over.

## Not counted

The firmware's own code and constants are in flash. FreeRTOS' kernel objects, the log buffers and
the Wi-Fi driver's static data are inside the "network stack" and "task stacks" rows by their
documented orders of magnitude, not item by item. No WebAssembly runtime exists in the tool, so
none is counted.

## What QEMU measured (spec 016)

The firmware runs the engine on the simulated radio with the realistic home of D20 and prints
the `engine:` line above. Reading it:

- **Peak 311 KB** (2026-11-25, spec 034; 327 KB after the simulator's memory file is written in a stream, 312 KB at spec 016, 363 KB while the file was built whole in a text of over 32 KB) against the 350 KB ceiling of D27 (D20's 300 KB, above which the first run
  went), 159 KB held when the scenario ends. The largest phase is the second (the recipes'
  start and the simulator's build, 311 KB), then the load of the home (208 KB), the run itself
  (186 KB) and the radio's identification (176 KB). The peak is an **upper bound** of the engine's: it counts the engine thread's 32 KB stack, the
  simulator and its memory file, the load of the home from its files (the text of
  `devices.json`, 94 KB, and of `equipments.json`, 71 KB, are read whole and parsed: the largest
  transient, about 210 KB with the home, 260 KB before spec 034), and the memory is read as the sum of the low
  watermarks of the internal RAM and the PSRAM, phase by phase (see D26). Both readings, the
  whole run's (329 KB at the first run) and the per-phase one (311 KB), are upper bounds of the
  true peak; the per-phase one is the tighter and never below the true peak. The bound holds for
  what the run exercised: the realistic home, the scenario of `firmware/fixtures/`, no network. It is labelled "QEMU,
  with simulator" and measured on the box with the measuring image ("On the box", below).
- **Internal RAM: 110 KB free at the lowest (95 KB before spec 034, 107 KB at spec 016)**, against the 64 KB floor of `scripts/qemu.sh`. This
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

## Spec 034: shared texts, one-pass read (2026-11-25)

The devices read back from `devices.json` share the catalog's constant texts (`key`, `category`,
`unit`, `valueOn`, `valueOff`) instead of holding a heap copy of each, and `parse_list` builds the
list in one pass. Measured before (`main`, 88fe91b) and after:

| Figure | Before | After |
| ------ | ------ | ----- |
| The 50 devices read back, held (Mac, `devices_load_memory`) | 73938 bytes (72 KB) | 67450 bytes (65 KB) |
| Peak of that read (Mac) | 123106 bytes (120 KB) | 109986 bytes (107 KB) |
| `memory_ceiling`, realistic home (Mac) | 174330 bytes (170 KB); 198180 bytes (193 KB) with 200-byte log entries | unchanged |
| QEMU `engine:` line | heap 176 KB (peak 327 KB); internal 221 KB free (min 95 KB) | heap 159 KB (peak 311 KB); internal 238 KB free (min 110 KB) |
| QEMU, load of the home (phase 1) | 258 KB | 208 KB |

The gain is about 6 KB held on the Mac counter and 17 KB in QEMU (32-bit blocks and headers weigh
more there), taken mostly from the internal RAM, where small blocks go first.

## On the box (spec 020)

Measured on the Dongle Max by `scripts/box.sh`: the box's image for the boot, the measuring image
(feature `flash-saves`: the realistic home with every save written to the flash, timed) for the
engine. The run was made on 2026-11-22 with the box on USB. A save stall above 1 s, or a peak above D27's 350 KB, is written here with its figure
and becomes a line of the roadmap's inbox (or a decision for the ceiling): the limit is never raised.

| Figure | Value | Where it comes from |
| ------ | ----- | ------------------- |
| Free memory at boot, box image | 359 KB internal RAM free (largest block 160 KB), 2045 KB of 2048 KB PSRAM free (2026-11-22) | `sh scripts/box.sh log --image box` (the `memory:` line) |
| Engine, realistic home: heap peak, internal RAM minimum, PSRAM minimum | Heap peak 312 KB (QEMU: 312 KB then, 327 KB on 2026-11-25, 311 KB with spec 034), internal RAM minimum 107 KB free, PSRAM minimum 1916 KB free; 50 devices, 10 zones, 50 equipments, 3 recipes (2026-11-22) | `sh scripts/box.sh log --image sim` (the `engine:` line) |
| FAT partition: size, use after the realistic home | 7964 KB; 208 KB used after the realistic home, kept at the next boot (2026-11-22) | `sh scripts/box.sh log --image sim` (the `storage-after:` line, logged after the last save; `storage:` is the boot value) |
| Save stall: the longest save (file, KB) and the total | Longest save 3770 ms (`devices.json`, 98 KB); 51 saves in 172 s in all; 3945 ms for the one save of a second boot (2026-11-22). Above the 1 s limit: a line of the roadmap inbox | `sh scripts/box.sh log --image sim` (the `saves:` line) |
| Internal RAM with Wi-Fi up | Measured on 2026-11-22, one device known, 10 s after `serving:`: `memory-network: internal 171 KB free (largest block 62 KB), psram 1904 KB free of 2048 KB`. The boot's `memory:` line showed 289 KB internal free, so Wi-Fi, the server and the engine's start took about 118 KB of internal RAM together (a difference of two snapshots, M). Not computed: the network's cost as a difference of two `heap:` minima (234 KB without Wi-Fi, minus a run's first `heap:` internal minimum) and the projection for the realistic home (107 KB minus that cost); until then, the network's cost is the estimate of the table above (70 KB, E) | `sh scripts/box.sh log --image radio` (the `heap:` and `memory-network:` lines) |
| The 33 requests of an equipment with 16 + 16 bindings | Measured on 2026-11-22 over Wi-Fi, the garage's two devices known, a browser tab open on the box: eight runs, never a 503 `Busy`, totals 14.4, 9.0, 10.5, 10.7, 16.1, 19.1, 22.3 and 20.6 s (slowest request 0.8 to 1.1 s); in two runs (9.0 and 10.5 s) one request got no answer (curl status 000), the other 32 answered 201. Over the 10 s limit in seven of the eight: each creation takes 0.15 to 0.65 s against about 0.05 s for a GET (a line of the roadmap inbox) | `sh scripts/measure-api.sh <box address>` |
| The serial link's buffers | 115200 baud, receive ring 4096 B, highest use 262 B, full 0 times, in 4 minutes with one device (2026-11-22) | `sh scripts/box.sh log --image radio` (the `serial:` line, every 240 s) |
| The radio image's internal RAM: minimum free and the engine stack's high-water mark | Boot of the radio image: internal 352 KB free (largest block 160 KB), PSRAM 2046 KB free. After 4 minutes with one device: internal 303 KB free (minimum 234 KB), PSRAM 2020 KB free (minimum 2016 KB), engine stack 27 KB unused of 32 KB. Nothing against D27: the home holds one device (2026-11-22). Since spec 034 the line also prints the largest free block of each memory (the fragmentation's indicator; not yet read on the box) | `sh scripts/box.sh log --image radio` (the `heap:` line) |
