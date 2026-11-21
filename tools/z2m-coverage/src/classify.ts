// The class of a definition, from plain data: the origin of each function, the clusters it reads.
import type { Origin } from "./origin.ts";

export type DefinitionClass = "tuya-table" | "declarative" | "brand" | "device";

export const STANDARD_CLUSTERS: readonly string[] = [
  "genBasic",
  "genPowerCfg",
  "genOnOff",
  "genLevelCtrl",
  "genScenes",
  "genMultistateInput",
  "lightingColorCtrl",
  "msTemperatureMeasurement",
  "msRelativeHumidity",
  "msPressureMeasurement",
  "msIlluminanceMeasurement",
  "msOccupancySensing",
  "ssIasZone",
  "closuresWindowCovering",
  "hvacThermostat",
  "seMetering",
  "haElectricalMeasurement",
];

export interface ClassifyInput {
  hasTuyaDatapoints: boolean;
  functions: { origin: Origin; configure: boolean }[];
  /** The cluster of each fromZigbee converter (undefined when a converter names none). */
  clusters: (string | undefined)[];
}

export interface Classification {
  class: DefinitionClass;
  /** Device code whose device or unknown functions all come from a `configure`. */
  configureOnly: boolean;
  standard: boolean;
  /** Not standard, but the standard clusters are read and every other one is manufacturer-specific. */
  standardWithExtras: boolean;
}

export function classify(input: ClassifyInput): Classification {
  const { functions } = input;
  const deviceLike = functions.filter((f) => f.origin === "device" || f.origin === "unknown");
  const hasBrand = functions.some((f) => f.origin === "brand");
  const standard =
    input.clusters.length > 0 &&
    input.clusters.every((c) => c !== undefined && STANDARD_CLUSTERS.includes(c));

  const isStandard = (c: string | undefined): boolean =>
    c !== undefined && STANDARD_CLUSTERS.includes(c);
  const standardWithExtras =
    !standard &&
    input.clusters.some(isStandard) &&
    input.clusters.every((c) => isStandard(c) || (c !== undefined && c.startsWith("manuSpecific")));

  let cls: DefinitionClass;
  if (input.hasTuyaDatapoints && deviceLike.length === 0 && !hasBrand) cls = "tuya-table";
  else if (deviceLike.length > 0) cls = "device";
  else if (hasBrand) cls = "brand";
  else cls = "declarative";

  const configureOnly = cls === "device" && deviceLike.every((f) => f.configure);
  return { class: cls, configureOnly, standard, standardWithExtras };
}

/** Classes 1 and 2 of the spec: data, or calls to code shared by the whole library. */
export function isExportable(cls: DefinitionClass): boolean {
  return cls === "tuya-table" || cls === "declarative";
}
