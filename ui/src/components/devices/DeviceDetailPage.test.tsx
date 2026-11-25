import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useDeviceDetail } from "../../store/useDeviceDetail";
import { useDevices } from "../../store/useDevices";
import {
  advance,
  device,
  deviceWithData,
  mockApi,
  resetStores,
  type Route,
} from "../../test-utils";
import { DeviceDetailPage } from "./DeviceDetailPage";

const DEV = "/api/v1/devices/zb";

beforeEach(() => {
  window.location.hash = "#/devices/zb";
  vi.spyOn(window, "confirm").mockReturnValue(true);
});

afterEach(async () => {
  cleanup();
  resetStores();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  await new Promise((resolve) => setTimeout(resolve, 50));
});

function full() {
  const d = deviceWithData(
    "zb",
    "ZBMINIR2",
    [
      ["temperature", "temperature"],
      ["battery", "battery"],
      ["humidity", "humidity"],
    ],
    { ieeeAddress: "0x00124b0001" },
    [{ key: "state" }, { key: "mode", type: "enum", enumValues: ["auto", "manual"] }],
  );
  d.orders.push({
    id: "zb-o-lvl",
    deviceId: "zb",
    key: "level",
    type: "number",
    min: 0,
    max: 100,
    unit: "%",
  });
  return d;
}

async function setup(routes: Record<string, Route | Route[]> = {}, listed = true) {
  const api = mockApi({ [`GET ${DEV}`]: { body: full() }, ...routes });
  if (listed) {
    useDevices.setState({
      devices: { zb: device("zb", "ZBMINIR2", { ieeeAddress: "0x00124b0001" }) },
    });
  }
  render(<DeviceDetailPage deviceId="zb" />);
  return api;
}

describe("DeviceDetailPage (spec 030, R7)", () => {
  it("shows the information, the data sorted by category then key, the orders with their range", async () => {
    await setup();
    expect(await screen.findByText("Online")).toBeTruthy();
    expect(screen.getByText("SONOFF")).toBeTruthy();
    expect(screen.getByText("0x00124b0001", { selector: "span.font-mono" })).toBeTruthy();
    expect(screen.getByText("0xzb")).toBeTruthy();
    expect(screen.getByText("never")).toBeTruthy();
    expect(screen.getByRole("link", { name: /Devices/ }).getAttribute("href")).toBe("#/devices");
    const [dataTable, ordersTable] = screen.getAllByRole("table");
    // battery, humidity, temperature: categories in order.
    const keys = within(dataTable)
      .getAllByRole("row")
      .slice(1)
      .map((r) => within(r).getAllByRole("cell")[0].textContent);
    expect(keys).toEqual(["battery", "humidity", "temperature"]);
    const orders = within(ordersTable).getAllByRole("row").slice(1);
    expect(orders.map((r) => r.textContent)).toEqual([
      expect.stringContaining("state"),
      expect.stringContaining("automanual"),
      expect.stringMatching(/level.*0–100\s*%/),
    ]);
    expect(within(orders[0]).getAllByRole("cell")[2].textContent).toBe("—");
  });

  it("leaves out a field the device does not give", async () => {
    mockApi({
      [`GET ${DEV}`]: {
        body: deviceWithData("zb", "Relay", [], { manufacturer: undefined, model: undefined }),
      },
    });
    useDevices.setState({
      devices: { zb: device("zb", "Relay", { manufacturer: undefined, model: undefined }) },
    });
    render(<DeviceDetailPage deviceId="zb" />);
    await screen.findByText("No data");
    expect(screen.queryByText("Manufacturer")).toBeNull();
    expect(screen.queryByText("Model")).toBeNull();
    expect(screen.queryByText("IEEE address")).toBeNull();
  });

  it("has no Orders section when the device has no order", async () => {
    await setup({
      [`GET ${DEV}`]: { body: deviceWithData("zb", "Sensor", [["t", "temperature"]]) },
    });
    await screen.findByText("Data");
    expect(screen.queryByRole("heading", { name: "Orders" })).toBeNull();
    expect(screen.getAllByRole("table")).toHaveLength(1);
  });

  it("says Device not found on a 404, with the back link", async () => {
    await setup({ [`GET ${DEV}`]: { status: 404, body: { error: "Device not found" } } });
    expect(await screen.findByText("Device not found")).toBeTruthy();
    expect(screen.getByRole("link", { name: /Devices/ }).getAttribute("href")).toBe("#/devices");
  });

  it("shows another failure with Retry, which reads again", async () => {
    const api = await setup({
      [`GET ${DEV}`]: [{ status: 400, body: { error: "radio busy" } }, { body: full() }],
    });
    expect(await screen.findByText(/radio busy/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await screen.findByText("battery", { selector: "span.font-mono" });
    expect(api.count(`GET ${DEV}`)).toBe(2);
  });

  it("shows the error and Retry when the devices list fails, instead of a spinner", async () => {
    const api = await setup(
      { "GET /api/v1/devices": [{ status: 400, body: { error: "list down" } }, { body: [] }] },
      false,
    );
    await useDevices.getState().fetchDevices();
    expect(await screen.findByText(/list down/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(api.count("GET /api/v1/devices")).toBe(2));
  });

  it("follows device.status_changed (from the devices store) and a rename", async () => {
    await setup({ [`PUT ${DEV}`]: { body: {} } });
    await screen.findByText("Online");
    act(() => useDevices.getState().updateDeviceStatus("zb", "offline"));
    expect(await screen.findByText("Offline")).toBeTruthy();
    fireEvent.click(screen.getByTitle("Edit name"));
    fireEvent.input(screen.getByRole("textbox"), { target: { value: "Garage relay" } });
    fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter" });
    expect(await screen.findByText("Garage relay")).toBeTruthy();
  });

  it("follows a data update of the store", async () => {
    await setup();
    await screen.findByText("battery", { selector: "span.font-mono" });
    act(() =>
      useDeviceDetail.getState().handleDataUpdated({
        deviceId: "zb",
        dataId: "zb-battery",
        key: "battery",
        value: 77,
        timestamp: new Date().toISOString(),
      }),
    );
    expect(await screen.findByText(/77/)).toBeTruthy();
  });

  it("shows Device not found when the device is removed", async () => {
    await setup();
    await screen.findByText("battery", { selector: "span.font-mono" });
    act(() => useDeviceDetail.getState().handleRemoved("zb"));
    expect(await screen.findByText("Device not found")).toBeTruthy();
  });

  it("deletes after confirmation and goes to the list", async () => {
    const api = await setup({ [`DELETE ${DEV}`]: { status: 204 } });
    await screen.findByText("battery", { selector: "span.font-mono" });
    (window.confirm as ReturnType<typeof vi.fn>).mockReturnValueOnce(false);
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(api.count(`DELETE ${DEV}`)).toBe(0);
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(window.location.hash).toBe("#/devices"));
    expect(api.count(`DELETE ${DEV}`)).toBe(1);
    expect(useDevices.getState().devices.zb).toBeUndefined();
  });

  it("shows the text of a refused delete for 5 s (409) and stays", async () => {
    await setup({ [`DELETE ${DEV}`]: { status: 409, body: { error: "Device is being paired" } } });
    await screen.findByText("battery", { selector: "span.font-mono" });
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    });
    await advance(0);
    expect(screen.getByText("Device is being paired")).toBeTruthy();
    expect(window.location.hash).toBe("#/devices/zb");
    await advance(5000);
    expect(screen.queryByText("Device is being paired")).toBeNull();
  });
});
