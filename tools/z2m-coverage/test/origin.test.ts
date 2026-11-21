import assert from "node:assert/strict";
import { test } from "node:test";
import { createOriginLookup } from "../src/origin.ts";

const BRAND_FILES = ["lib/ikea.js", "lib/legacy.js"];
const fn = "(model, msg) => { return msg.data.value * 2; }";
const other = "(model, msg) => { return msg.data.other; }";
const lookup = (files: { file: string; text: string }[], text = fn) =>
  createOriginLookup(files, BRAND_FILES)(text);

test("origin: text in a common converters file is shared", () => {
  assert.equal(lookup([{ file: "converters/fromZigbee.js", text: `x = ${fn};` }]), "shared");
});

test("origin: text in a brand file is brand", () => {
  assert.equal(lookup([{ file: "lib/ikea.js", text: `x = ${fn};` }]), "brand");
});

test("origin: text in lib/tuya.js is shared", () => {
  assert.equal(lookup([{ file: "lib/tuya.js", text: `x = ${fn};` }]), "shared");
});

test("origin: text in a device file is device", () => {
  assert.equal(lookup([{ file: "devices/sonoff.js", text: `x = ${fn};` }]), "device");
});

test("origin: text found nowhere is unknown", () => {
  assert.equal(lookup([{ file: "devices/sonoff.js", text: `x = ${other};` }]), "unknown");
});

test("origin: text in a shared file and a device file is shared", () => {
  const files = [
    { file: "lib/utils.js", text: fn },
    { file: "devices/sonoff.js", text: fn },
  ];
  assert.equal(lookup(files), "shared");
});

test("origin: text in a brand file and a device file is brand", () => {
  const files = [
    { file: "lib/ikea.js", text: fn },
    { file: "devices/ikea.js", text: fn },
  ];
  assert.equal(lookup(files), "brand");
});

test("origin: the same text in two device files stays device", () => {
  const files = [
    { file: "devices/sonoff.js", text: fn },
    { file: "devices/tuya.js", text: fn },
  ];
  assert.equal(lookup(files), "device");
});

test("origin: text in lib/legacy.js is brand", () => {
  assert.equal(lookup([{ file: "lib/legacy.js", text: fn }]), "brand");
});

test("origin: text in index.js is shared", () => {
  assert.equal(lookup([{ file: "index.js", text: fn }]), "shared");
});
