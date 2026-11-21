// Markdown rendering of the report, from the aggregated numbers (no library access here).
import type { AnalysedDefinition, Aggregate, Row } from "./aggregate.ts";

export interface InterestRow {
  vendor: string;
  model: string;
  why: string;
  /** Undefined when the library has no such model. */
  record: AnalysedDefinition | undefined;
}

export interface ReportData {
  libraryVersion: string;
  date: string;
  definitionsLoaded: number;
  aggregate: Aggregate;
  brandFiles: readonly string[];
  /** Keys of the converters `prepareDefinition` adds to every definition (left out of the counts). */
  commonConverters: readonly string[];
  interest: readonly InterestRow[];
}

const CLASS_LABELS = {
  "tuya-table": "Tuya table",
  declarative: "Declarative",
  brand: "Brand code",
  device: "Device code",
} as const;

function cell(count: number, pct: number): string {
  return `${count} (${pct}%)`;
}

function rowCells(row: Row): string[] {
  const p = row.pct;
  return [
    row.name,
    String(row.definitions),
    String(row.whiteLabels),
    cell(row.tuyaTable, p.tuyaTable),
    cell(row.declarative, p.declarative),
    cell(row.brand, p.brand),
    cell(row.device, p.device),
    cell(row.configureOnly, p.configureOnly),
    cell(row.exportable, p.exportable),
    cell(row.standard, p.standard),
    cell(row.standardWithExtras, p.standardWithExtras),
    cell(row.gap, p.gap),
    String(row.sharedFunctions),
  ];
}

function table(header: readonly string[], rows: readonly (readonly string[])[]): string {
  const escape = (s: string): string => s.replaceAll("|", "\\|");
  const line = (cells: readonly string[]): string => `| ${cells.map(escape).join(" | ")} |`;
  return [line(header), line(header.map(() => "---")), ...rows.map(line)].join("\n");
}

/** The one line printed on the terminal. */
export function totalsLine(aggregate: Aggregate, libraryVersion: string): string {
  const l = aggregate.library;
  return (
    `z2m-coverage ${libraryVersion}: ${l.definitions} definitions, ` +
    `exportable ${l.exportable} (${l.pct.exportable}%) = Tuya table ${l.tuyaTable} + declarative ${l.declarative}; ` +
    `brand code ${l.brand} (${l.pct.brand}%); device code ${l.device} (${l.pct.device}%); ` +
    `standard ${l.standard} (${l.pct.standard}%); standard with extras ${l.standardWithExtras} (${l.pct.standardWithExtras}%); gap ${l.gap} (${l.pct.gap}%)`
  );
}

function functionsSummary(record: AnalysedDefinition): string {
  if (record.functions.length === 0) return "no function";
  const counts = new Map<string, number>();
  for (const f of record.functions) {
    const label = f.configure ? `${f.origin} (configure)` : f.origin;
    counts.set(label, (counts.get(label) ?? 0) + 1);
  }
  const parts = [...counts.entries()].sort().map(([label, n]) => `${label} ${n}`);
  const named = record.functions
    .filter((f) => f.origin !== "shared")
    .map((f) => f.name)
    .slice(0, 6);
  return named.length > 0
    ? `${parts.join(", ")}; not shared: ${named.join(", ")}`
    : parts.join(", ");
}

function interestRow(item: InterestRow): string[] {
  const r = item.record;
  if (!r) return [item.model, item.vendor, "not in the library", "-", "-", item.why];
  return [
    r.model,
    r.vendor,
    CLASS_LABELS[r.class] + (r.configureOnly ? ", configure only" : ""),
    r.standard ? "standard" : r.standardWithExtras ? "standard with extras" : "not standard",
    functionsSummary(r),
    item.why,
  ];
}

