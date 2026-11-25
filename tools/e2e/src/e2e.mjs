// End-to-end test of the simulated path (spec 019, J1 and J2). Started by scripts/e2e-sim.sh, which
// runs the host and gives its address in E2E_URL. Drives the interface in a headless Chromium (the
// one in Playwright's cache; nothing is downloaded), takes a screenshot after each step in
// E2E_SHOTS (spec 030: the equipment and device pages, in the dark and Hybrid themes), and exits 1 on the first failed check. Every wait is on a condition.
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

// The host asks for a password (spec 029): the first step sets one up in the browser, then reads
// the access token the interface stored, for the checks that go through the API.
const USER = "e2e-admin";
const PASSWORD = "e2e-synthetic-password-1"; // synthetic, for this throwaway host only
let token = null;

async function api(path) {
  // The interface renews its token (reload, log-in): always use the one it holds now.
  if (page)
    token = (await page.evaluate(() => localStorage.getItem("sowel_access_token"))) ?? token;
  const res = await fetch(`${base}/api/v1${path}`, {
    headers: token ? { Authorization: `Bearer ${token}` } : {},
  });
  check(res.ok, `GET ${path} answered ${res.status}`);
  return res.json();
}

/** POSTs `body` as JSON with the interface's token, returns the answer's JSON. */
async function apiPost(path, body) {
  if (page)
    token = (await page.evaluate(() => localStorage.getItem("sowel_access_token"))) ?? token;
  const res = await fetch(`${base}/api/v1${path}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
    },
    body: JSON.stringify(body),
  });
  check(res.ok, `POST ${path} answered ${res.status}`);
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

  await step("first-start", async () => {
    const before = await fetch(`${base}/api/v1/zones`);
    check(before.status === 403, `GET /zones before the setup answered ${before.status}, not 403`);
    await page.goto(`${base}/`);
    await page.getByText("Set up your Solite").waitFor();
    await page.getByLabel("Username").fill(USER);
    await page.getByLabel("Display name").fill("E2E");
    await page.getByLabel("Password", { exact: true }).fill(PASSWORD);
    await page.getByLabel("Confirm password").fill(PASSWORD);
    await page.getByRole("button", { name: "Create account" }).click();
    await page.getByRole("button", { name: "Log out" }).waitFor();
    token = await page.evaluate(() => localStorage.getItem("sowel_access_token"));
    check(token, "the interface kept no access token after the setup");
    const refused = await fetch(`${base}/api/v1/zones`);
    check(refused.status === 401, `GET /zones without a token answered ${refused.status}, not 401`);
  });

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
    await page.getByRole("heading", { level: 2, name: "Zigbee" }).waitFor();
    check(
      (await page.getByRole("button", { name: /^Zigbee/ }).count()) === 0,
      "Discovery shows a tab bar with one integration",
    );
    await page.getByRole("button", { name: "Add a device" }).click();
    // The three simulated devices join within 8 simulated seconds (speed 10).
    for (const model of ["ZBMINIR2", "TH01", "MS01"]) {
      await row(model).waitFor();
      await row(model).getByText("New", { exact: true }).waitFor();
    }
    check((await page.locator("tbody tr").count()) === 3, "Discovery does not list 3 devices");
    check(
      (await page.locator("tbody tr").getByRole("button", { name: "Create equipment" }).count()) ===
        0,
      "a Discovery row has a Create equipment button",
    );
  });

  // No equipment is made from a device (spec 037, R1): the type is picked, the devices are ticked.
  const addEquipment = async (name, type) => {
    await page.goto(`${base}/#/equipments`);
    await page.getByRole("button", { name: "Add equipment" }).first().click();
    await page.locator("#equipment-name").waitFor();
    await page.locator("#equipment-type").selectOption(type);
    await page.locator("#equipment-name").fill(name);
    await page.locator("#equipment-zone").selectOption({ label: "Garage" });
    await page.getByRole("button", { name: "Next: Select devices" }).click();
  };
  const tick = async (model) => {
    const box = page.getByRole("checkbox", { name: new RegExp(model) });
    await box.waitFor();
    if (!(await box.isChecked())) await box.check();
  };
  const createEnabled = () =>
    page.waitForFunction(() =>
      [...document.querySelectorAll("button")].some(
        (b) => b.textContent === "Create" && !b.disabled,
      ),
    );

  await step("create-light", async () => {
    // "Create" waits for the data of the device: wait until they are set up.
    await deviceWithData("ZBMINIR2", "state");
    await addEquipment("Lumière garage", "light_onoff");
    await tick("ZBMINIR2");
    await createEnabled();
    await page.getByRole("button", { name: "Create", exact: true }).click();
    await page.getByText("Lumière garage").first().waitFor();
  });

  await step("create-sensor", async () => {
    await deviceWithData("MS01", "occupancy");
    await deviceWithData("TH01", "temperature");
    await addEquipment("Capteurs garage", "sensor");
    await tick("TH01");
    await tick("MS01");
    await createEnabled();
    await page.getByRole("button", { name: "Create", exact: true }).click();
    await page.getByText("Capteurs garage").first().waitFor();
  });

  await step("devices-page", async () => {
    await page.goto(`${base}/#/devices`);
    await row("ZBMINIR2").waitFor();
    check(
      (await page.locator("tbody tr").getByRole("button", { name: "Create equipment" }).count()) ===
        0,
      "a Devices row has a Create equipment button",
    );
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

  await step("log-out-and-in", async () => {
    await page.getByRole("button", { name: "Log out" }).click();
    await page.getByRole("button", { name: "Log in" }).waitFor();
    await page.getByLabel("Username").fill(USER);
    await page.getByLabel("Password").fill("not-the-password-1");
    await page.getByRole("button", { name: "Log in" }).click();
    await page.getByText("Invalid credentials").waitFor();
    await page.getByLabel("Password").fill(PASSWORD);
    await page.getByRole("button", { name: "Log in" }).click();
    await page.getByRole("button", { name: "Log out" }).waitFor();
    await row("ZBMINIR2").waitFor();
  });

  // Spec 030: the pages of an equipment and of a device. The token changed at the log-in above.
  token = await page.evaluate(() => localStorage.getItem("sowel_access_token"));
  const lightId = async () =>
    (await api("/equipments")).find((e) => e.name === "Lumière garage").id;
  const lightFromApi = async () => api(`/equipments/${await lightId()}`);

  const openActivation = async () => {
    const sw = page.getByRole("switch", { name: "Activation" });
    if (!(await sw.isVisible()))
      await page
        .getByRole("button", { name: /Activation/ })
        .first()
        .click();
    await sw.waitFor();
  };

  await step("equipment-page-from-room", async () => {
    await page.goto(`${base}/#/devices`);
    await page
      .locator("aside a, nav a")
      .filter({ hasText: /^Garage$/ })
      .first()
      .click();
    await page.getByRole("link", { name: "Lumière garage" }).first().click();
    await page.getByRole("heading", { name: "Lumière garage" }).first().waitFor();
    await page
      .getByRole("button", { name: /Activation/ })
      .first()
      .waitFor();
    check(page.url().includes(`#/equipments/${await lightId()}`), `the address is ${page.url()}`);
  });

  await step("disable-light", async () => {
    await openActivation();
    await page.getByRole("switch", { name: "Activation" }).click();
    await until(
      "the API to report enabled:false",
      async () => (await lightFromApi()).enabled === false,
    );
    await page
      .getByRole("switch", { name: "Activation" })
      .and(page.locator('[aria-checked="false"]'))
      .waitFor();
  });

  await step("room-row-dimmed", async () => {
    await page.goto(`${base}/#/devices`);
    await page
      .locator("aside a, nav a")
      .filter({ hasText: /^Garage$/ })
      .first()
      .click();
    await page.getByRole("link", { name: "Lumière garage" }).first().waitFor();
    await until("the room row to be dimmed", () =>
      page.evaluate(() => {
        const link = [...document.querySelectorAll("a")].find(
          (a) => a.textContent === "Lumière garage",
        );
        return !!link?.closest(".opacity-50");
      }),
    );
  });

  await step("enable-light", async () => {
    await page.getByRole("link", { name: "Lumière garage" }).first().click();
    await openActivation();
    await page.getByRole("switch", { name: "Activation" }).click();
    await until(
      "the API to report enabled:true",
      async () => (await lightFromApi()).enabled === true,
    );
  });

  const openBindings = async () => {
    const toggleBindings = page.getByRole("button", { name: /^Bindings/ });
    await toggleBindings.waitFor();
    if ((await toggleBindings.getAttribute("aria-expanded")) !== "true")
      await toggleBindings.click();
    await page.locator('[data-testid="data-binding"]').first().waitFor();
  };

  // A motion-light instance on the light, so the check below is not empty: taking the `state` DATA
  // binding away and putting it back must leave it enabled (only the order binding is required).
  const lightNow = await lightFromApi();
  await apiPost("/recipe-instances", {
    recipeId: "motion-light",
    params: { zone: lightNow.zoneId, lights: [lightNow.id], timeout: "10m" },
  });
  const instancesBefore = await api("/recipe-instances");
  check(
    instancesBefore.some((i) => i.enabled === true),
    "no enabled recipe instance to check after the binding change",
  );

  await step("remove-state-data-binding", async () => {
    await openBindings();
    const binding = page.locator('[data-testid="data-binding"]').filter({ hasText: "state" });
    await binding.hover();
    await binding.getByRole("button", { name: "Remove" }).click();
    await until("the state data binding to be gone", async () => {
      const l = await lightFromApi();
      return !l.dataBindings.some((b) => b.alias === "state");
    });
    await page.getByText("1 new data point available").waitFor();
  });

  await step("review-and-add-missing", async () => {
    await page.getByRole("button", { name: "Review" }).click();
    await page.getByText("Add the missing bindings").waitFor();
    await page.getByRole("button", { name: /^Add 1$/ }).click();
    await until("the state data binding to be back", async () => {
      const l = await lightFromApi();
      return l.dataBindings.some((b) => b.alias === "state");
    });
    await page.getByText("Add the missing bindings").waitFor({ state: "hidden" });
  });

  await step("recipe-instances-unchanged", async () => {
    const after = await api("/recipe-instances");
    console.log(`e2e: ${instancesBefore.length} recipe instance(s) before, ${after.length} after`);
    for (const before of instancesBefore) {
      const now = after.find((i) => i.id === before.id);
      if (!now) throw new Error(`recipe instance ${before.id} is gone`);
      if (now.enabled !== before.enabled)
        throw new Error(`recipe instance ${before.id} enabled changed`);
    }
  });

  const sensorDevice = async () => (await api("/devices")).find((d) => d.model === "TH01");

  await step("device-page-from-devices", async () => {
    await page.goto(`${base}/#/devices`);
    await row("TH01").waitFor();
    await row("TH01")
      .locator("td")
      .nth(1)
      .click({ position: { x: 4, y: 4 } });
    const id = (await sensorDevice()).id;
    await until("the device page address", () => page.url().includes(`#/devices/${id}`));
    const temperature = page.locator("tr", { hasText: "temperature" }).first();
    await temperature.waitFor();
    await until("a value in the temperature row", async () =>
      /\d/.test(await temperature.innerText()),
    );
  });

  // Four screenshots of the pages, in the current theme.
  const four = async (theme) => {
    const eq = await lightId();
    const dev = (await sensorDevice()).id;
    // The sidebar slides while the page changes: wait for it to rest before a screenshot.
    const settled = async () => {
      await page.waitForFunction(() =>
        document.getAnimations().every((a) => a.playState !== "running"),
      );
    };
    await step(`${theme}-equipment-page`, async () => {
      await page.goto(`${base}/#/equipments/${eq}`);
      await openBindings();
      await settled();
    });
    await step(`${theme}-add-binding-dialog`, async () => {
      await page.getByRole("button", { name: "Add", exact: true }).first().click();
      await page.getByText("Add a data binding").waitFor();
      await settled();
    });
    await page.keyboard.press("Escape");
    await page.getByText("Add a data binding").waitFor({ state: "hidden" });
    await step(`${theme}-change-devices-dialog`, async () => {
      const change = page.getByRole("button", { name: "Change devices" });
      if (!(await change.isVisible()))
        await page
          .getByRole("button", { name: /^Devices/ })
          .first()
          .click();
      await change.click();
      await page.locator('[role="dialog"]').waitFor();
      await settled();
    });
    await page.keyboard.press("Escape");
    await page.locator('[role="dialog"]').waitFor({ state: "hidden" });
    await step(`${theme}-device-page`, async () => {
      await page.goto(`${base}/#/devices/${dev}`);
      await page.locator("tr", { hasText: "temperature" }).first().waitFor();
      await settled();
    });
  };

  // The interface follows the system theme (main.tsx applies "system"; the stored `sowel_theme`
  // is read by nothing yet): the browser's colour scheme is what is switched.
  await page.emulateMedia({ colorScheme: "dark" });
  await page.waitForFunction(() => document.documentElement.dataset.theme === "dark");
  await four("dark");

  await page.emulateMedia({ colorScheme: "light" });
  await page.waitForFunction(() => document.documentElement.dataset.theme === "hybrid");
  await four("hybrid");
  void modal;
} catch (error) {
  console.error(`e2e: ${error.message}`);
  process.exitCode = 1;
} finally {
  await browser.close();
}
