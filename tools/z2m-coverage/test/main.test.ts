import assert from "node:assert/strict";
import { test } from "node:test";
import { loadRealLibrary, run } from "../src/main.ts";
import type { RunOptions } from "../src/main.ts";

function harness(loadLibrary: RunOptions["loadLibrary"]) {
  const written: string[] = [];
  const logs: string[] = [];
  const errors: string[] = [];
  const options: RunOptions = {
    loadLibrary,
    write: (r) => written.push(r),
    log: (l) => logs.push(l),
    error: (l) => errors.push(l),
    today: "2030-01-02",
  };
  return { options, written, logs, errors };
}

// The real library is loaded and measured once for the three tests that need it.
const real = (async () => {
  const h = harness(loadRealLibrary);
  const result = await run(h.options);
  return { ...h, result };
})();

// What `prepareDefinition` adds to an empty definition, read from the library itself.
const realConverters = (async () => {
  const { report } = await real.then((r) => ({ report: r.written[0] ?? "" }));
  const library = await loadRealLibrary();
  const prepared = library.prepare({
    model: "e",
    vendor: "e",
    exposes: [],
    fromZigbee: [],
    toZigbee: [],
  }) as {
    toZigbee: unknown[];
  };
  return { report, converters: prepared.toZigbee };
})();

test("main: a library that loads nothing exits non-zero and writes no report", async () => {
  const h = harness(async () => ({
    version: "0",
    definitions: [],
    sources: [],
    prepare: () => ({}),
  }));
  const result = await run(h.options);
  assert.notEqual(result.code, 0);
  assert.equal(h.written.length, 0);
  assert.equal(h.errors.length, 1);
});

test("main: a loader that throws exits non-zero, prints the reason and writes no report", async () => {
  const h = harness(async () => {
    throw new Error("module not found: zigbee-herdsman-converters");
  });
  const result = await run(h.options);
  assert.notEqual(result.code, 0);
  assert.equal(h.written.length, 0);
  assert.match(h.errors.join("\n"), /module not found: zigbee-herdsman-converters/);
});

test("main: the real library gives a report with the SNZB-02 line and totals that add up", async () => {
  const { result, written, logs } = await real;
  assert.equal(result.code, 0);
  assert.equal(written.length, 1);
  assert.equal(logs.length, 1);
  const report = written[0] ?? "";
  assert.match(report, /\| SNZB-02 \| SONOFF \|/);
  const records = result.records ?? [];
  assert.ok(records.length > 1000);
  assert.match(report, new RegExp(`Definitions loaded: ${records.length}`));
  const count = (cls: string) => records.filter((r) => r.class === cls).length;
  assert.equal(
    count("tuya-table") + count("declarative") + count("brand") + count("device"),
    records.length,
  );
  const library = report.split("\n").find((l) => l.startsWith("| Whole library |")) ?? "";
  assert.ok(library.includes(`| ${records.length} |`));
});

test("main: the real library gives a modern-extend light of IKEA as declarative", async () => {
  const { result } = await real;
  const light = result.records?.find((r) => r.vendor === "IKEA" && r.model === "KAJPLATS_WS");
  assert.equal(light?.class, "declarative");
});

test("main: the real library gives the SNZB-02 as device code, not configure only, standard", async () => {
  const { result } = await real;
  const sensor = result.records?.find((r) => r.vendor === "SONOFF" && r.model === "SNZB-02");
  assert.equal(sensor?.class, "device");
  assert.equal(sensor?.configureOnly, false);
  assert.equal(sensor?.standard, true);
});

test("main: the converters added by prepareDefinition are found and the report count matches its names", async () => {
  const { report, converters } = await realConverters;
  assert.equal(converters.length, 11);
  assert.equal((await real).result.commonFunctionKeys, 11);
  assert.match(report, /\(11 of them: scene_store, /);
});

test("main: the real library gives a Philips Hue colour bulb as standard with extras", async () => {
  const { result } = await real;
  const bulb = result.records?.find((r) => r.vendor === "Philips" && r.model === "929003809401");
  assert.equal(bulb?.standard, false);
  assert.equal(bulb?.standardWithExtras, true);
});

test("main: the real library gives Tuya tables a share above 9% of Tuya's row", async () => {
  const { written } = await real;
  const line = (written[0] ?? "").split("\n").find((l) => l.startsWith("| Tuya |")) ?? "";
  const share = Number(/\| \d+ \((\d+)%\)/.exec(line)?.[1]);
  assert.ok(share > 9, line);
});
