# CLAUDE.md

Guidance for Claude working on mini-Sowel. The **first file to read** in every conversation. It is
short on purpose: it points to where things are, it does not describe them. **It stays under
about 150 lines**: a subject that grows moves to `docs/` or becomes a check.

## Where to find context

| You want to know...                          | Read this                                   |
| -------------------------------------------- | ------------------------------------------- |
| What the tool does today (source of truth)   | [docs/living-spec/](docs/living-spec/README.md) |
| How the code is organised                    | [docs/architecture.md](docs/architecture.md) |
| Why it is built this way                     | [docs/decisions.md](docs/decisions.md)      |
| What comes next                              | [docs/roadmap.md](docs/roadmap.md)          |
| Every change ever specified                  | [docs/specs-index.md](docs/specs-index.md)  |
| The design of one change                     | `specs/NNN-name/{spec,architecture,plan}.md` |
| The documents the project started from       | [inputs/](inputs/README.md)                 |
| How the interface looks                      | [docs/design-system.md](docs/design-system.md) |
| How specs and code are reviewed              | [.claude/agents/](.claude/agents/)          |
| Sowel's model, API and recipes (the contract) | The Sowel repository, see "Sowel contract" below |

## True north

**True north**: a minimal Sowel in a Sonoff Dongle Max (about 43 $) that automates an annex or a
small home on its own (no server, no Docker, no cloud) by proposing the recipes itself, and that
becomes an extension of the full Sowel when there is one.
**As soon as possible**: a personal open-source project (steps 1 to 6 of the framing note), used by
its author, their family and the Sowel users who flash their Dongle Max from a public release;
maintained by its author with Claude, on foundations clean enough (Sowel-format API, signed
binaries, tested memory ceilings) for a product to stay possible without starting over.
**Success looks like**: a flashed Dongle Max runs for a month in a real annex without
intervention; pair a sensor and pick the room, it proposes a relevant recipe, and its equipments
appear in Sowel.

Every feature is judged against it. A request that leads away from it: say so before building.

## The user

The author of Sowel (a TypeScript home-automation engine): at ease with TypeScript, React and
home automation, **a beginner in Rust and on the ESP32**. Talk in their language (French), plainly;
explain a Rust or embedded term the first time it appears (crate, borrow, `no_std`, partition,
eFuse...). They review the spec and the demo, not the code.

## Stack

Rust for the core, the host program (Mac) and the ESP32 firmware (`std` on ESP-IDF through
`esp-idf-svc`, Xtensa toolchain installed by `espup`); Preact + TypeScript + Vite + Tailwind 4 for
the interface, with Sowel's design system. Details and reasons: decision D1.

## Commands

```
cd ui && npm ci                                  # once, after a clone
cd tools/z2m-coverage && npm ci                  # once, after a clone
cargo run -p mini-sowel-host                     # the host program (Mac)
cd ui && npm run dev                             # the interface, on http://localhost:5173
cargo test --workspace                           # Rust tests (core, host)
cargo test -p mini-sowel-core --test banner      # one Rust test file
cd ui && npx vitest run src/App.test.tsx         # one UI test file
cd tools/z2m-coverage && npm run report          # the Zigbee2MQTT coverage report
sh scripts/qemu.sh                               # the firmware, booted in Espressif's QEMU
. "$HOME/export-esp.sh" && cd firmware && cargo build --release   # ESP32 firmware (macOS, Linux)
sh scripts/validate.sh                           # the verdict
sh scripts/validate.sh --fast                    # quick checks (the pre-commit hook)
```

