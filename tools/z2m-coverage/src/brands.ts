// The brands measured, the library files that count as brand code, and the devices of interest.

export interface Brand {
  name: string;
  /** Vendor names as the library writes them, matched without regard to case. */
  vendors: string[];
}

export const BRANDS: readonly Brand[] = [
  { name: "Philips (Hue)", vendors: ["Philips"] },
  { name: "IKEA", vendors: ["IKEA"] },
  { name: "Aqara (with Xiaomi)", vendors: ["Aqara", "Xiaomi"] },
  { name: "Tuya", vendors: ["Tuya"] },
  { name: "Moes", vendors: ["Moes"] },
  { name: "Zemismart", vendors: ["Zemismart"] },
  { name: "Lidl", vendors: ["Lidl"] },
  { name: "Sonoff", vendors: ["Sonoff"] },
  { name: "Schneider Electric", vendors: ["Schneider Electric"] },
  { name: "Legrand", vendors: ["Legrand"] },
  { name: "Innr", vendors: ["Innr"] },
  { name: "Ledvance", vendors: ["Ledvance"] },
  { name: "OSRAM", vendors: ["OSRAM"] },
  { name: "Gledopto", vendors: ["Gledopto"] },
  { name: "Heiman", vendors: ["Heiman"] },
  { name: "Lexman (ADEO)", vendors: ["ADEO", "Lexman"] },
  { name: "Third Reality", vendors: ["Third Reality", "Third Reality, Inc"] },
  { name: "Develco (frient)", vendors: ["Develco", "Frient"] },
  { name: "Bosch", vendors: ["Bosch"] },
  { name: "NodOn", vendors: ["NodOn"] },
  { name: "Somfy", vendors: ["Somfy"] },
  { name: "Danfoss", vendors: ["Danfoss"] },
  { name: "Hager", vendors: ["Hager"] },
  { name: "Niko", vendors: ["Niko"] },
  { name: "Ubisys", vendors: ["Ubisys"] },
  { name: "Profalux", vendors: ["Profalux"] },
];

/**
 * Files of the library's `dist/lib/` written for one manufacturer (brand code). `legacy.js` counts
 * as brand code (the cautious side). `tuya.js` is not listed: it is the shared Tuya datapoint
 * interpreter, used by hundreds of white labels. Other `lib/` files (`modernExtend.js`,
 * `utils.js`, `light.js`...) are shared.
 */
export const BRAND_FILES: readonly string[] = [
  "lib/adurosmart.js",
  "lib/bosch.js",
  "lib/develco.js",
  "lib/ewelink.js",
  "lib/heiman.js",
  "lib/heimgard.js",
  "lib/ikea.js",
  "lib/inovelli.js",
  "lib/ledvance.js",
  "lib/legacy.js",
  "lib/legrand.js",
  "lib/lidl.js",
  "lib/lumi.js",
  "lib/namron.js",
  "lib/nodon.js",
  "lib/philips.js",
  "lib/sonoff.js",
  "lib/sunricher.js",
  "lib/ubisys.js",
  "lib/zosung.js",
];

export interface DeviceOfInterest {
  vendor: string;
  model: string;
  why: string;
}

export const DEVICES_OF_INTEREST: readonly DeviceOfInterest[] = [
  {
    vendor: "SONOFF",
    model: "SNZB-02",
    why: "temperature and humidity sensor on standard clusters, with a converter written in its device file",
  },
  { vendor: "IKEA", model: "KAJPLATS_WS", why: "a light built from modern extensions only" },
  { vendor: "IKEA", model: "LED1545G12", why: "a bulb that uses code of the IKEA library file" },
  {
    vendor: "Philips",
    model: "929004440102",
    why: "a Hue light that uses code of the Philips library file",
  },
  {
    vendor: "Tuya",
    model: "TS0601_dimmer_1_gang_3",
    why: "a Tuya device described by a datapoint table",
  },
  {
    vendor: "Aqara",
    model: "SJCGQ13LM",
    why: "device code only in configure (binding and reporting)",
  },
  { vendor: "Legrand", model: "067755", why: "device code that reads messages" },
];
