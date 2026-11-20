import assert from "node:assert/strict";
import { test } from "node:test";
import { collectFunctions } from "../src/collect.ts";

const fz = () => 1;
const tz = () => 2;
const configure = () => 3;
const onEvent = () => 4;
const metaFn = () => 5;

test("collect: every function of a definition is collected once", () => {
  const def = {
    fromZigbee: [{ cluster: "genOnOff", convert: fz }],
    toZigbee: [{ key: ["state"], convertSet: tz }],
    configure,
    onEvent,
    meta: { custom: metaFn },
  };
  const fns = collectFunctions(def).map((c) => c.fn);
  assert.deepEqual(new Set(fns), new Set([fz, tz, configure, onEvent, metaFn]));
  assert.equal(fns.length, 5);
});

test("collect: functions of a modern extension come from the extension, not from a wrapper", () => {
  const c1 = () => 6;
  const c2 = () => 7;
  const e1 = () => 8;
  const e2 = () => 9;
  const wrapper = () => 10;
  const def = {
    // `wrapper` stands for the closure the library builds in prepareDefinition: it is on the
    // prepared object only, never on the raw definition.
    extend: [
      {
        isModernExtend: true,
        fromZigbee: [{ convert: fz }],
        configure: [c1, c2],
        onEvent: [e1, e2],
      },
    ],
  };
  const fns = collectFunctions(def).map((c) => c.fn);
  assert.deepEqual(new Set(fns), new Set([fz, c1, c2, e1, e2]));
  assert.ok(!fns.includes(wrapper));
});

test("collect: endpoint functions and converter options are not collected", () => {
  const endpoint = () => ({ left: 1 });
  const options = () => [];
  const def = {
    endpoint,
    fromZigbee: [{ cluster: "genOnOff", convert: fz, options }],
    extend: [{ endpoint, toZigbee: [{ convertSet: tz, options }] }],
  };
  const fns = collectFunctions(def).map((c) => c.fn);
  assert.deepEqual(new Set(fns), new Set([fz, tz]));
});

test("collect: a function from configure is marked configure", () => {
  const found = collectFunctions({ configure, fromZigbee: [{ convert: fz }] });
  assert.equal(found.find((c) => c.fn === configure)?.configure, true);
  assert.equal(found.find((c) => c.fn === fz)?.configure, false);
});

test("collect: the function of a Tuya datapoint value converter is collected", () => {
  const to = (v: unknown) => v;
  const from = (v: unknown) => v;
  const def = {
    meta: {
      tuyaDatapoints: [
        [1, "state", { to, from }],
        [2, "mode", { lookup: { a: 1 } }],
      ],
    },
  };
  const fns = collectFunctions(def).map((c) => c.fn);
  assert.deepEqual(new Set(fns), new Set([to, from]));
});

test("collect: a definition with no function gives an empty list", () => {
  assert.deepEqual(collectFunctions({ model: "router", fromZigbee: [], toZigbee: [] }), []);
});

test("collect: a toZigbee converter gives its convertSet and convertGet", () => {
  const convertSet = () => 11;
  const convertGet = () => 12;
  const fns = collectFunctions({ toZigbee: [{ key: ["state"], convertSet, convertGet }] }).map(
    (c) => c.fn,
  );
  assert.deepEqual(new Set(fns), new Set([convertSet, convertGet]));
});
