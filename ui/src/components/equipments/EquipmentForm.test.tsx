import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "../../api/client";
import { deviceWithData, mockApi, zone } from "../../test-utils";
import { EquipmentForm } from "./EquipmentForm";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const tree = [zone("root", "Home", [zone("garage", "Garage", [], "root")])];
const TH01 = deviceWithData("th", "TH01", [
  ["temperature", "temperature"],
  ["battery", "battery"],
]);
const MS01 = deviceWithData("ms", "MS01", [
  ["occupancy", "motion"],
  ["battery", "battery"],
]);
const ZBMINIR2 = deviceWithData("zb", "ZBMINIR2", [["state", "light_state"]]);

function setup(
  extra: Partial<Parameters<typeof EquipmentForm>[0]> = {},
  devices: unknown[] = [TH01, MS01, ZBMINIR2],
) {
  mockApi({ "GET /api/v1/devices": { body: devices } });
  const onSubmit = vi.fn(async () => {});
  const onClose = vi.fn();
  render(
    <EquipmentForm
      title="Create equipment"
      zones={tree}
      onSubmit={onSubmit}
      onClose={onClose}
      {...extra}
    />,
  );
  return { onSubmit, onClose };
}

const type = (label: string, value: string) =>
  fireEvent.input(screen.getByLabelText(label), { target: { value } });
const choose = (label: string, value: string) =>
  fireEvent.change(screen.getByLabelText(label), { target: { value } });
const next = () =>
  screen.getByRole("button", { name: "Next: Select devices" }) as HTMLButtonElement;
const loaded = () => waitFor(() => expect(screen.queryByText("Loading devices...")).toBeNull());

async function toDevices(name = "Lumière garage") {
  type("Name", name);
  choose("Zone", "garage");
  fireEvent.click(next());
  await loaded();
}

describe("EquipmentForm", () => {
  it("keeps Next disabled while the name is blank or no room is chosen", () => {
    setup();
    expect(next().disabled).toBe(true);
    type("Name", "Light");
    expect(next().disabled).toBe(true);
    choose("Zone", "garage");
    expect(next().disabled).toBe(false);
    type("Name", "  ");
    expect(next().disabled).toBe(true);
  });

  it("offers the seven types sorted by label", () => {
    setup();
    const labels = Array.from((screen.getByLabelText("Type") as HTMLSelectElement).options).map(
      (o) => o.textContent,
    );
    expect(labels).toEqual([
      "Button or remote",
      "Light (Dimmable)",
      "Light (On/Off)",
      "Plug or relay",
      "Sensor",
      "Shutter",
      "Thermostat",
    ]);
    expect((screen.getByLabelText("Type") as HTMLSelectElement).value).toBe("light_onoff");
  });

  it("creates a light from the relay with one request and closes", async () => {
    const { onSubmit, onClose } = setup();
    await toDevices();
    fireEvent.click(screen.getByLabelText("ZBMINIR2"));
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(onSubmit).toHaveBeenCalledWith({
      name: "Lumière garage",
      type: "light_onoff",
      zoneId: "garage",
      deviceIds: ["zb"],
    });
  });

  it("creates with no device chosen: an empty deviceIds", async () => {
    const { onSubmit } = setup();
    await toDevices("Empty");
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await waitFor(() => expect(onSubmit).toHaveBeenCalled());
    expect(onSubmit).toHaveBeenCalledWith(expect.objectContaining({ deviceIds: [] }));
  });

  it("warns for 17 distinct data keys on a sensor", async () => {
    const big = deviceWithData(
      "big",
      "Big",
      Array.from({ length: 17 }, (_, i): [string, "temperature"] => [`k${i}`, "temperature"]),
    );
    const { onSubmit } = setup({}, [big, TH01]);
    type("Name", "Sensors");
    choose("Type", "sensor");
    choose("Zone", "garage");
    fireEvent.click(next());
    await loaded();
    fireEvent.click(screen.getByLabelText("Big"));
    expect(screen.getByText("These devices give 17 values; only 16 are bound")).toBeTruthy();
    const create = screen.getByRole("button", { name: "Create" }) as HTMLButtonElement;
    expect(create.disabled).toBe(false);
    fireEvent.click(create);
    await waitFor(() => expect(onSubmit).toHaveBeenCalled());
    expect(onSubmit).toHaveBeenCalledWith(expect.objectContaining({ deviceIds: ["big"] }));
  });

  it("shows no warning for TH01 and MS01, whose battery is given twice", async () => {
    setup();
    type("Name", "Sensors");
    choose("Type", "sensor");
    choose("Zone", "garage");
    fireEvent.click(next());
    await loaded();
    fireEvent.click(screen.getByLabelText("TH01"));
    fireEvent.click(screen.getByLabelText("MS01"));
    expect(screen.queryByText(/only 16 are bound/)).toBeNull();
  });

  it("clears the devices when the type changes, and keeps the fields on Back", async () => {
    const { onSubmit } = setup();
    await toDevices();
    fireEvent.click(screen.getByLabelText("ZBMINIR2"));
    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    expect((screen.getByLabelText("Name") as HTMLInputElement).value).toBe("Lumière garage");
    expect((screen.getByLabelText("Zone") as HTMLSelectElement).value).toBe("garage");
    choose("Type", "switch");
    fireEvent.click(next());
    await loaded();
    expect((screen.getByLabelText("ZBMINIR2") as HTMLInputElement).checked).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await waitFor(() => expect(onSubmit).toHaveBeenCalled());
    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({ type: "switch", deviceIds: [] }),
    );
  });

  it.each(["Zone not found: z1", "Equipment not found"])(
    "shows the server's text for a 404: %s",
    async (text) => {
      setup({
        onSubmit: async () => {
          throw new ApiError(404, text);
        },
      });
      await toDevices();
      fireEvent.click(screen.getByRole("button", { name: "Create" }));
      expect(await screen.findByText(text)).toBeTruthy();
    },
  );

  it("stays open with the API's text when it refuses", async () => {
    const { onClose } = setup({
      onSubmit: async () => {
        throw new ApiError(400, "Too many equipments (64)");
      },
    });
    await toDevices();
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    expect(await screen.findByText("Too many equipments (64)")).toBeTruthy();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("edits in one step with the type locked, sending the name and the room", async () => {
    const { onSubmit } = setup({
      title: "Edit equipment",
      initial: { name: "Light", type: "light_onoff", zoneId: "garage" },
    });
    const kind = screen.getByLabelText("Type") as HTMLSelectElement;
    expect(kind.disabled).toBe(true);
    expect(screen.queryByRole("button", { name: "Next: Select devices" })).toBeNull();
    type("Name", "Ceiling light");
    choose("Zone", "root");
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(onSubmit).toHaveBeenCalled());
    expect(onSubmit).toHaveBeenCalledWith({
      name: "Ceiling light",
      type: "light_onoff",
      zoneId: "root",
      deviceIds: [],
    });
  });

  it("preselects the room it is given", () => {
    setup({ defaultZoneId: "garage" });
    expect((screen.getByLabelText("Zone") as HTMLSelectElement).value).toBe("garage");
  });

  it("closes on Escape, the cross and a click outside, sending nothing", () => {
    const { onSubmit, onClose } = setup();
    fireEvent.keyDown(document, { key: "Escape" });
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    fireEvent.click(screen.getByRole("dialog").parentElement!);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalledTimes(4);
    expect(onSubmit).not.toHaveBeenCalled();
  });
});
