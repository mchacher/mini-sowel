import assert from "node:assert/strict";
import { test } from "node:test";
import { aggregate, analyseDefinition } from "../src/aggregate.ts";
import type { AnalysedDefinition, AnalysedFunction } from "../src/aggregate.ts";
import type { Brand } from "../src/brands.ts";

const brands: Brand[] = [
  { name: "Alpha", vendors: ["Alpha", "Alpha Labs"] },
  { name: "Beta", vendors: ["Beta"] },
  { name: "Gamma", vendors: ["Gamma"] },
  { name: "Sonoff", vendors: ["Sonoff"] },
];

const shared = (key: string): AnalysedFunction => ({
  key,
  name: key,
  origin: "shared",
  configure: false,
});

function record(over: Partial<AnalysedDefinition> = {}): AnalysedDefinition {
  return {
    model: "M1",
    vendor: "Alpha",
    whiteLabels: 0,
    class: "declarative",
    configureOnly: false,
    standard: true,
    standardWithExtras: false,
    threw: false,
    functions: [],
    ...over,
  };
}

const row = (records: AnalysedDefinition[], name: string, commonKeys = new Set<string>()) => {
  const found = aggregate(records, brands, commonKeys).brands.find((r) => r.name === name);
  assert.ok(found);
  return found;
};

test("aggregate: definitions of two vendors of one brand make one row with counts added", () => {
  const a = row(
    [
      record({ vendor: "Alpha" }),
      record({ vendor: "Alpha Labs", class: "device", standard: false }),
    ],
    "Alpha",
  );
  assert.equal(a.definitions, 2);
  assert.equal(a.declarative, 1);
  assert.equal(a.device, 1);
  assert.deepEqual(a.vendors, ["Alpha", "Alpha Labs"]);
});

test("aggregate: the vendor SONOFF is matched to the brand Sonoff", () => {
  assert.equal(row([record({ vendor: "SONOFF" })], "Sonoff").definitions, 1);
});

test("aggregate: two brands with the same number of definitions are sorted by name", () => {
  const result = aggregate(
    [record({ vendor: "Beta" }), record({ vendor: "Alpha" }), record({ vendor: "Alpha" })],
    brands,
  );
  assert.deepEqual(
    result.brands.map((r) => r.name),
    ["Alpha", "Beta", "Gamma", "Sonoff"],
  );
  const tied = aggregate([record({ vendor: "Gamma" }), record({ vendor: "Beta" })], brands);
  assert.deepEqual(
    tied.brands.slice(0, 2).map((r) => r.name),
    ["Beta", "Gamma"],
  );
});

test("aggregate: converters added to every definition are left out of the shared-function count", () => {
  const records = [record({ functions: [shared("scene_store"), shared("light_onoff")] })];
  assert.equal(row(records, "Alpha").sharedFunctions, 2);
  assert.equal(row(records, "Alpha", new Set(["scene_store"])).sharedFunctions, 1);
});

test("aggregate: a brand with no definition has a row with 0 and no NaN", () => {
  const g = row([record()], "Gamma");
  assert.equal(g.definitions, 0);
  assert.equal(g.sharedFunctions, 0);
  assert.deepEqual(Object.values(g.pct), [0, 0, 0, 0, 0, 0, 0, 0, 0]);
});

test("aggregate: white labels are counted in the column, not as definitions", () => {
  const a = row([record({ whiteLabels: 3 }), record({ whiteLabels: 2 })], "Alpha");
  assert.equal(a.definitions, 2);
  assert.equal(a.whiteLabels, 5);
});

test("aggregate: a definition of a vendor of no brand is in the library total only", () => {
  const result = aggregate([record({ vendor: "Nobody" }), record({ vendor: "Alpha" })], brands);
  assert.equal(result.library.definitions, 2);
  assert.equal(result.brandsTotal.definitions, 1);
  assert.equal(
    result.brands.reduce((n, r) => n + r.definitions, 0),
    1,
  );
});

test("aggregate: the same shared function used twice counts once", () => {
  const records = [
    record({ functions: [shared("f"), shared("g")] }),
    record({ functions: [shared("f")] }),
    record({ class: "device", functions: [shared("h")] }),
  ];
  assert.equal(row(records, "Alpha").sharedFunctions, 2);
});

test("aggregate: a definition that threw when prepared is device code and its model is listed", () => {
  const def = { model: "BROKEN", vendor: "Alpha", fromZigbee: [], toZigbee: [] };
  const analysed = analyseDefinition(def, {
    originOf: () => "shared",
    prepare: () => {
      throw new Error("bad definition");
    },
  });
  assert.equal(analysed.class, "device");
  const result = aggregate([analysed], brands);
  assert.equal(result.brands.find((r) => r.name === "Alpha")?.device, 1);
  assert.deepEqual(result.threwModels, ["Alpha BROKEN"]);
});

test("aggregate: unknown functions are counted, with the definitions device code only because of them", () => {
  const unknown = (key: string): AnalysedFunction => ({
    key,
    name: key,
    origin: "unknown",
    configure: false,
  });
  const device: AnalysedFunction = { key: "d", name: "d", origin: "device", configure: false };
  const result = aggregate(
    [
      record({ class: "device", functions: [unknown("u1")] }),
      record({ class: "device", functions: [unknown("u1"), unknown("u2")] }),
      record({ class: "device", functions: [unknown("u1"), device] }),
      record({ functions: [shared("s")] }),
    ],
    brands,
  );
  assert.equal(result.unknownFunctions, 2);
  assert.equal(result.unknownDeviceDefinitions, 2);
});
