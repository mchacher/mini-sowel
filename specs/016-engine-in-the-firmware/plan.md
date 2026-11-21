# Plan: Spec 016

## Steps

- [ ] 1. Living spec updated (delta of the spec)
- [ ] 2. `core::engine`: the loop and the saver moved from the host, `Storage` trait; host adapted;
      every existing test green
- [ ] 3. `crates/sim` builds for the ESP32
- [ ] 4. Fixtures of the realistic home and their test
- [ ] 5. Firmware: storage mount, network line, `radio: none`; feature `sim`: seed, engine,
      `engine:` line
- [ ] 6. `scripts/qemu.sh` and `scripts/validate.sh`; QEMU figures in `docs/memory-budget.md`
- [ ] 7. D26, architecture, roadmap (E1 done when both specs are), what this makes false
- [ ] 8. Types mirroring Sowel, API, WebSocket, interface: not applicable

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| core::engine | A turn with measures from the simulated radio, in-memory storage | Device data updated, zone recomputed, files due written |
| core::engine | Motion through the loop with a `motion-light` instance | The relay's order is sent through the radio |
| core::engine | Storage write fails | Logged, retried after the retry delay; on stop, the error is returned |
| core::engine | Storage write fails then succeeds | The file is written at the retry |
| core::engine | Empty storage (no files) | The home starts empty |
| core::engine | A file unreadable | Loading fails with the file named (as the host does today) |
| host | Existing tests (serve, store, sim, demo) | Unchanged and green |
| core::engine | The sink gets the verbose lines and the write failures | Lines as the host prints them today |
| core::engine | Stop flag set | The loop returns, the saver writes what is due |
| sim | `finished()` before, at and after the last event | false, true, true |
| sim | Memory file written then read back (no unix mode) | Same network and keys |
| host | The fixture scenario with the fixture home through `--sim` | Ends, every device reported |
| fixtures | Fixture files equal the realistic home's builder | Equal; `UPDATE_FIXTURES=1` rewrites them |
| fixtures | A fixture edited by hand | The test fails |
| firmware (QEMU, sim) | Blank partition | `storage: formatted (...)` then `storage: /data, ...`, grepped by `qemu.sh` |
| firmware (QEMU, sim) | No `network.json` | `network: wifi (setting auto), no link started`, grepped by `qemu.sh` |
| firmware (QEMU, sim) | Realistic home, scenario played | `engine:` line, peak ≤ 300 KB, internal min ≥ 64 KB |
| firmware (QEMU, box image) | Boot | `storage:`, `network:`, `radio: none` (the build is checked by validate; the boot by hand once, noted in the demo) |
| firmware | Invalid `network.json`, full partition | Covered by the core's tests (spec 015 R5, the engine's write-failure tests); no QEMU run |

## Validation

- [ ] Validate command green
- [ ] Agent review done, blocking findings fixed
- [ ] Demo seen and approved by the user
