import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { EquipmentType } from "../../types";
import { deviceWithData, fixtures, mockApi } from "../../test-utils";
import { DeviceSelector } from "./DeviceSelector";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const TH01 = fixtures.th01();
const MS01 = fixtures.ms01();
const ZBMINIR2 = fixtures.zbminir2();
const BULB = fixtures.bulb();
const VOLTAGE = fixtures.voltageOnly();
const FIVE = [TH01, MS01, ZBMINIR2, BULB, VOLTAGE];

async function setup(type: EquipmentType, devices: unknown[] = FIVE, selected: string[] = []) {
  const onSelectionChange = vi.fn();
  mockApi({ "GET /api/v1/devices": { body: devices } });
  render(
    <DeviceSelector
      equipmentType={type}
      selectedDeviceIds={selected}
      onSelectionChange={onSelectionChange}
    />,
  );
  await loaded();
  return onSelectionChange;
}

const loaded = () => waitFor(() => expect(screen.queryByText("Loading devices...")).toBeNull());

const listed = () =>
  screen.queryAllByRole("checkbox").map((c) => c.getAttribute("aria-label") as string);

describe("DeviceSelector", () => {
  it("lists the ZBMINIR2 and the bulb for a light, and All devices lists the five", async () => {
    await setup("light_onoff");
    expect(listed()).toEqual(["ZBMINIR2", "Bulb"]);
    fireEvent.click(screen.getByRole("button", { name: "Show all devices" }));
    expect(listed()).toEqual(["TH01", "MS01", "ZBMINIR2", "Bulb", "Voltage only"]);
  });

  it("lists the devices ticked first, compatible or not, when the type is changed", async () => {
    await setup("sensor", FIVE, ["zb"]);
    expect(listed()).toEqual(["ZBMINIR2", "TH01", "MS01"]);
    expect((screen.getByLabelText("ZBMINIR2") as HTMLInputElement).checked).toBe(true);
  });

  it("gives every read of the devices back to the form", async () => {
    const onDevicesRead = vi.fn();
    mockApi({ "GET /api/v1/devices": { body: FIVE } });
    render(
      <DeviceSelector
        equipmentType="sensor"
        selectedDeviceIds={[]}
        onSelectionChange={vi.fn()}
        onDevicesRead={onDevicesRead}
      />,
    );
    await loaded();
    expect(onDevicesRead).toHaveBeenCalledTimes(1);
    expect(onDevicesRead.mock.calls[0][0]).toHaveLength(5);
  });

  it("lists the sensors for a sensor", async () => {
    await setup("sensor");
    expect(listed()).toEqual(["TH01", "MS01"]);
  });

  it("says nothing is compatible with a thermostat, and All devices is offered", async () => {
    await setup("thermostat");
    expect(screen.getByText("No compatible devices found for this equipment type.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Show all devices" }));
    expect(listed()).toEqual(["TH01", "MS01", "ZBMINIR2", "Bulb", "Voltage only"]);
  });

  it("narrows the list by name", async () => {
    await setup("light_onoff");
    fireEvent.click(screen.getByRole("button", { name: "Show all devices" }));
    fireEvent.input(screen.getByPlaceholderText("Filter by name..."), { target: { value: "zb" } });
    expect(listed()).toEqual(["ZBMINIR2"]);
  });

  it("reads again every 5 s while told to, and stops when told to stop", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    try {
      const api = mockApi({ "GET /api/v1/devices": { body: FIVE } });
      const props = {
        equipmentType: "sensor" as const,
        selectedDeviceIds: [],
        onSelectionChange: vi.fn(),
      };
      const view = render(<DeviceSelector {...props} reread />);
      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });
      expect(api.count("GET /api/v1/devices")).toBe(1);
      await act(async () => {
        await vi.advanceTimersByTimeAsync(5000);
      });
      expect(api.count("GET /api/v1/devices")).toBe(2);
      view.rerender(<DeviceSelector {...props} reread={false} />);
      await act(async () => {
        await vi.advanceTimersByTimeAsync(15000);
      });
      expect(api.count("GET /api/v1/devices")).toBe(2);
    } finally {
      vi.useRealTimers();
    }
  });

  it("shows a device's data when its expander is opened", async () => {
    await setup("sensor");
    fireEvent.click(screen.getAllByRole("button", { name: "Data" })[0]);
    expect(screen.getByText("temperature", { selector: "span.font-mono" })).toBeTruthy();
    expect(screen.getByText("(humidity)")).toBeTruthy();
  });

  it("shows the error text and an empty list when the devices cannot be read", async () => {
    const onSelectionChange = vi.fn();
    mockApi({ "GET /api/v1/devices": { status: 400, body: { error: "disk error" } } });
    render(
      <DeviceSelector
        equipmentType="sensor"
        selectedDeviceIds={[]}
        onSelectionChange={onSelectionChange}
      />,
    );
    await loaded();
    expect(screen.getByText("disk error")).toBeTruthy();
    expect(listed()).toEqual([]);
  });

  it("says no device is compatible when there is no device at all", async () => {
    await setup("sensor", []);
    expect(screen.getByText("No devices found. Add a device first.")).toBeTruthy();
  });

  it("checks and unchecks a device", async () => {
    const onChange = await setup("sensor", [TH01, MS01], ["th"]);
    fireEvent.click(screen.getByLabelText("MS01"));
    expect(onChange).toHaveBeenCalledWith(["th", "ms"]);
    fireEvent.click(screen.getByLabelText("TH01"));
    expect(onChange).toHaveBeenCalledWith([]);
  });

  it("disables the other check boxes once 64 devices are chosen, and says so", async () => {
    const devices = Array.from({ length: 65 }, (_, i) =>
      deviceWithData(`d${i}`, `Sensor ${i}`, [[`k${i}`, "temperature"]]),
    );
    await setup(
      "sensor",
      devices,
      devices.slice(0, 64).map((d) => d.id),
    );
    const boxes = screen.getAllByRole("checkbox") as HTMLInputElement[];
    expect(boxes).toHaveLength(65);
    expect(boxes[64].disabled).toBe(true);
    expect(boxes[0].disabled).toBe(false);
    expect(screen.getByText("At most 64 devices")).toBeTruthy();
  });

  it("warns, without blocking, when the plan holds more than 16 values", async () => {
    const six = Array.from({ length: 6 }, (_, i) => fixtures.th01(`th${i}`));
    await setup(
      "sensor",
      six,
      six.map((d) => d.id),
    );
    expect(screen.getByText("These devices give 18 values; only 16 are bound")).toBeTruthy();
    cleanup();
    await setup("sensor", [TH01, MS01], ["th", "ms"]);
    expect(screen.queryByText(/only 16 are bound/)).toBeNull();
  });

  it("does not count what the type does not use: a voltage is no value for a sensor", async () => {
    const many = deviceWithData(
      "big",
      "Big",
      Array.from({ length: 20 }, (_, i): [string, "voltage"] => [`k${i}`, "voltage"]),
    );
    await setup("sensor", [many], ["big"]);
    expect(screen.queryByText(/only 16 are bound/)).toBeNull();
  });
});
