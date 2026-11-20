# Architecture: Spec 001

## Overview

```
crates/core: NAME, VERSION, banner()
   ├── crates/host/src/main.rs      prints banner()               (Mac)
   └── firmware/src/main.rs         log::info!(banner())          (ESP32, ESP-IDF)

ui/src/App.tsx: Sidebar (SowelLogo + MINI-SOWEL) + h1 APP_NAME  (browser)
```

## Data

None.

## Files changed

| File | Change |
| ---- | ------ |
| `Cargo.toml`, `rust-toolchain.toml` | Mac workspace (core, host), Rust 1.100, shared lints |
| `crates/core/` | Library: `NAME`, `VERSION`, `banner()`; test `tests/banner.rs` |
| `crates/host/` | Binary printing the banner; test `tests/start.rs` runs it |
| `firmware/` | ESP-IDF project: `Cargo.toml`, `rust-toolchain.toml` (esp), `.cargo/config.toml` (target, ESP-IDF v5.5.5), `build.rs`, `sdkconfig.defaults`, `src/main.rs` |
| `ui/` | Preact + Vite + Tailwind 4 app; Sowel's tokens, theme, design rules and logo (`docs/design-system.md`); `App.test.tsx`; budget script |
| `scripts/validate.sh`, `scripts/check-docs.sh` | The verdict and the docs check |
| `.githooks/`, `.claude/hooks/`, `.claude/settings.json` | Commit hooks |

## Reused

- Sowel: `design-system/tokens.css`, the `@theme` blocks of `ui/src/index.css`,
  `ui/eslint/design-classes.js`, `SowelLogo.tsx`, and the sidebar's header classes from
  `Sidebar.tsx`.
- `esp-idf-svc`'s standard start: `link_patches()`, then `EspLogger::initialize_default()`.
