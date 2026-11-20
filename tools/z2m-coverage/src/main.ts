// Loads the pinned library, measures every definition, writes the report and prints the totals.
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { aggregate, analyseDefinition } from "./aggregate.ts";
import type { AnalysedDefinition } from "./aggregate.ts";
import { BRANDS, BRAND_FILES, DEVICES_OF_INTEREST } from "./brands.ts";
import type { RawDefinition } from "./collect.ts";
import { createOriginLookup } from "./origin.ts";
import type { SourceFile } from "./origin.ts";
import { renderReport, totalsLine } from "./report.ts";
import type { InterestRow } from "./report.ts";

export interface Library {
  version: string;
  definitions: RawDefinition[];
  sources: SourceFile[];
  prepare: (def: RawDefinition) => unknown;
}

export interface RunOptions {
  loadLibrary: () => Promise<Library>;
  /** Receives the finished report; nothing is written when the run fails. */
  write: (report: string) => void;
  log: (line: string) => void;
  error: (line: string) => void;
  today: string;
}

export interface RunResult {
  code: number;
  report?: string;
  records?: AnalysedDefinition[];
  /** Distinct source texts of the converters added to every definition. */
  commonFunctionKeys?: number;
}

export async function run(options: RunOptions): Promise<RunResult> {
  let library: Library;
  try {
    library = await options.loadLibrary();
  } catch (e) {
    options.error(
      `z2m-coverage: cannot load the library: ${e instanceof Error ? e.message : String(e)}`,
    );
    return { code: 1 };
  }
  if (library.definitions.length === 0) {
    options.error("z2m-coverage: the library loaded no definition.");
    return { code: 1 };
  }

  const originOf = createOriginLookup(library.sources, BRAND_FILES);
  const deps = { originOf, prepare: library.prepare };
  const records = library.definitions.map((def) => analyseDefinition(def, deps));

  // The converters `prepareDefinition` adds to every definition: found by preparing an empty one.
  const empty = { model: "empty", vendor: "empty", exposes: [], fromZigbee: [], toZigbee: [] };
  const commonKeys = new Set<string>();
  const commonConverters: string[] = [];
  try {
    const prepared = library.prepare(empty) as {
      toZigbee?: { key?: string[]; convertSet?: object; convertGet?: object }[];
    };
    for (const c of prepared.toZigbee ?? []) {
      commonConverters.push((c.key ?? ["(unnamed)"]).join("/"));
      if (c.convertSet) commonKeys.add(c.convertSet.toString());
      if (c.convertGet) commonKeys.add(c.convertGet.toString());
    }
  } catch {
    // Without them the shared-function counts only include them; the report says nothing less.
  }

  const result = aggregate(records, BRANDS, commonKeys);
  const interest: InterestRow[] = DEVICES_OF_INTEREST.map((d) => ({
    vendor: d.vendor,
    model: d.model,
    why: d.why,
    record: records.find((r) => r.vendor === d.vendor && r.model === d.model),
  }));
  const report = renderReport({
    libraryVersion: library.version,
    date: options.today,
    definitionsLoaded: library.definitions.length,
    aggregate: result,
    brandFiles: BRAND_FILES,
    commonConverters,
    interest,
  });
  options.write(report);
  options.log(totalsLine(result, library.version));
  return { code: 0, report, records, commonFunctionKeys: commonKeys.size };
}

function listJsFiles(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return listJsFiles(path);
    return entry.name.endsWith(".js") ? [path] : [];
  });
}

/** The real library: its definitions, its prepareDefinition, and its dist/**\/*.js as text. */
export async function loadRealLibrary(): Promise<Library> {
  const require = createRequire(import.meta.url);
  const packagePath = require.resolve("zigbee-herdsman-converters/package.json");
  const dist = join(dirname(packagePath), "dist");
  const version = (JSON.parse(readFileSync(packagePath, "utf8")) as { version: string }).version;
  // The library is CommonJS: the module's `default` is its `module.exports`.
  const devices = (await import("zigbee-herdsman-converters/devices/index")) as {
    default: { default?: RawDefinition[] };
  };
  const main = (await import("zigbee-herdsman-converters")) as unknown as {
    default?: { prepareDefinition?: Library["prepare"] };
    prepareDefinition?: Library["prepare"];
  };
  const prepare = main.prepareDefinition ?? main.default?.prepareDefinition;
  if (!prepare) throw new Error("prepareDefinition not found in the library");
  const sources = listJsFiles(dist).map((path) => ({
    file: relative(dist, path).split(sep).join("/"),
    text: readFileSync(path, "utf8"),
  }));
  return { version, definitions: devices.default.default ?? [], sources, prepare };
}

if (import.meta.main) {
  const reportPath = join(dirname(fileURLToPath(import.meta.url)), "..", "report.md");
  const { code } = await run({
    loadLibrary: loadRealLibrary,
    write: (report) => writeFileSync(reportPath, report),
    log: (line) => console.log(line),
    error: (line) => console.error(line),
    today: new Date().toISOString().slice(0, 10),
  });
  process.exitCode = code;
}
