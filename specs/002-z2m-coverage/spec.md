# Spec 002: z2m-coverage

## Context

mini-Sowel recognises devices by reusing Zigbee2MQTT's device definitions
(`zigbee-herdsman-converters`, MIT), exported to a data file the box interprets without
JavaScript. Only the part of those definitions that is data, or that calls code shared by the
whole library, can be exported; code written for one device or one brand cannot. The framing note
(`inputs/note-de-cadrage-mini-sowel.md`, "Key functions" and "Risks and unknowns") asks for that
share to be measured at the first step. The user narrowed it to the leading brands (Tuya,
Schneider, Sonoff, Legrand, IKEA) and asked to widen the list to the leaders of the Zigbee market,
so that nothing basic is forgotten.

A first look by reading the library's text (2026-11-20, version 26.135.0) gave orders of
magnitude: lighting brands almost fully declarative, Tuya mostly datapoint tables, sensors and
wiring devices (Aqara, Sonoff, Schneider, Legrand, Heiman) often code; the Sonoff SNZB-02 uses a
converter written in its device file, although it only speaks standard clusters. This spec makes
that measure exact and repeatable, by loading the library.

Specified in autonomy: the user approved in advance (2026-11-20) the specs reviewed by
`spec-reviewer` without an important finding, and will read this spec and the report the next day.
Choices made without the user are listed under "Decided in autonomy".

## Goals

1. Know, per leading brand, how many definitions the box can reuse by export, and how many need
   code.
2. Know how many definitions a generic reading of the standard Zigbee clusters would handle even
   without any definition (the safety net for the SNZB-02 and simple devices).
3. Know the cost of the export: how many distinct shared functions of the library the box must
   reimplement to cover each brand.

## Out of scope

- The export itself (the data file, its signature, its publication): milestone B.
- The generic reading of standard clusters in the core: A5 and milestone B. Here it is only
  counted.
- Any change to the core, the host, the firmware or the interface.
- Running the tool in CI on a schedule, or tracking the measure over time.

## Requirements

### R1: The tool

A Node program in `tools/z2m-coverage/`, written in TypeScript, run with `npm run report` from its
folder. It loads every definition shipped by the pinned version of `zigbee-herdsman-converters`,
as the library prepares it (modern extensions merged into converters, configure, exposes), and
writes the report `tools/z2m-coverage/report.md`. It also prints the report's totals line to the
terminal. It needs no network once its packages are installed, and no hardware.

### R2: Where each function of a definition comes from

The library's preparation (`prepareDefinition`) wraps the `configure`, `onEvent` and functional
`exposes` of the modern extensions in new closures defined in `dist/index.js`, and the wrapped
functions cannot be reached from the prepared object. So the tool collects the functions from the
**raw definition and its `extend[]` entries**, as `processExtensions` reads them: from the
definition and from each extension, the `convert` of each `fromZigbee` converter, the
`convertSet` and `convertGet` of each `toZigbee` converter,
`configure` (a function or a list), `onEvent` (a function or a list), a functional `exposes`, and
the functions found in `meta`, searched into nested objects (the `valueConverter` objects of
`meta.tuyaDatapoints` carry `to` and `from`). `endpoint` functions (a data mapping of endpoint
names) and converter `options` are not collected; the report's limits say so. `prepareDefinition`
is called on each definition only to find those that throw (R6).

The origin of each function is found by locating its source text in the library's files:

- **shared**: defined in the library's common code: `dist/converters/`, `dist/lib/` except the
  brand files below, and `dist/index.js`;
- **brand**: defined in a brand library file of `dist/lib/`, code written for one manufacturer
  (for example `ikea.js`, `lumi.js`, `philips.js`); the list of brand files is written in the
  tool and printed in the report. `dist/lib/legacy.js` (old converters, mostly for Tuya devices)
  counts as a brand file: the cautious side. `dist/lib/tuya.js` is not a brand file: it is the
  shared Tuya datapoint interpreter, used by hundreds of white labels;