export function renderReport(data: ReportData): string {
  const { aggregate: a } = data;
  const absent = a.brands.filter((b) => b.definitions === 0).map((b) => b.name);
  const out: string[] = [];

  out.push("# Zigbee2MQTT definitions coverage", "");
  out.push(`- Library: \`zigbee-herdsman-converters\` ${data.libraryVersion}`);
  out.push(`- Date: ${data.date}`);
  out.push(`- Definitions loaded: ${data.definitionsLoaded}`, "");

  out.push("## Coverage per brand", "");
  out.push(
    "Each cell is a count of definitions and its share of the row. Configure only is a share of the " +
      "device-code definitions. Exportable is Tuya table plus declarative. Standard with " +
      "extras: the standard clusters plus manufacturer-specific ones. The gap is neither " +
      "exportable nor standard nor standard with extras. Shared functions is the number of distinct shared functions used by " +
      "the exportable definitions of the row.",
    "",
  );
  out.push(
    table(
      [
        "Brand",
        "Definitions",
        "White labels",
        "Tuya table",
        "Declarative",
        "Brand code",
        "Device code",
        "Device, configure only",
        "Exportable",
        "Standard",
        "Standard with extras",
        "Gap",
        "Shared functions",
      ],
      [...a.brands, a.brandsTotal, a.library].map(rowCells),
    ),
    "",
  );
  out.push(
    `The converters that the library adds to every definition (${data.commonConverters.length} of them: ` +
      `${data.commonConverters.join(", ")}) are left out of the shared-function counts.`,
    "",
  );

  out.push("## Vendor strings per brand", "");
  out.push(
    table(
      ["Brand", "Vendor strings found in the library"],
      a.brands.map((b) => [b.name, b.vendors.length > 0 ? b.vendors.join(", ") : "none"]),
    ),
    "",
  );

  out.push("## Devices of interest", "");
  out.push(
    table(
      ["Model", "Vendor", "Class", "Clusters", "Origin of each function", "Why"],
      data.interest.map(interestRow),
    ),
    "",
  );

  out.push("## Method and limits", "");
  out.push(
    "- The tool loads every definition of the library, collects the functions of each raw " +
      "definition and of its `extend[]` entries (the `convert` of each `fromZigbee` " +
      "converter, the `convertSet` and `convertGet` of each `toZigbee` converter, `configure`, `onEvent`, a functional `exposes`, and the functions " +
      "found in `meta`, nested objects included), and locates each function's source text " +
      "(`Function.prototype.toString`) in the library's `dist/**/*.js` files.",
    "- Origin of a function: shared (`dist/converters/`, `dist/lib/` except the brand files, " +
      "`dist/index.js`), brand (a brand file of `dist/lib/`), device (`dist/devices/<brand>.js`), " +
      "unknown (found nowhere; counted as device, the cautious side). A text found in several " +
      "categories takes the widest reuse: shared if it is in shared code, else brand if it is in a " +
      "brand file, else device.",
    `- Brand files: ${data.brandFiles.map((f) => `\`${f}\``).join(", ")}. \`lib/legacy.js\` counts as ` +
      "brand code. `lib/tuya.js` is shared: the Tuya datapoint interpreter used by many white labels.",
    "- Class of a definition, the first that applies: Tuya table (`meta.tuyaDatapoints` and no " +
      "brand, device or unknown function), declarative (every function shared, or no function), " +
      "brand code (no device function, at least one brand function), device code (at least one " +
      "device or unknown function). Tuya table and declarative are exportable.",
    "- Device code in configure only: every device or unknown function comes from a `configure`. " +
      "Such code is often only binding and reporting setup, which an export would carry as a table.",
    "- Standard: every `cluster` of the `fromZigbee` converters is one of the standard clusters " +
      "(genBasic, genPowerCfg, genOnOff, genLevelCtrl, genScenes, genMultistateInput, " +
      "lightingColorCtrl, msTemperatureMeasurement, msRelativeHumidity, msPressureMeasurement, " +
      "msIlluminanceMeasurement, msOccupancySensing, ssIasZone, closuresWindowCovering, " +
      "hvacThermostat, seMetering, haElectricalMeasurement). A definition with no `fromZigbee` " +
      "converter is not standard. A converter with no cluster makes the definition not standard.",
    "- Two functions with the same source text count as one shared function.",
    `- Functions whose text is found in no file (unknown, counted as device): ${a.unknownFunctions} distinct. ` +
      `${a.unknownDeviceDefinitions} definitions are device code only because of them.`,
    a.threwModels.length > 0
      ? `- Definitions that threw while prepared (counted as device code): ${a.threwModels.join(", ")}.`
      : "- No definition threw while prepared.",
    absent.length > 0
      ? `- Brands with no definition in the library (row with 0): ${absent.join(", ")}.`
      : "- Every brand has at least one definition in the library.",
    "- Not collected: `endpoint` functions (a data mapping of endpoint names), converter " +
      "`options`.",
    "- What the measure cannot see: a shared function may still be hard to reimplement; `configure` " +
      "written in a device file is often only binding and reporting; a function whose text is " +
      "found in a shared file may be a closure that captures device-specific data.",
    "",
  );
  return out.join("\n");
}
