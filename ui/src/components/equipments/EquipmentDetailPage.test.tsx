import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useDevices } from "../../store/useDevices";
import { useEquipments } from "../../store/useEquipments";
import { useZones } from "../../store/useZones";
import {
  advance,
  dataBinding,
  device,
  equipment,
  fixtures,
  light,
  mockApi,
  resetStores,
  sensor,
  zone,
  type Route,
} from "../../test-utils";
import type { EquipmentWithDetails, OrderBindingWithDetails } from "../../types";
import { ChangeDeviceModal, DevicesSection, EquipmentDetailPage } from "./EquipmentDetailPage";

const EQ = "/api/v1/equipments";
const tree = () => [zone("root", "Home", [zone("garage", "Garage", [], "root")])];

beforeEach(() => {
  window.location.hash = "";
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

function setup(
  equipments: EquipmentWithDetails[],
  routes: Record<string, Route | Route[]> = {},
  id = equipments[0]?.id ?? "l",
) {
  const api = mockApi({
    [`GET ${EQ}`]: { body: equipments },
    "GET /api/v1/devices": { body: [fixtures.zbminir2()] },
    "GET /api/v1/devices/zb": { body: fixtures.zbminir2() },
    ...routes,
  });
  useZones.setState({ tree: tree() });
  useEquipments.setState({ equipments, loaded: true });
  render(<EquipmentDetailPage equipmentId={id} />);
  return api;
}

const openRow = (name: RegExp | string) => fireEvent.click(screen.getByRole("button", { name }));

/** A light bound to the ZBMINIR2 (`zb`) through its `state` order only: its `state` data is missing. */
function lightMissingState(extra: Partial<EquipmentWithDetails> = {}): EquipmentWithDetails {
  const base = light("l", "Garage light", "garage", false);
  const order: OrderBindingWithDetails = {
    ...base.orderBindings[0],
    deviceId: "zb",
    deviceName: "ZBMINIR2",
    deviceOrderId: "zb-o-state",
  };
  return { ...base, dataBindings: [], orderBindings: [order], ...extra };
}

/** A light bound to `zb` through both its bindings. */
function boundLight(extra: Partial<EquipmentWithDetails> = {}): EquipmentWithDetails {
  const base = lightMissingState();
  return {
    ...base,
    dataBindings: [
      dataBinding("state", "light_state", false, {
        id: "db-state",
        equipmentId: "l",
        deviceId: "zb",
        deviceName: "ZBMINIR2",
        deviceDataId: "zb-state",
      }),
    ],
    ...extra,
  };
}

describe("EquipmentDetailPage (spec 030, R3)", () => {
  it("shows the header, the light's control and the configuration rows", () => {
    setup([boundLight()]);
    expect(screen.getByRole("heading", { level: 1, name: "Garage light" })).toBeTruthy();
    expect(screen.getByText(/Light \(On\/Off\) · Garage/)).toBeTruthy();
    // Sowel's full control: a button showing the light's state (off here).
    expect(screen.getByRole("button", { name: "Off" })).toBeTruthy();
    expect(screen.getByRole("heading", { level: 3, name: "Controls" })).toBeTruthy();
    for (const row of [/Activation/, /Devices/, /Bindings/, /Delete equipment/]) {
      expect(screen.getByRole("button", { name: row })).toBeTruthy();
    }
    expect(screen.getByRole("button", { name: "Edit" })).toBeTruthy();
  });

  it("shows Disabled in the header and no control for a disabled equipment", () => {
    setup([boundLight({ enabled: false })]);
    expect(screen.getByText("Disabled", { selector: "span.text-warning-ink" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Off" })).toBeNull();
    expect(screen.queryByRole("heading", { level: 3, name: "Controls" })).toBeNull();
  });

  it("shows a sensor's values, also when disabled", () => {
    setup([sensor("s", "Climate", "garage", { enabled: false })]);
    expect(screen.getByText(/12\.6/)).toBeTruthy();
    expect(screen.getByRole("heading", { level: 3, name: "Values" })).toBeTruthy();
  });

  it("shows a refused order's text for 5 s under the control", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    setup([boundLight()], {
      [`POST ${EQ}/l/orders/state`]: { status: 502, body: { error: "device did not answer" } },
    });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Off" }));
    });
    await advance(0);
    expect(screen.getByText("device did not answer")).toBeTruthy();
    await advance(5100);
    expect(screen.queryByText("device did not answer")).toBeNull();
  });

  it("goes back to the room, or to Equipments when the room is unknown", () => {
    setup([boundLight()]);
    const back = screen.getByText("Garage", { selector: "a" });
    expect(back.getAttribute("href")).toBe("#/zone/garage");
    cleanup();
    setup([boundLight({ zoneId: "nowhere" })]);
    expect(screen.getByRole("link", { name: "Equipments" }).getAttribute("href")).toBe(
      "#/equipments",
    );
  });

  it("opens the form in edit mode with the name and the room", () => {
    setup([boundLight()]);
    fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    expect(screen.getByRole("dialog", { name: "Edit equipment" })).toBeTruthy();
    expect((screen.getByLabelText("Name") as HTMLInputElement).value).toBe("Garage light");
    expect((screen.getByLabelText("Zone") as HTMLSelectElement).value).toBe("garage");
    expect((screen.getByLabelText("Type") as HTMLSelectElement).disabled).toBe(true);
  });

  describe("not found", () => {
    it("says Equipment not found with the back link, once the store is loaded", () => {
      setup([boundLight()], {}, "other");
      expect(screen.getByText("Equipment not found")).toBeTruthy();
      expect(screen.getByRole("link", { name: "Equipments" }).getAttribute("href")).toBe(
        "#/equipments",
      );
    });

    it("says nothing before the first read", () => {
      mockApi({});
      useEquipments.setState({ equipments: [], loaded: false });
      const { container } = render(<EquipmentDetailPage equipmentId="l" />);
      expect(container.textContent).toBe("");
      expect(screen.queryByText("Equipment not found")).toBeNull();
    });

    it("says Equipment not found when the equipment leaves the store (equipment.removed)", () => {
      setup([boundLight()]);
      expect(screen.getByRole("heading", { level: 1, name: "Garage light" })).toBeTruthy();
      act(() => useEquipments.setState({ equipments: [] }));
      expect(screen.getByText("Equipment not found")).toBeTruthy();
    });

    it("shows the error and Retry, which reads the equipments again, when they cannot be read", async () => {
      const api = mockApi({ [`GET ${EQ}`]: { body: [boundLight()] } });
      useEquipments.setState({ equipments: [], loaded: false, error: "Cannot reach Solite" });
      render(<EquipmentDetailPage equipmentId="l" />);
      expect(screen.getByText("Cannot reach Solite")).toBeTruthy();
      fireEvent.click(screen.getByRole("button", { name: "Retry" }));
      await waitFor(() => expect(api.count(`GET ${EQ}`)).toBe(1));
      expect(await screen.findByRole("heading", { level: 1, name: "Garage light" })).toBeTruthy();
    });
  });

  describe("bindings", () => {
    it("removes a data binding with a DELETE and no confirmation", async () => {
      const api = setup([boundLight()], {
        [`DELETE ${EQ}/l/data-bindings/db-state`]: { status: 204 },
      });
      openRow(/Bindings/);
      const row = screen.getByTestId("data-binding");
      fireEvent.click(within(row).getByRole("button", { name: "Remove" }));
      await waitFor(() => expect(api.count(`DELETE ${EQ}/l/data-bindings/db-state`)).toBe(1));
      expect(window.confirm).not.toHaveBeenCalled();
      await waitFor(() => expect(api.count(`GET ${EQ}`)).toBe(1));
    });

    it("sends one DELETE for a double click on Remove", async () => {
      const api = setup([boundLight()], {
        [`DELETE ${EQ}/l/data-bindings/db-state`]: { status: 204 },
      });
      openRow(/Bindings/);
      const button = within(screen.getByTestId("data-binding")).getByRole("button", {
        name: "Remove",
      });
      fireEvent.click(button);
      fireEvent.click(button);
      await waitFor(() => expect(api.count(`DELETE ${EQ}/l/data-bindings/db-state`)).toBe(1));
      await new Promise((r) => setTimeout(r, 20));
      expect(api.count(`DELETE ${EQ}/l/data-bindings/db-state`)).toBe(1);
    });

    it("shows the refusal of a data binding removal", async () => {
      setup([boundLight()], {
        [`DELETE ${EQ}/l/data-bindings/db-state`]: { status: 400, body: { error: "Refused" } },
      });
      openRow(/Bindings/);
      fireEvent.click(
        within(screen.getByTestId("data-binding")).getByRole("button", { name: "Remove" }),
      );
      expect(await screen.findByText("Refused")).toBeTruthy();
    });

    it("sends the DELETE of an order binding, shows its refusal, and clears it after 5 s", async () => {
      vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
      const api = setup([boundLight()], {
        [`DELETE ${EQ}/l/order-bindings/ob-l`]: { status: 400, body: { error: "Order refused" } },
      });
      openRow(/Bindings/);
      await act(async () => {
        fireEvent.click(
          within(screen.getByTestId("order-binding")).getByRole("button", { name: "Remove" }),
        );
      });
      await advance(0);
      expect(api.count(`DELETE ${EQ}/l/order-bindings/ob-l`)).toBe(1);
      expect(screen.getByText("Order refused")).toBeTruthy();
      await advance(5100);
      expect(screen.queryByText("Order refused")).toBeNull();
    });

    it("lists the bindings with their alias, category, device, key and value", () => {
      setup([boundLight()]);
      openRow(/Bindings/);
      const data = screen.getByTestId("data-binding");
      expect(data.textContent).toContain("state");
      expect(data.textContent).toContain("(light_state)");
      expect(data.textContent).toContain("ZBMINIR2");
      expect(screen.getByTestId("order-binding").textContent).toContain("(enum)");
      cleanup();
      setup([equipment("e", "Empty", "sensor", "garage")]);
      openRow(/Bindings/);
      expect(screen.getAllByText("None")).toHaveLength(2);
    });

    it("shows the banner with the count once opened, not before", async () => {
      const api = setup([lightMissingState()]);
      expect(screen.queryByText(/new data point/)).toBeNull();
      expect(api.count("GET /api/v1/devices/zb")).toBe(0);
      openRow(/Bindings/);
      expect(await screen.findByText("1 new data point available")).toBeTruthy();
      expect(api.count("GET /api/v1/devices/zb")).toBe(1);
    });

    it("puts the missing binding back from the banner", async () => {
      const api = setup([lightMissingState()], {
        [`POST ${EQ}/l/data-bindings`]: { status: 201, body: {} },
      });
      openRow(/Bindings/);
      await screen.findByText("1 new data point available");
      fireEvent.click(screen.getByRole("button", { name: "Review" }));
      fireEvent.click(await screen.findByRole("button", { name: "Add 1" }));
      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
      const post = api.calls.find((c) => c.key === `POST ${EQ}/l/data-bindings`)!;
      expect(post.body).toEqual({ deviceDataId: "zb-state", alias: "state" });
    });

    it("shows no banner when the device cannot be read for the count", async () => {
      const api = setup([lightMissingState()], {
        "GET /api/v1/devices/zb": { status: 500, body: { error: "boom" } },
      });
      openRow(/Bindings/);
      await waitFor(() => expect(api.count("GET /api/v1/devices/zb")).toBe(1));
      await new Promise((r) => setTimeout(r, 20));
      expect(screen.queryByText(/new data point/)).toBeNull();
    });

    it("offers Add but never a banner for a door", async () => {
      const door = equipment("l", "Front door", "door", "garage", {
        orderBindings: lightMissingState().orderBindings,
      });
      const api = setup([door]);
      openRow(/Bindings/);
      await waitFor(() => expect(api.count("GET /api/v1/devices/zb")).toBe(1));
      await new Promise((r) => setTimeout(r, 20));
      expect(screen.queryByText(/new data point/)).toBeNull();
      expect(screen.getAllByRole("button", { name: "Add" })).toHaveLength(2);
    });

    it("adds a data binding from the dialog, the list changing without a reload", async () => {
      const api = setup([lightMissingState()], {
        [`POST ${EQ}/l/data-bindings`]: { status: 201, body: {} },
        [`GET ${EQ}`]: { body: [boundLight()] },
      });
      openRow(/Bindings/);
      fireEvent.click(screen.getAllByRole("button", { name: "Add" })[0]);
      fireEvent.click(await screen.findByRole("button", { name: /^ZBMINIR2/ }));
      fireEvent.click(await screen.findByRole("button", { name: /state/ }));
      fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Add" }));
      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
      expect(api.calls.find((c) => c.key === `POST ${EQ}/l/data-bindings`)!.body).toEqual({
        deviceDataId: "zb-state",
        alias: "state",
      });
      await waitFor(() => expect(screen.getByTestId("data-binding")).toBeTruthy());
    });
  });

  describe("delete", () => {
    it("deletes after the confirmation and goes to the list", async () => {
      const api = setup([boundLight()], { [`DELETE ${EQ}/l`]: { status: 204 } });
      fireEvent.click(screen.getByRole("button", { name: "Delete equipment" }));
      expect(window.confirm).toHaveBeenCalledWith(
        'Delete equipment "Garage light"? This cannot be undone.',
      );
      await waitFor(() => expect(window.location.hash).toBe("#/equipments"));
      expect(api.count(`DELETE ${EQ}/l`)).toBe(1);
    });

    it("sends nothing when cancelled, and shows a refusal", async () => {
      const api = setup([boundLight()], {
        [`DELETE ${EQ}/l`]: { status: 400, body: { error: "Cannot delete it" } },
      });
      (window.confirm as ReturnType<typeof vi.fn>).mockReturnValueOnce(false);
      fireEvent.click(screen.getByRole("button", { name: "Delete equipment" }));
      expect(api.count(`DELETE ${EQ}/l`)).toBe(0);
      fireEvent.click(screen.getByRole("button", { name: "Delete equipment" }));
      expect(await screen.findByText("Cannot delete it")).toBeTruthy();
      expect(window.location.hash).toBe("");
    });
  });
});

describe("DevicesSection", () => {
  const twoDevices = (): EquipmentWithDetails =>
    equipment("e", "Mixed", "sensor", "garage", {
      dataBindings: [
        dataBinding("temperature", "temperature", 1, {
          deviceId: "d1",
          deviceName: "Probe",
          equipmentId: "e",
        }),
        dataBinding("humidity", "humidity", 1, {
          deviceId: "d1",
          deviceName: "Probe",
          equipmentId: "e",
        }),
        dataBinding("occupancy", "motion", true, {
          deviceId: "d2",
          deviceName: "Motion",
          equipmentId: "e",
        }),
      ],
      orderBindings: [
        {
          id: "ob",
          equipmentId: "e",
          deviceOrderId: "o",
          alias: "state",
          deviceId: "d2",
          deviceName: "Motion",
          key: "state",
          type: "enum",
        },
      ],
    });

  it("collapsed shows the number of devices; opened, each with its link, last seen and counts", () => {
    useDevices.setState({
      devices: {
        d1: device("d1", "Probe", { lastSeen: new Date(Date.now() - 120_000).toISOString() }),
        d2: device("d2", "Motion", { lastSeen: null }),
      },
    });
    render(<DevicesSection equipment={twoDevices()} onChangeDevice={() => {}} />);
    expect(screen.getByRole("button", { name: /Devices/ }).textContent).toContain("2");
    expect(screen.queryByText("Probe")).toBeNull();
    openRow(/Devices/);
    expect(screen.getByText("Probe").closest("a")!.getAttribute("href")).toBe("#/devices/d1");
    expect(screen.getByText("Motion").closest("a")!.getAttribute("href")).toBe("#/devices/d2");
    expect(screen.getByText("2 data")).toBeTruthy();
    expect(screen.getByText("1 data · 1 order")).toBeTruthy();
    expect(screen.getByText(/ago/)).toBeTruthy();
    expect(screen.getByText("never")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Change devices" })).toBeTruthy();
  });

  it("says No device with an Add button when nothing is bound", () => {
    const onChange = vi.fn();
    render(
      <DevicesSection
        equipment={equipment("e", "Empty", "sensor", "garage")}
        onChangeDevice={onChange}
      />,
    );
    openRow(/Devices/);
    expect(screen.getByText("No device")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(onChange).toHaveBeenCalled();
  });
});

describe("ChangeDeviceModal", () => {
  const routes = (extra: Record<string, Route | Route[]> = {}) => ({
    "GET /api/v1/devices": { body: [fixtures.zbminir2(), fixtures.voltageOnly()] },
    [`GET ${EQ}`]: { body: [] },
    [`DELETE ${EQ}/l/data-bindings/db-state`]: { status: 204 },
    [`DELETE ${EQ}/l/order-bindings/ob-l`]: { status: 204 },
    [`POST ${EQ}/l/data-bindings`]: { status: 201, body: {} },
    [`POST ${EQ}/l/order-bindings`]: { status: 201, body: {} },
    ...extra,
  });
  const mount = (extra: Record<string, Route | Route[]> = {}, eq = boundLight()) => {
    const api = mockApi(routes(extra));
    const onDone = vi.fn();
    const onClose = vi.fn();
    render(<ChangeDeviceModal equipment={eq} onDone={onDone} onClose={onClose} />);
    return { api, onDone, onClose };
  };
  const save = () => screen.getByRole("button", { name: "Save" }) as HTMLButtonElement;
  const tick = async (label: string) => fireEvent.click(await screen.findByLabelText(label));
  const writes = (api: ReturnType<typeof mockApi>) =>
    api.calls.filter((c) => /^(POST|DELETE)/.test(c.key)).map((c) => c.key.replace(EQ + "/l/", ""));

  it("deletes every old binding, then writes the plan, then closes", async () => {
    const { api, onDone } = mount();
    expect(
      screen.getByText(
        "The current bindings are removed and replaced by those of the devices chosen.",
      ),
    ).toBeTruthy();
    await tick("ZBMINIR2");
    fireEvent.click(save());
    await waitFor(() => expect(onDone).toHaveBeenCalled());
    expect(writes(api)).toEqual([
      "DELETE data-bindings/db-state",
      "DELETE order-bindings/ob-l",
      "POST data-bindings",
      "POST order-bindings",
    ]);
  });

  it("disables Save with nothing ticked", async () => {
    mount();
    await screen.findByLabelText("ZBMINIR2");
    expect(save().disabled).toBe(true);
  });

  it("disables Save when the devices ticked give nothing the type uses", async () => {
    mount();
    await screen.findByLabelText("ZBMINIR2");
    fireEvent.click(screen.getByRole("button", { name: "Show all devices" }));
    await tick("Voltage only");
    expect(save().disabled).toBe(true);
  });

  it("disables Save for a type without binding rules, and says it is completed binding by binding", async () => {
    mount({}, equipment("l", "Front door", "door", "garage"));
    await screen.findByLabelText("ZBMINIR2");
    expect(screen.getByText("This type is completed binding by binding")).toBeTruthy();
    await tick("ZBMINIR2");
    expect(save().disabled).toBe(true);
  });

  it("stops at a refused POST: the text shown, the dialog open, no further request", async () => {
    const { api, onDone, onClose } = mount({
      [`POST ${EQ}/l/data-bindings`]: { status: 400, body: { error: "Too many data bindings" } },
    });
    await tick("ZBMINIR2");
    fireEvent.click(save());
    expect(await screen.findByText("Too many data bindings")).toBeTruthy();
    expect(writes(api)).toEqual([
      "DELETE data-bindings/db-state",
      "DELETE order-bindings/ob-l",
      "POST data-bindings",
    ]);
    expect(onDone).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("stops at a refused DELETE: the text shown, no POST", async () => {
    const { api, onDone } = mount({
      [`DELETE ${EQ}/l/data-bindings/db-state`]: { status: 400, body: { error: "Cannot remove" } },
    });
    await tick("ZBMINIR2");
    fireEvent.click(save());
    expect(await screen.findByText("Cannot remove")).toBeTruthy();
    expect(writes(api)).toEqual(["DELETE data-bindings/db-state"]);
    expect(onDone).not.toHaveBeenCalled();
  });
});