- **device**: defined in a device file `dist/devices/<brand>.js`;
- when the same text is found in several categories, the widest reuse wins: found in shared
  code, it is **shared** (a shared implementation exists, so the box reimplements it once; for
  example `(v) => v`, written in the Tuya interpreter and in device files); found in a brand file
  and device files only, it is **brand**;
- **unknown**: its text is found nowhere; counted as **device** (the cautious side); the report
  prints their number and the number of definitions that are device code only because of them.

A function is also marked **configure** when it was collected from a `configure`, so the report
can tell device code that only binds and sets up reporting from device code that interprets
messages.

### R3: Classes of a definition

Each definition gets one class, the first that applies:

1. **Tuya table**: it has `meta.tuyaDatapoints`, and no function of origin brand, device or
   unknown;
2. **Declarative**: every function is shared (a definition with no function at all is
   declarative);
3. **Brand code**: no device function, at least one brand function;
4. **Device code**: at least one device or unknown function.

Among device code, a definition is **device code in configure only** when all its device or
unknown functions were collected from `configure`: the box would need a binding and reporting
table for it, not code that reads messages.

Classes 1 and 2 are **exportable**. Separately, a definition is **standard** when every `cluster`
of its `fromZigbee` converters belongs to the list of standard clusters below, whatever its class.
Standard clusters: `genBasic`, `genPowerCfg`, `genOnOff`, `genLevelCtrl`, `genScenes`,
`genMultistateInput`, `lightingColorCtrl`, `msTemperatureMeasurement`, `msRelativeHumidity`,
`msPressureMeasurement`, `msIlluminanceMeasurement`, `msOccupancySensing`, `ssIasZone`,
`closuresWindowCovering`, `hvacThermostat`, `seMetering`, `haElectricalMeasurement`. A definition
with no `fromZigbee` converter is not standard (nothing tells what it reports).

