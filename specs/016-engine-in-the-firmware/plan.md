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
| sim | Builds without `cfg(unix)` parts | `cargo check` for the ESP32 target through the firmware's `sim` build |
| fixtures | Fixture files equal the realistic home's builder | Equal; `UPDATE_FIXTURES=1` rewrites them |
| fixtures | A fixture edited by hand | The test fails |
| firmware (QEMU, sim) | Realistic home, scenario played | `engine:` line, peak ≤ 300 KB, internal min ≥ 16 KB |
| firmware (QEMU, sim) | Second boot on the same image | Data mounted, not seeded again (checked once by hand, noted in the demo) |
| firmware (QEMU, box image) | Boot | `storage:`, `network: wifi (setting auto), not started`, `radio: none` |

## Validation

- [ ] Validate command green
- [ ] Agent review done, blocking findings fixed
- [ ] Demo seen and approved by the user
