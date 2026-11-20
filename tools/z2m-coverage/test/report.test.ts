import assert from "node:assert/strict";
import { test } from "node:test";
import { aggregate } from "../src/aggregate.ts";
import type { AnalysedDefinition } from "../src/aggregate.ts";
import type { Brand } from "../src/brands.ts";
import { renderReport } from "../src/report.ts";

const brands: Brand[] = [
  { name: "Small", vendors: ["Small"] },
  { name: "Big", vendors: ["Big"] },
  { name: "Absent", vendors: ["Absent"] },
];

const rec = (over: Partial<AnalysedDefinition>): AnalysedDefinition => ({
  model: "M",
  vendor: "Big",
  whiteLabels: 0,
  class: "declarative",
  configureOnly: false,
  standard: true,
  standardWithExtras: false,
  threw: false,
  functions: [{ key: "f", name: "f", origin: "shared", configure: false }],
  ...over,
});

test("report: a known aggregate renders header, rows sorted by size, totals, devices of interest and limits", () => {
  const records = [
    rec({ vendor: "Small", model: "S1" }),
    rec({ model: "B1" }),
    rec({ model: "B2", class: "device", standard: false }),
  ];
  const text = renderReport({
    libraryVersion: "1.2.3",
    date: "2030-01-02",
    definitionsLoaded: 3,
    aggregate: aggregate(records, brands),
    brandFiles: ["lib/ikea.js"],
    commonConverters: ["scene_store", "read"],
    interest: [
      { vendor: "Big", model: "B2", why: "a device", record: records[2] },
      { vendor: "Big", model: "NOPE", why: "missing", record: undefined },
    ],
  });
  assert.match(text, /1\.2\.3/);
  assert.match(text, /2030-01-02/);
  assert.match(text, /Definitions loaded: 3/);
  const at = (s: string) => text.indexOf(`| ${s} |`);
  assert.ok(at("Big") > 0 && at("Big") < at("Small") && at("Small") < at("Absent"));
  assert.ok(at("Absent") < at("Leading brands") && at("Leading brands") < at("Whole library"));
  assert.match(text, /\| Big \| 2 \| 0 \| 0 \(0%\) \| 1 \(50%\) \|/);
  assert.match(text, /## Devices of interest/);
  assert.match(text, /\| B2 \| Big \| Device code \| not standard \|/);
  assert.match(text, /\| NOPE \| Big \| not in the library/);
  assert.match(text, /\(2 of them: scene_store, read\)/);
  assert.match(text, /## Method and limits/);
  assert.match(text, /`lib\/ikea\.js`/);
  assert.match(text, /Brands with no definition in the library \(row with 0\): Absent/);
});
