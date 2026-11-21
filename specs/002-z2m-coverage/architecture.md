# Architecture: Spec 002

## Overview

```
zigbee-herdsman-converters (pinned, node_modules), loaded from `zigbee-herdsman-converters/devices/index`
   │ raw definitions and their extend[]          │ source text of dist/**/*.js
   ▼                                             ▼
collect functions per definition  ──►  origin of each function (shared, brand, device, unknown)
   │                                             │
   └──────────────► classify (Tuya table, declarative, brand code, device code; standard)
                                   │
                    aggregate per brand (vendor lists) and in total
                                   │
                    render report.md  +  totals line on the terminal
```

A Node program, outside the Rust workspace and the interface: `tools/z2m-coverage/`, its own
`package.json` and `package-lock.json`. It runs on the Mac only; nothing of it is shipped in the
box. TypeScript run directly by Node 24 (type stripping, no build step), type-checked by `tsc`,
tested with `node:test`.

The logic is split so it can be tested without the library: the origin lookup takes a list of
`{file, text}` and a function's source text; the classifier takes plain objects; the report
renderer takes the aggregated numbers.

## Data

| Name | Type | Where |
| ---- | ---- | ----- |
| Brands | list of `{name, vendors[]}` | `src/brands.ts` |
| Brand library files | list of file names in `dist/lib/` | `src/brands.ts` |
| Standard clusters | list of cluster names | `src/classify.ts` |
| Devices of interest | list of models | `src/brands.ts` |
| Report | Markdown | `tools/z2m-coverage/report.md` (committed) |

No data in the box; no bound needed (a run on the Mac, about 5 000 definitions).

## Files changed

| File | Change |
| ---- | ------ |
| `tools/z2m-coverage/package.json`, `package-lock.json` | New: `zigbee-herdsman-converters` (exact version), `typescript`, `@types/node`; scripts `report`, `typecheck`, `test` |
| `tools/z2m-coverage/tsconfig.json` | New: strict, `noEmit`, `allowImportingTsExtensions` |
| `tools/z2m-coverage/src/origin.ts` | New: index of the library's source files, origin of a function |
| `tools/z2m-coverage/src/collect.ts` | New: functions of a prepared definition |
| `tools/z2m-coverage/src/classify.ts` | New: class and standard flag of a definition |
| `tools/z2m-coverage/src/brands.ts` | New: brands, brand files, devices of interest |
| `tools/z2m-coverage/src/aggregate.ts` | New: counts per brand and in total |
| `tools/z2m-coverage/src/report.ts` | New: Markdown rendering |
| `tools/z2m-coverage/src/main.ts` | New: load the library, run, write, print |
| `tools/z2m-coverage/test/*.test.ts` | New: tests of the plan |
| `tools/z2m-coverage/report.md` | New: the report |
| `tools/z2m-coverage/.prettierrc` or reuse | Formatting with the UI's Prettier settings |
| `scripts/validate.sh` | New step "Tools: types, tests"; a missing `node_modules` is reported like the UI's |
| `.github/workflows/ci.yml` | `npm ci` in `tools/z2m-coverage` |
| `.gitignore` | `tools/*/node_modules` |
| `docs/living-spec/README.md` | The delta of the spec |
| `docs/architecture.md` | The Tools block |
| `docs/decisions.md` | D10: how the Node tools of D1 are laid out and checked (refers to D1) |
| `README.md`, `CLAUDE.md` | The command |

## Reused

- Node 24 and npm, already used by `ui/` and by the CI (`actions/setup-node`).
- The validate step pattern of the UI (missing `node_modules` reported with the command to run).
- The vendor and brand list of the first look (2026-11-20).
