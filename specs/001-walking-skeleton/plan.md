# Plan: Spec 001

## Steps

- [x] 1. Living spec written (Purpose, Users, "Starts and shows its name")
- [x] 2. Rust workspace: core and host, with their tests
- [x] 3. Firmware: ESP-IDF project using the core, built for the ESP32
- [x] 4. Interface: Preact app with Sowel's design system, test, budget
- [x] 5. Checks: `scripts/validate.sh`, docs check, git hooks, Claude's commit hook
- [x] 6. Docs: README, CLAUDE.md, architecture, decisions, roadmap, design system

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| core | The banner names the tool | `NAME` is `mini-sowel`; `banner()` is `mini-sowel 0.1.0` |
| host | The program starts | Exit code 0; standard output is the banner |
| ui | The page opens | The `h1` is `mini-sowel`; the sidebar (`complementary`) is present |
| ui build | First load | Under 170 KB gzip (`npm run check:bundle`) |
| firmware | Build for the ESP32 | `cargo build --release` succeeds for `xtensa-esp32-espidf` |
| checks | Raw colour, arbitrary font size, off-scale radius in a component | ESLint refuses each one |

## Validation

- [x] Validate command green
- [ ] Agent review done, blocking findings fixed: not applicable: setup
- [ ] Demo seen and approved by the user: not applicable: setup
