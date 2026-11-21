# inputs

The documents Solite started from. Kept in git: they are the project's origin. A document with
confidential or real personal data does not go here, or is listed in `.gitignore`.

| Document | What it brought |
| -------- | --------------- |
| [note-de-cadrage-mini-sowel.md](note-de-cadrage-mini-sowel.md) | The framing note (in French, 2026-11-20): the objective, the scope kept from Sowel, the architecture (ESP32 in Rust, EFR32MG24 radio over EZSP), the key functions (recipe proposals, WebAssembly recipes, Zigbee2MQTT definitions, link with Sowel), the interface budget, signed updates with rollback, the hardware (Sonoff Dongle Max), development on the Mac, the risks and the steps. Source of the true north, of decisions D1 to D6 and of the roadmap inbox |

Also used at setup, outside this folder: the Sowel repository (`github.com/mchacher/sowel`): its
model and API (`src/shared/types.ts`, `docs/technical/`) for the Sowel contract (decision D3), and
its design system (`design-system/`, `ui/`) for `docs/design-system.md` (decision D4).