A definition is **standard with extras** when it is not standard, at least one of its
`fromZigbee` clusters is standard, and every other one is a manufacturer-specific cluster (its
name starts with `manuSpecific`): the generic reading handles its main function (a Philips Hue
bulb's on/off, level and colour) and misses only the manufacturer's extras (its effects).

### R4: Brands

A brand is a list of vendor names (the `vendor` field of a definition and of its `whiteLabel`
entries), matched without regard to case (the library writes `SONOFF`). The list is written in the tool: Philips (Hue), IKEA, Aqara (with Xiaomi), Tuya, Moes,
Zemismart, Lidl, Sonoff, Schneider Electric, Legrand, Innr, Ledvance, OSRAM, Gledopto, Heiman,
Lexman (ADEO), Third Reality, Develco (frient), Bosch, NodOn, Somfy, Danfoss, Hager, Niko, Ubisys,
Profalux. A definition belongs to the brand of its `vendor`. A brand with no definition in the
library still gets its line, with 0, and is flagged in the limits section. Every definition, of any vendor, counts in the totals line.

### R5: The report

`tools/z2m-coverage/report.md`, in English, holds:

- a header: the library's version, the date the report was made, the number of definitions loaded;
- one table, one row per brand, sorted by number of definitions then by name: definitions,
  white labels, the share (rounded percent) and count of each class, the share of device code in
  configure only, the share exportable, the share standard, the share standard with extras, the
  share neither exportable nor standard nor standard with extras (the gap), and the number of distinct shared functions the brand's exportable
  definitions use (the converters `prepareDefinition` adds to every definition are left out of
  that count, and named once);
- each brand's vendor strings as found in the library;
- a totals row over the brands, and a totals row over the whole library;
- a "Devices of interest" table: model, vendor, class, standard or not, and the origin of each of
  its functions, for every model listed in the tool (at least the Sonoff SNZB-02, and one model
  per class taken from the measured data);
- a "Method and limits" section: how origins and classes are decided, the brand files, the number
  of unknown functions, and what the measure cannot see (a shared function may still be hard to
  reimplement; `configure` written in a device file is often only binding and reporting).

### R6: Failures

If the library cannot be loaded, or loads no definition, the tool prints why and exits with a
non-zero status without writing the report. A definition that throws while being prepared is
counted as device code, named in the report's limits section, and does not stop the tool.

## Living spec delta

### docs/living-spec/README.md

- New section "Measures the coverage of Zigbee2MQTT's definitions": a tool on the Mac
  (`tools/z2m-coverage`, `npm run report`) loads the pinned version of Zigbee2MQTT's device
  library and writes a report per leading brand: the share of definitions the box can reuse by
  export (declarative, Tuya tables), the share that needs code (brand or device), the share a
  generic reading of the standard clusters handles, and the cost in shared functions; with the
  Sonoff SNZB-02's line.
- Changed: "No device, equipment, zone, mode or recipe exists in the tool" stays true; the
  sentence is kept.
- New: History row 002.

### No longer true

- `docs/architecture.md`: the overview and the blocks list only core, host, firmware, interface and
  checks → a "Tools" block is added (`tools/`, Node, run on the Mac, not shipped).
- `README.md`, `CLAUDE.md`: the commands list → the tool's command is added.
- Searched: `tools`, `Zigbee2MQTT`, `z2m`, `coverage`, `A1` in `docs/`, `README.md`, `CLAUDE.md`,
  `inputs/README.md`, `.claude/`.

## Acceptance criteria

- [x] `cd tools/z2m-coverage && npm ci && npm run report` writes `report.md` and prints the
      totals line.
- [x] The report has one row per brand of R4, the totals rows, and the SNZB-02's line.
- [x] The classes of R3 add up to 100 % per row (rounding aside) and the counts add up exactly.
- [x] `sh scripts/validate.sh` runs the tool's type check and tests, and is green.

## Edge cases

- A definition with no function and no converter (a router dongle): declarative, not standard.
- A function whose text is in no file: unknown, counted as device. In shared code and elsewhere: shared.
- A definition that throws when prepared: device code, named in the limits.
- A brand absent from the library: its row with 0, no division by zero.
- A white label of another vendor (a Tuya device sold as Moes): counted in its definition's brand
  (Tuya), and in the white labels column; not double-counted.
- The library cannot be loaded or loads nothing: error, no report written.
- A vendor written in another case (`SONOFF`): matched to its brand.

## Sowel contract

None: the tool exposes nothing to Sowel and changes no type.

## Decided in autonomy

- The measure loads the library (it runs its JavaScript on the Mac) rather than reading its text:
  more exact; the package is pinned to an exact version.
- "Exportable" means data plus calls to shared code, not data alone: shared functions are
  reimplemented once in Rust and referenced by name in the export; their number is the cost.
- The SNZB-02 is expected to be **device code** but **standard**: the generic reading covers it.
- After the first run (2026-11-20): a text found in shared code and elsewhere is shared, not
  unknown (the identity function `(v) => v` had pushed about 200 Tuya tables into device code);
  `toZigbee` converters read through `convertSet` and `convertGet` (they have no `convert`: the
  first run read none of them, and lighting brands came out more exportable than they are);
  "standard with extras" added (583 Philips Hue lights carry one manufacturer-specific cluster
  beside the standard ones, and came out not standard).
- `dist/lib/legacy.js` counts as brand code, not shared (spec review question; the cautious side).
- The report has a "device code in configure only" share (spec review question): it separates
  real code from a binding table, which the export of milestone B needs.
- The report is committed: it is the result the roadmap step asked for, and its header says which
  library version and date it measures.

## How the user will check it

Read `tools/z2m-coverage/report.md`: the brands table, the totals, the SNZB-02's line; run
`npm run report` again and get the same table.

Result of the run (2026-11-20, library 26.135.0, no hardware): 4682 definitions; exportable 44 %
(Tuya tables 394, declarative 1681), brand code 22 %, device code 33 %; standard 50 %, standard
with extras 21 %, gap 17 %. Leading brands: Tuya 53 % exportable, gap 22 %; Philips Hue 2 %
exportable but 97 % standard with extras (its code is in Philips' brand library); IKEA, Gledopto,
Ledvance 92 to 100 % standard; Sonoff gap 51 %, Legrand gap 86 %, Aqara gap 38 %. The SNZB-02 is
device code and standard: the generic reading covers it. Demo approved in advance (autonomous run);
the user reads the report on 2026-11-21.