`sh scripts/validate.sh` is the verdict: docs, format, lint, types, tests, UI size budget, ESP32
firmware build and boot in QEMU. It must be green before any commit. After a fresh clone: `git config
core.hooksPath .githooks` (Claude's commit hook also sets it). **Never flash the Dongle Max**
without the user's explicit yes: its original firmware exposes the radio over TCP for development
(decision D6).

## How we work

- **Spec-driven**: every change goes through `/mini-sowel-feature` or `/mini-sowel-fix`: spec
  approved, test plan before code, checks, review, demo.
- **Living spec**: `docs/living-spec/` describes the tool as it is. Each spec's "living spec delta"
  is applied in the same change as the code. Code that disagrees with it is a bug in one of the
  two: never resolve it silently.
- **Docs kept current, by checks** (`scripts/check-docs.sh`): a `feat` or `fix` commit that changes
  code but not `docs/living-spec/` is refused, unless its message has the line
  `Living-Spec: none — <reason>` (the reason is required). Every `specs/NNN-*` folder has its three
  files (`spec.md`, `architecture.md`, `plan.md`) and its row in `docs/specs-index.md`, or the
  validate command fails.
- **Docs say what is true today**: the living spec, the architecture, the design system and the
  README describe the tool as it is, in the present. What comes next, and how far the work has got,
  live in `docs/roadmap.md`; `docs/decisions.md` records what was decided, with its date (D1 holds
  the target architecture). `scripts/check-docs.sh` refuses words of the future, status lines and
  paths of one computer. It finds words, not truth: every change also searches for and corrects
  what it makes false elsewhere (feature skill 3.4, fix skill), and the code review checks it.
- **Git**: never commit to `main` except trivial fixes (the hook asks the user to confirm);
  branches `feat/`, `fix/`, `docs/`, `chore/`; conventional commits in English, scopes in
  `.claude/skills/mini-sowel-feature/reference.md`; remote `origin` on GitHub, pull requests
  with `gh pr create`, CI runs `sh scripts/validate.sh` (decision D9); **no AI attribution line** (no
  `Co-Authored-By`, no "Generated with"); never push, merge or publish without the user's explicit
  yes.
- **Review agents**: `.claude/agents/` (`spec-reviewer`, `code-reviewer`), launched by the skills.
  If one is not available as an agent type (a conversation started outside the project folder),
  launch a general-purpose agent and tell it to follow that file word for word.
- **Roadmap**: `docs/roadmap.md`, planned with `/mini-sowel-roadmap`, reviewed after every feature.
  A **new need, at any time**: one line in its inbox, then carry on; never slipped into the change
  in progress.
- **Secrets and data**: no secret in the code (`.env`, ignored by git); no signing key in the
  repository, ever; no real personal data (a home's devices, network names, addresses) in the
  repository: test scenarios are synthetic.

## Sowel contract

Everything mini-Sowel exposes takes the names, types and JSON of Sowel's API (decision D3). The
reference is the Sowel repository (`github.com/mchacher/sowel`, usually cloned next to this one as
`../sowel`):

- model: `src/shared/types.ts` (devices, equipments, zones, modes, recipes, `EngineEvent`); **it
  wins over Sowel's docs** where they disagree;
- per entity: `docs/technical/data-model/*.md`; API: `docs/technical/api-reference.md`;
- recipes: `docs/technical/recipe-development.md`, and the `sowel-recipe-*` repositories listed in
  Sowel's `plugins/registry.json`.

A spec that adds a type, a field or an event names the Sowel file it mirrors. Something mini-Sowel
needs on the Sowel side becomes an ordinary Sowel issue, never a change made from here.

## Conventions

- **The core never touches hardware**: `crates/core` has no ESP-IDF or OS dependency; radio,
  storage, network and updates reach it through adapter traits that `crates/host` and `firmware/`
  implement. Logic lives in the core, where it is tested on the Mac.
- **Errors**: functions that can fail return `Result`; no `unwrap`/`expect`/`panic!` outside tests
  and start-up code in `main`. An event handler never brings the engine down.
- **Logging**: the `log` crate (`log::info!`...); `println!` only for the host's command-line
  output.
- **Memory**: the ESP32 has 2 MB of PSRAM. A collection that grows with devices, events or history
  has a bound, written next to it.
- **JSON**: `serde`, with Sowel's field names (`#[serde(rename_all = "camelCase")]` where Sowel uses
  camelCase); ids are UUID v4 strings, dates ISO 8601, as in Sowel.
- **Tests**: Rust: integration tests in `crates/<crate>/tests/`, unit tests of private logic in a
  `#[cfg(test)] mod tests` beside it; the firmware has no tests (its logic is in the core). UI:
  Vitest, `*.test.tsx` beside the component.
- **Interface**: Sowel's tokens and Tailwind classes only (rules in `docs/design-system.md`); a
  Sowel component is ported unchanged through `preact/compat` when possible.
- **Files in English**, comments included; the conversation in the user's language.
