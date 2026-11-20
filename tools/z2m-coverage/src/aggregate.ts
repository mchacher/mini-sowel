// Counts per brand and in total, from one analysed record per definition.
import type { Brand } from "./brands.ts";
import { collectClusters, collectFunctions, hasTuyaDatapoints } from "./collect.ts";
import type { RawDefinition } from "./collect.ts";
import { classify, isExportable } from "./classify.ts";
import type { DefinitionClass } from "./classify.ts";
import type { Origin } from "./origin.ts";

export interface AnalysedFunction {
  /** The function's source text: two functions with the same text are one function to reimplement. */
  key: string;
  name: string;
  origin: Origin;
  configure: boolean;
}

export interface AnalysedDefinition {
  model: string;
  vendor: string;
  whiteLabels: number;
  class: DefinitionClass;
  configureOnly: boolean;
  standard: boolean;
  standardWithExtras: boolean;
  /** True when `prepareDefinition` threw: the definition is counted as device code. */
  threw: boolean;
  functions: AnalysedFunction[];
}

export interface AnalyseDeps {
  originOf: (text: string) => Origin;
  /** The library's `prepareDefinition`, called only to find the definitions that throw. */
  prepare: (def: RawDefinition) => unknown;
}

export function analyseDefinition(def: RawDefinition, deps: AnalyseDeps): AnalysedDefinition {
  const functions: AnalysedFunction[] = collectFunctions(def).map((c) => {
    const key = c.fn.toString();
    return {
      key,
      name: c.fn.name || "(anonymous)",
      origin: deps.originOf(key),
      configure: c.configure,
    };
  });
  const classification = classify({
    hasTuyaDatapoints: hasTuyaDatapoints(def),
    functions,
    clusters: collectClusters(def),
  });
  let threw = false;
  try {
    deps.prepare(def);
  } catch {
    threw = true;
  }
  return {
    model: String(def["model"]),
    vendor: String(def["vendor"]),
    whiteLabels: Array.isArray(def["whiteLabel"]) ? def["whiteLabel"].length : 0,
    class: threw ? "device" : classification.class,
    configureOnly: threw ? false : classification.configureOnly,
    standard: classification.standard,
    standardWithExtras: classification.standardWithExtras,
    threw,
    functions,
  };
}

export interface Percentages {
  tuyaTable: number;
  declarative: number;
  brand: number;
  device: number;
  /** Share of the device-code definitions. */
  configureOnly: number;
  exportable: number;
  standard: number;
  standardWithExtras: number;
  gap: number;
}

export interface Row {
  name: string;
  vendors: string[];
  definitions: number;
  whiteLabels: number;
  tuyaTable: number;
  declarative: number;
  brand: number;
  device: number;
  /** Device-code definitions whose device functions all come from a `configure`. */
  configureOnly: number;
  exportable: number;
  standard: number;
  standardWithExtras: number;
  /** Neither exportable nor standard nor standard with extras. */
  gap: number;
  /** Distinct shared functions used by the exportable definitions. */
  sharedFunctions: number;
  pct: Percentages;
}

export interface Aggregate {
  brands: Row[];
  brandsTotal: Row;
  library: Row;
  /** Distinct functions whose origin is unknown (counted as device). */
  unknownFunctions: number;
  /** Definitions that are device code only because of unknown functions. */
  unknownDeviceDefinitions: number;
  /** Models of the definitions that threw when prepared. */
  threwModels: string[];
}

export function percent(part: number, whole: number): number {
  return whole === 0 ? 0 : Math.round((100 * part) / whole);
}

function buildRow(
  name: string,
  records: readonly AnalysedDefinition[],
  commonKeys: ReadonlySet<string>,
): Row {
  const count = (pred: (r: AnalysedDefinition) => boolean): number => records.filter(pred).length;
  const definitions = records.length;
  const tuyaTable = count((r) => r.class === "tuya-table");
  const declarative = count((r) => r.class === "declarative");
  const brand = count((r) => r.class === "brand");
  const device = count((r) => r.class === "device");
  const configureOnly = count((r) => r.configureOnly);
  const exportable = count((r) => isExportable(r.class));
  const standard = count((r) => r.standard);
  const standardWithExtras = count((r) => r.standardWithExtras);
  const gap = count((r) => !isExportable(r.class) && !r.standard && !r.standardWithExtras);
  const shared = new Set<string>();
  for (const r of records) {
    if (!isExportable(r.class)) continue;
    for (const f of r.functions) {
      if (f.origin === "shared" && !commonKeys.has(f.key)) shared.add(f.key);
    }
  }
  return {
    name,
    vendors: [...new Set(records.map((r) => r.vendor))].sort(),
    definitions,
    whiteLabels: records.reduce((sum, r) => sum + r.whiteLabels, 0),
    tuyaTable,
    declarative,
    brand,
    device,
    configureOnly,
    exportable,
    standard,
    standardWithExtras,
    gap,
    sharedFunctions: shared.size,
    pct: {
      tuyaTable: percent(tuyaTable, definitions),
      declarative: percent(declarative, definitions),
      brand: percent(brand, definitions),
      device: percent(device, definitions),
      configureOnly: percent(configureOnly, device),
      exportable: percent(exportable, definitions),
      standard: percent(standard, definitions),
      standardWithExtras: percent(standardWithExtras, definitions),
      gap: percent(gap, definitions),
    },
  };
}

/**
 * @param commonKeys source texts of the converters `prepareDefinition` adds to every definition:
 *   left out of the shared-function count.
 */
export function aggregate(
  records: readonly AnalysedDefinition[],
  brands: readonly Brand[],
  commonKeys: ReadonlySet<string> = new Set(),
): Aggregate {
  const vendorToBrand = new Map<string, string>();
  for (const b of brands) for (const v of b.vendors) vendorToBrand.set(v.toLowerCase(), b.name);

  const perBrand = new Map<string, AnalysedDefinition[]>(brands.map((b) => [b.name, []]));
  const inBrands: AnalysedDefinition[] = [];
  for (const r of records) {
    const name = vendorToBrand.get(r.vendor.toLowerCase());
    if (name === undefined) continue;
    perBrand.get(name)?.push(r);
    inBrands.push(r);
  }

  const rows = [...perBrand.entries()].map(([name, rs]) => buildRow(name, rs, commonKeys));
  rows.sort((a, b) => b.definitions - a.definitions || a.name.localeCompare(b.name));

  const unknown = new Set<string>();
  for (const r of records)
    for (const f of r.functions) if (f.origin === "unknown") unknown.add(f.key);

  return {
    brands: rows,
    brandsTotal: buildRow("Leading brands", inBrands, commonKeys),
    library: buildRow("Whole library", records, commonKeys),
    unknownFunctions: unknown.size,
    unknownDeviceDefinitions: records.filter(
      (r) =>
        !r.threw &&
        r.class === "device" &&
        r.functions.some((f) => f.origin === "unknown") &&
        !r.functions.some((f) => f.origin === "device"),
    ).length,
    threwModels: records.filter((r) => r.threw).map((r) => `${r.vendor} ${r.model}`),
  };
}
