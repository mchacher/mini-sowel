import assert from "node:assert/strict";
import { test } from "node:test";
import { classify } from "../src/classify.ts";
import type { ClassifyInput } from "../src/classify.ts";
import type { Origin } from "../src/origin.ts";

const fn = (origin: Origin, configure = false) => ({ origin, configure });
const input = (over: Partial<ClassifyInput>): ClassifyInput => ({
  hasTuyaDatapoints: false,
  functions: [],
  clusters: ["genOnOff"],
  ...over,
});

test("classify: Tuya datapoints with shared functions only is a Tuya table", () => {
  const r = classify(input({ hasTuyaDatapoints: true, functions: [fn("shared"), fn("shared")] }));
  assert.equal(r.class, "tuya-table");
});

test("classify: Tuya datapoints with a device function is device code", () => {
  const r = classify(input({ hasTuyaDatapoints: true, functions: [fn("shared"), fn("device")] }));
  assert.equal(r.class, "device");
});

test("classify: shared functions only is declarative", () => {
  assert.equal(classify(input({ functions: [fn("shared")] })).class, "declarative");
});

test("classify: no function at all is declarative", () => {
  assert.equal(classify(input({ functions: [] })).class, "declarative");
});

test("classify: shared and brand functions is brand code", () => {
  assert.equal(classify(input({ functions: [fn("shared"), fn("brand")] })).class, "brand");
});

test("classify: a brand and a device function is device code", () => {
  assert.equal(classify(input({ functions: [fn("brand"), fn("device")] })).class, "device");
});

test("classify: an unknown function is device code", () => {
  assert.equal(classify(input({ functions: [fn("shared"), fn("unknown")] })).class, "device");
});

test("classify: device functions only from configure are device code in configure only", () => {
  const r = classify(input({ functions: [fn("shared"), fn("device", true), fn("unknown", true)] }));
  assert.equal(r.class, "device");
  assert.equal(r.configureOnly, true);
});

test("classify: a device function in a converter is not configure only", () => {
  const r = classify(input({ functions: [fn("device", true), fn("device", false)] }));
  assert.equal(r.class, "device");
  assert.equal(r.configureOnly, false);
});

test("classify: fromZigbee clusters all standard is standard", () => {
  const clusters = ["genOnOff", "msTemperatureMeasurement", "haElectricalMeasurement"];
  const r = classify(input({ clusters }));
  assert.equal(r.standard, true);
  assert.equal(r.standardWithExtras, false);
});

test("classify: one fromZigbee cluster that is not standard is not standard", () => {
  assert.equal(classify(input({ clusters: ["genOnOff", "manuSpecificTuya"] })).standard, false);
});

test("classify: no fromZigbee converter is not standard", () => {
  assert.equal(classify(input({ clusters: [] })).standard, false);
});

test("classify: standard clusters and manuSpecificPhilips2 is standard with extras, not standard", () => {
  const r = classify(
    input({ clusters: ["genOnOff", "lightingColorCtrl", "manuSpecificPhilips2"] }),
  );
  assert.equal(r.standard, false);
  assert.equal(r.standardWithExtras, true);
});

test("classify: only manuSpecificTuya is neither standard nor standard with extras", () => {
  const r = classify(input({ clusters: ["manuSpecificTuya"] }));
  assert.equal(r.standard, false);
  assert.equal(r.standardWithExtras, false);
});

test("classify: a standard cluster and a non-manufacturer cluster that is not standard is neither", () => {
  const r = classify(input({ clusters: ["genOnOff", "closuresDoorLock"] }));
  assert.equal(r.standard, false);
  assert.equal(r.standardWithExtras, false);
});
