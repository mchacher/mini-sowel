import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { EquipmentType } from "../../types";
import { deviceWithData, mockApi } from "../../test-utils";
import { DeviceSelector } from "./DeviceSelector";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const TH01 = deviceWithData("th", "TH01", [
  ["temperature", "temperature"],
  ["humidity", "humidity"],
  ["battery", "battery"],
]);
const MS01 = deviceWithData("ms", "MS01", [
  ["occupancy", "motion"],
  ["battery", "battery"],
]);
const ZBMINIR2 = deviceWithData("zb", "ZBMINIR2", [["state", "light_state"]]);

async function setup(
  type: EquipmentType,
  devices: unknown[] = [TH01, MS01, ZBMINIR2],
  selected: string[] = [],
) {
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
  it("lists only the devices with a data of the type's categories", async () => {
    await setup("light_onoff");
    expect(listed()).toEqual(["ZBMINIR2"]);
  });

  it("lists the sensors for a sensor", async () => {
    await setup("sensor");
    expect(listed()).toEqual(["TH01", "MS01"]);
  });

  it("says nothing is compatible with a thermostat, and All devices lists the three", async () => {
    await setup("thermostat");
    expect(screen.getByText("No compatible devices found for this equipment type.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Show all devices" }));
    expect(listed()).toEqual(["TH01", "MS01", "ZBMINIR2"]);
  });

  it("narrows the list by name", async () => {
    await setup("light_onoff");
    fireEvent.click(screen.getByRole("button", { name: "Show all devices" }));
    fireEvent.input(screen.getByPlaceholderText("Filter by name..."), { target: { value: "zb" } });
    expect(listed()).toEqual(["ZBMINIR2"]);
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

  it("warns, without blocking, when the chosen devices give more than 16 values", async () => {
    const many = deviceWithData(
      "big",
      "Big",
      Array.from({ length: 17 }, (_, i): [string, "temperature"] => [`k${i}`, "temperature"]),
    );
    await setup("sensor", [many, TH01, MS01], ["big"]);
    expect(screen.getByText("These devices give 17 values; only 16 are bound")).toBeTruthy();
    cleanup();
    await setup("sensor", [TH01, MS01], ["th", "ms"]);
    expect(screen.queryByText(/only 16 are bound/)).toBeNull();
  });
});
