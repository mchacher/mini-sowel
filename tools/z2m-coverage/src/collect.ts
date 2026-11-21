// The functions of a definition, collected from the RAW definition and its `extend[]` entries, the
// way the library's `processExtensions` reads them (the prepared definition wraps `configure`,
// `onEvent` and functional `exposes` in new closures that cannot be traced back).

export type AnyFunction = (...args: any[]) => any;
export type RawDefinition = Record<string, any>;

export interface CollectedFunction {
  fn: AnyFunction;
  /** True when every place the function was found in is a `configure`. */
  configure: boolean;
}

const MAX_DEPTH = 8;

function sources(def: RawDefinition): RawDefinition[] {
  const extend: unknown = def["extend"];
  return [def, ...(Array.isArray(extend) ? extend.filter((e) => e && typeof e === "object") : [])];
}

function toArray(value: unknown): unknown[] {
  if (value === undefined || value === null) return [];
  return Array.isArray(value) ? value : [value];
}

/** Functions found in a value, searched into nested objects and arrays. */
function nestedFunctions(
  value: unknown,
  seen: WeakSet<object>,
  depth: number,
  out: AnyFunction[],
): void {
  if (typeof value === "function") {
    out.push(value as AnyFunction);
    return;
  }
  if (value === null || typeof value !== "object" || depth > MAX_DEPTH || seen.has(value)) return;
  seen.add(value);
  for (const inner of Object.values(value)) nestedFunctions(inner, seen, depth + 1, out);
}

export function collectFunctions(def: RawDefinition): CollectedFunction[] {
  const found = new Map<AnyFunction, CollectedFunction>();
  const add = (fn: unknown, configure: boolean): void => {
    if (typeof fn !== "function") return;
    const known = found.get(fn as AnyFunction);
    if (known) known.configure = known.configure && configure;
    else found.set(fn as AnyFunction, { fn: fn as AnyFunction, configure });
  };
  const seen = new WeakSet<object>();
  for (const source of sources(def)) {
    for (const converter of toArray(source["fromZigbee"])) add((converter as any)?.convert, false);
    for (const converter of toArray(source["toZigbee"])) {
      add((converter as any)?.convertSet, false);
      add((converter as any)?.convertGet, false);
    }
    for (const fn of toArray(source["configure"])) add(fn, true);
    for (const fn of toArray(source["onEvent"])) add(fn, false);
    for (const fn of toArray(source["exposes"])) add(fn, false);
    const metaFunctions: AnyFunction[] = [];
    nestedFunctions(source["meta"], seen, 0, metaFunctions);
    for (const fn of metaFunctions) add(fn, false);
  }
  return [...found.values()];
}

/** The `cluster` of every fromZigbee converter of the definition and its extensions. */
export function collectClusters(def: RawDefinition): (string | undefined)[] {
  return sources(def).flatMap((source) =>
    toArray(source["fromZigbee"]).map((c) => {
      const cluster = (c as any)?.cluster;
      return typeof cluster === "string" ? cluster : undefined;
    }),
  );
}

/** True when the definition or one of its extensions has `meta.tuyaDatapoints`. */
export function hasTuyaDatapoints(def: RawDefinition): boolean {
  return sources(def).some((source) => source["meta"]?.tuyaDatapoints !== undefined);
}
