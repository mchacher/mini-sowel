import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "../../api/client";
import { fixtures, mockApi, zone } from "../../test-utils";
import { EquipmentForm } from "./EquipmentForm";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const tree = [zone("root", "Home", [zone("garage", "Garage", [], "root")])];
const TH01 = fixtures.th01();
const MS01 = fixtures.ms01();
const ZBMINIR2 = fixtures.zbminir2();
const VOLTAGE = fixtures.voltageOnly();

function setup(
  extra: Partial<Parameters<typeof EquipmentForm>[0]> = {},
  devices: unknown[] = [TH01, MS01, ZBMINIR2, VOLTAGE],
) {
  const api = mockApi({ "GET /api/v1/devices": { body: devices } });
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
  return { onSubmit, onClose, api };
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

  it("gives the devices ticked and read to onSubmit, and closes", async () => {
    const { onSubmit, onClose } = setup();
    await toDevices();
    fireEvent.click(screen.getByLabelText("ZBMINIR2"));
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(onSubmit).toHaveBeenCalledTimes(1);
    const sent = (onSubmit.mock.calls as unknown as unknown[][])[0][0] as {
      devices: { id: string; orders: unknown[] }[];
    };
    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({
        name: "Lumière garage",
        type: "light_onoff",
        zoneId: "garage",
        deviceIds: ["zb"],
      }),
    );
    expect(sent.devices.map((d) => d.id)).toEqual(["zb"]);
    expect(sent.devices[0].orders).toHaveLength(1);
  });

  it("keeps Create disabled with no device ticked, and says to choose one", async () => {
    const { onSubmit } = setup();
    await toDevices("Empty");
    const create = screen.getByRole("button", { name: "Create" }) as HTMLButtonElement;
    expect(create.disabled).toBe(true);
    expect(screen.getByText("Choose at least one device")).toBeTruthy();
    fireEvent.click(create);
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("sends Create once when it is clicked twice", async () => {
    const onSubmit = vi.fn(() => new Promise<void>(() => undefined));
    setup({ onSubmit });
    await toDevices();
    fireEvent.click(screen.getByLabelText("ZBMINIR2"));
    const create = screen.getByRole("button", { name: "Create" });
    fireEvent.click(create);
    fireEvent.click(create);
    expect(onSubmit).toHaveBeenCalledTimes(1);
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
    expect((screen.getByRole("button", { name: "Create" }) as HTMLButtonElement).disabled).toBe(
      false,
    );
  });

  it("keeps the devices ticked, listed first, when the type is changed, and the fields on Back", async () => {
    const { onSubmit } = setup();
    await toDevices();
    fireEvent.click(screen.getByLabelText("ZBMINIR2"));
    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    expect((screen.getByLabelText("Name") as HTMLInputElement).value).toBe("Lumière garage");
    expect((screen.getByLabelText("Zone") as HTMLSelectElement).value).toBe("garage");
    choose("Type", "sensor");
    fireEvent.click(next());
    await loaded();
    const boxes = screen.getAllByRole("checkbox") as HTMLInputElement[];
    expect(boxes[0].getAttribute("aria-label")).toBe("ZBMINIR2");
    expect(boxes[0].checked).toBe(true);
    // The relay gives a sensor nothing: Create waits for a device that does.
    expect((screen.getByRole("button", { name: "Create" }) as HTMLButtonElement).disabled).toBe(
      true,
    );
    fireEvent.click(screen.getByLabelText("TH01"));
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await waitFor(() => expect(onSubmit).toHaveBeenCalled());
    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({ type: "sensor", deviceIds: ["zb", "th"] }),
    );
  });

  it("opens with the type, name and device it is given, the room still to choose", async () => {
    const { onSubmit } = setup({
      initialType: "sensor",
      initialName: "TH01",
      initialDeviceIds: ["th"],
    });
    expect((screen.getByLabelText("Type") as HTMLSelectElement).value).toBe("sensor");
    expect((screen.getByLabelText("Name") as HTMLInputElement).value).toBe("TH01");
    expect((screen.getByLabelText("Zone") as HTMLSelectElement).value).toBe("");
    expect(next().disabled).toBe(true);
    choose("Zone", "garage");
    fireEvent.click(next());
    await loaded();
    expect((screen.getByLabelText("TH01") as HTMLInputElement).checked).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await waitFor(() => expect(onSubmit).toHaveBeenCalled());
  });

  describe("a ticked device that gives nothing yet", () => {
    afterEach(() => vi.useRealTimers());

    const advance = (ms: number) =>
      act(async () => {
        await vi.advanceTimersByTimeAsync(ms);
      });

    it("waits, reading again every 5 s, until the device gives what the type uses", async () => {
      vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
      const fresh = { ...fixtures.zbminir2(), data: [], orders: [] };
      const api = mockApi({
        "GET /api/v1/devices": [{ body: [fresh] }, { body: [fixtures.zbminir2()] }],
      });
      render(
        <EquipmentForm
          title="Create equipment"
          zones={tree}
          initialType="light_onoff"
          initialName="ZBMINIR2"
          initialDeviceIds={["zb"]}
          defaultZoneId="garage"
          onSubmit={async () => {}}
          onClose={() => {}}
        />,
      );
      fireEvent.click(next());
      await advance(0);
      const create = () => screen.getByRole("button", { name: "Create" }) as HTMLButtonElement;
      expect(create().disabled).toBe(true);
      expect(screen.getByText(/These devices give nothing this type uses yet/)).toBeTruthy();
      expect(api.count("GET /api/v1/devices")).toBe(1);
      await advance(5000);
      expect(api.count("GET /api/v1/devices")).toBe(2);
      expect(create().disabled).toBe(false);
      expect(screen.queryByText(/give nothing this type uses yet/)).toBeNull();
      await advance(20000);
      expect(api.count("GET /api/v1/devices")).toBe(2);
    });

    it("guesses the type again from a read that brings the data, until the user picks one", async () => {
      vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
      const fresh = { ...fixtures.th01(), data: [], orders: [] };
      mockApi({ "GET /api/v1/devices": [{ body: [fresh] }, { body: [fixtures.th01()] }] });
      render(
        <EquipmentForm
          title="Create equipment"
          zones={tree}
          initialType="light_onoff"
          typeIsGuess
          initialName="TH01"
          initialDeviceIds={[fresh.id]}
          defaultZoneId="garage"
          onSubmit={async () => {}}
          onClose={() => {}}
        />,
      );
      fireEvent.click(next());
      await advance(0);
      const create = () => screen.getByRole("button", { name: "Create" }) as HTMLButtonElement;
      expect(create().disabled).toBe(true);
      await advance(5000);
      expect(create().disabled).toBe(false);
      fireEvent.click(screen.getByRole("button", { name: "Back" }));
      expect((screen.getByLabelText("Type") as HTMLSelectElement).value).toBe("sensor");
    });

    it("reads once, not every 5 s, when the plan is not empty", async () => {
      vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
      const api = mockApi({ "GET /api/v1/devices": { body: [ZBMINIR2] } });
      render(
        <EquipmentForm
          title="Create equipment"
          zones={tree}
          initialDeviceIds={["zb"]}
          defaultZoneId="garage"
          initialName="x"
          onSubmit={async () => {}}
          onClose={() => {}}
        />,
      );
      fireEvent.click(next());
      await advance(0);
      await advance(30000);
      expect(api.count("GET /api/v1/devices")).toBe(1);
    });

    it("stops reading when step 2 is left", async () => {
      vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
      const api = mockApi({ "GET /api/v1/devices": { body: [VOLTAGE] } });
      render(
        <EquipmentForm
          title="Create equipment"
          zones={tree}
          initialDeviceIds={["volt"]}
          defaultZoneId="garage"
          initialName="x"
          onSubmit={async () => {}}
          onClose={() => {}}
        />,
      );
      fireEvent.click(next());
      await advance(0);
      await advance(5000);
      expect(api.count("GET /api/v1/devices")).toBe(2);
      fireEvent.click(screen.getByRole("button", { name: "Back" }));
      await advance(30000);
      expect(api.count("GET /api/v1/devices")).toBe(2);
    });
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
      fireEvent.click(screen.getByLabelText("ZBMINIR2"));
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
    fireEvent.click(screen.getByLabelText("ZBMINIR2"));
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
      devices: [],
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
