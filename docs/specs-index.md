# Specs index

Every change ever specified. One row per folder in `specs/`.

| Spec | Name | What it does | Date | Status |
| ---- | ---- | ------------ | ---- | ------ |
| 001 | walking-skeleton | The tool starts and shows its name on the Mac, on the ESP32 and in the browser; the whole chain of checks works | 2026-11-20 | done |
| 002 | z2m-coverage | Measures, per leading Zigbee brand, the share of Zigbee2MQTT definitions the box can reuse by export, and the share a generic reading of standard clusters handles | 2026-11-20 | done |
| 003 | firmware-in-qemu | The firmware boots in Espressif's QEMU on the Mac and shows its free memory; a broken boot fails the checks | 2026-11-20 | done |
| 004 | talk-to-the-radio | ASH and EZSP in Rust in the core; the host identifies the Dongle Max's radio over TCP | 2026-11-21 | in progress |
| 005 | pair-a-device | The radio runs a Zigbee network; the host opens pairing and shows the device that joins (manufacturer, model) | 2026-11-21 | in progress |
| 006 | read-the-measures | Measures of the standard clusters read generically: reporting set up on pairing, shown live with `--listen`, run after run | 2026-11-21 | in progress |
