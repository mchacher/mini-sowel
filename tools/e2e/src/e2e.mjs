// End-to-end test of the simulated path (spec 019, J1 and J2). Started by scripts/e2e-sim.sh, which
// runs the host and gives its address in E2E_URL. Drives the interface in a headless Chromium (the
// one in Playwright's cache; nothing is downloaded), takes a screenshot after each step in
// E2E_SHOTS, and exits 1 on the first failed check. Every wait is on a condition.
import { chromium } from "playwright";
import { mkdirSync } from "node:fs";
import { join } from "node:path";

const base = process.env.E2E_URL;
const shots = process.env.E2E_SHOTS;
if (!base || !shots) {
  console.error("e2e: E2E_URL and E2E_SHOTS are required (run sh scripts/e2e-sim.sh)");
  process.exit(2);
}
mkdirSync(shots, { recursive: true });

const WAIT = 30_000;
let stepNumber = 0;

function check(condition, message) {
  if (!condition) throw new Error(message);
}

async function api(path) {
  const res = await fetch(`${base}/api/v1${path}`);
  check(res.ok, `GET ${path} answered ${res.status}`);
  return res.json();
}

/** Polls `read` until it returns something truthy (at most WAIT ms), then returns it. */
async function until(what, read) {
  const end = Date.now() + WAIT;
  for (;;) {
    const value = await read();
    if (value) return value;
    check(Date.now() < end, `timeout waiting for ${what}`);
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
}

/** The device whose model is `model`, once its data (a key of `dataKey`) are set up. */
async function deviceWithData(model, dataKey) {
  return until(`${model} to report ${dataKey}`, async () => {
    const list = await api("/devices");
    const found = list.find((d) => d.model === model);
    if (!found) return null;
    const full = await api(`/devices/${found.id}`);
    return JSON.stringify(full).includes(`"${dataKey}"`) ? full : null;
  });
}

const browser = await chromium.launch({ headless: true });
let page;
try {
  const context = await browser.newContext({
    locale: "en-US",
    viewport: { width: 1280, height: 900 },
  });
  page = await context.newPage();
  page.setDefaultTimeout(WAIT);
  page.on("dialog", (dialog) => void dialog.accept());

  const step = async (name, run) => {
    stepNumber += 1;
    const label = `${String(stepNumber).padStart(2, "0")}-${name}`;
    try {
      await run();
    } catch (error) {
      await page.screenshot({ path: join(shots, `${label}-FAILED.png`) }).catch(() => {});
      error.message = `step ${stepNumber} (${name}): ${error.message}`;
      throw error;
    }
    await page.screenshot({ path: join(shots, `${label}.png`) });
  };

  const row = (name) => page.locator("tr", { hasText: name });
  const modal = () => page.locator(".fixed").filter({ has: page.locator("#equipment-name") });

  await step("open-topology", async () => {
    await page.goto(`${base}/#/zones`);
    await page.getByRole("button", { name: "Add zone" }).waitFor();
  });

  await step("add-zone-garage", async () => {
    await page.getByRole("button", { name: "Add zone" }).click();
    await page.locator("#zone-name").fill("Garage");
    await page.getByRole("button", { name: "Create", exact: true }).click();
    await page.getByText("Garage", { exact: true }).first().waitFor();
  });

  await step("discovery-add-a-device", async () => {
    await page.goto(`${base}/#/discovery`);
    await page.getByRole("button", { name: "Add a device" }).click();
    // The three simulated devices join within 8 simulated seconds (speed 10).
    for (const model of ["ZBMINIR2", "TH01", "MS01"]) {
      await row(model).waitFor();
      await row(model).getByText("New", { exact: true }).waitFor();
    }
    check((await page.locator("tbody tr").count()) === 3, "Discovery does not list 3 devices");
  });

  await step("create-light", async () => {
    // The form guesses the type from the device's data: wait until they are set up.
    await deviceWithData("ZBMINIR2", "state");
    await row("ZBMINIR2").getByRole("button", { name: "Create equipment" }).click();
    await page.locator("#equipment-name").waitFor();
    check(
      (await page.locator("#equipment-type").inputValue()) === "light_onoff",
      "the type is not guessed as light on/off",
    );
    await page.locator("#equipment-name").fill("Lumière garage");
    await page.locator("#equipment-zone").selectOption({ label: "Garage" });
    await page.getByRole("button", { name: "Next: Select devices" }).click();
    const create = page.getByRole("button", { name: "Create", exact: true });
    await page.waitForFunction(() =>
      [...document.querySelectorAll("button")].some(
        (b) => b.textContent === "Create" && !b.disabled,
      ),
    );
    await create.click();
    await row("ZBMINIR2").getByText("Lumière garage").waitFor();
  });

  await step("create-sensor", async () => {
    await deviceWithData("TH01", "temperature");
    await deviceWithData("MS01", "occupancy");
    await row("TH01").getByRole("button", { name: "Create equipment" }).click();
    await page.locator("#equipment-name").waitFor();
    check(
      (await page.locator("#equipment-type").inputValue()) === "sensor",
      "the type is not guessed as sensor",
    );
    await page.locator("#equipment-name").fill("Capteurs garage");
    await page.locator("#equipment-zone").selectOption({ label: "Garage" });
    await page.getByRole("button", { name: "Next: Select devices" }).click();
    const ms01 = page.getByRole("checkbox", { name: /MS01/ });
    await ms01.waitFor();
    if (!(await ms01.isChecked())) await ms01.check();
    await page.waitForFunction(() =>
      [...document.querySelectorAll("button")].some(
        (b) => b.textContent === "Create" && !b.disabled,
      ),
    );
    await page.getByRole("button", { name: "Create", exact: true }).click();
    await row("TH01").getByText("Capteurs garage").waitFor();
    await row("MS01").getByText("Capteurs garage").waitFor();
  });

  await step("devices-page", async () => {
    await page.goto(`${base}/#/devices`);
    for (const [model, name] of [
      ["ZBMINIR2", "Lumière garage"],
      ["TH01", "Capteurs garage"],
      ["MS01", "Capteurs garage"],
    ]) {
      const cell = row(model);
      await cell.waitFor();
      const text = (await cell.innerText()).replace(/\s*›\s*/g, " › ").replace(/\s+/g, " ");
      check(
        text.includes(`Garage › ${name}`),
        `Devices: ${model} does not read "Garage › ${name}"`,
      );
    }
  });

  const toggle = (title) => page.locator(`button[title="${title}"]`);

  await step("garage-light-on", async () => {
    await page.goto(`${base}/#/devices`);
    await page
      .locator("aside a, nav a")
      .filter({ hasText: /^Garage$/ })
      .first()
      .click();
    await page.getByText("Lumière garage").first().waitFor();
    await toggle("Turn on").click();
    await toggle("Turn off").waitFor();
  });

  await step("garage-light-off", async () => {
    await toggle("Turn off").click();
    await toggle("Turn on").waitFor();
  });

  let equipments;
  await step("api-equipments", async () => {
    equipments = await api("/equipments");
    check(equipments.length === 2, `the API lists ${equipments.length} equipments, not 2`);
    const light = equipments.find((e) => e.name === "Lumière garage");
    const sensor = equipments.find((e) => e.name === "Capteurs garage");
    check(light && sensor, "the API lacks one of the two equipments");
    const zones = await api("/zones");
    const flat = (nodes) => nodes.flatMap((z) => [z, ...flat(z.children ?? [])]);
    const garage = flat(zones).find((z) => z.name === "Garage");
    check(garage, "the API has no zone Garage");
    check(light.zoneId === garage.id && sensor.zoneId === garage.id, "equipments not in Garage");
    check(light.type === "light_onoff", `light type is ${light.type}`);
    check(sensor.type === "sensor", `sensor type is ${sensor.type}`);
    const lightData = light.dataBindings.map((b) => b.alias);
    const lightOrders = light.orderBindings.map((b) => b.alias);
    check(
      lightData.length === 1 && lightData[0] === "state",
      `light data bindings: ${lightData.join(",")}`,
    );
    check(
      lightOrders.length === 1 && lightOrders[0] === "state",
      `light order bindings: ${lightOrders.join(",")}`,
    );
    const aliases = sensor.dataBindings.map((b) => b.alias);
    for (const wanted of ["temperature", "humidity", "battery", "occupancy"]) {
      check(
        aliases.includes(wanted),
        `sensor lacks a binding "${wanted}" (has ${aliases.join(",")})`,
      );
    }
    check(new Set(aliases).size === aliases.length, "sensor has duplicate aliases");
  });

  await step("reload-keeps-everything", async () => {
    await page.goto(`${base}/#/devices`);
    await page.reload();
    for (const [model, name] of [
      ["ZBMINIR2", "Lumière garage"],
      ["TH01", "Capteurs garage"],
      ["MS01", "Capteurs garage"],
    ]) {
      await row(model).getByText(name).waitFor();
    }
    const again = await api("/equipments");
    check(again.length === 2, "the equipments changed after the reload");
  });
  void modal;
} catch (error) {
  console.error(`e2e: ${error.message}`);
  process.exitCode = 1;
} finally {
  await browser.close();
}
