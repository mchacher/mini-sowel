import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useEquipments } from "../../store/useEquipments";
import { useZones } from "../../store/useZones";
import { fixtures, light, mockApi, resetStores, zone, type Route } from "../../test-utils";
import { NewEquipmentForm } from "./NewEquipmentForm";

beforeEach(() => {
  window.location.hash = "";
  useZones.setState({
    tree: [zone("root", "Home", [zone("garage", "Garage", [], "root")])],
    loading: false,
    error: null,
  });
});

afterEach(() => {
  cleanup();
  resetStores();
  vi.unstubAllGlobals();
});

const EQ = "/api/v1/equipments";

function setup(devices: unknown[], extra: Record<string, Route | Route[]> = {}) {
  const api = mockApi({
    "GET /api/v1/devices": { body: devices },
    [`GET ${EQ}`]: { body: [] },
    [`POST ${EQ}`]: { status: 201, body: light("new", "Made", "garage", false) },
    [`POST ${EQ}/new/data-bindings`]: { status: 201, body: {} },
    [`POST ${EQ}/new/order-bindings`]: { status: 201, body: {} },
    [`DELETE ${EQ}/new`]: { status: 204 },
    ...extra,
  });
  const onClose = vi.fn();
  render(<NewEquipmentForm defaultZoneId="garage" onClose={onClose} />);
  return { api, onClose };
}

const write = (api: ReturnType<typeof mockApi>) =>
  api.calls.filter((c) => c.key.startsWith("POST") || c.key.startsWith("DELETE"));

async function fill(name: string, type: string, tick: string[]) {
  fireEvent.input(screen.getByLabelText("Name"), { target: { value: name } });
  fireEvent.change(screen.getByLabelText("Type"), { target: { value: type } });
  fireEvent.click(screen.getByRole("button", { name: "Next: Select devices" }));
  await waitFor(() => expect(screen.queryByText("Loading devices...")).toBeNull());
  for (const label of tick) fireEvent.click(screen.getByLabelText(label));
}

describe("NewEquipmentForm", () => {
  it("creates a light from the relay: the equipment without deviceIds, then state and state", async () => {
    const { api, onClose } = setup([fixtures.zbminir2()]);
    await fill("Lumière garage", "light_onoff", ["ZBMINIR2"]);
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(write(api)).toEqual([
      {
        key: `POST ${EQ}`,
        body: { name: "Lumière garage", type: "light_onoff", zoneId: "garage" },
      },
      {
        key: `POST ${EQ}/new/data-bindings`,
        body: { deviceDataId: "zb-state", alias: "state" },
      },
      {
        key: `POST ${EQ}/new/order-bindings`,
        body: { deviceOrderId: "zb-o-state", alias: "state" },
      },
    ]);
    // The list is read again once the equipment is made.
    expect(api.count(`GET ${EQ}`)).toBe(1);
  });

  it("creates a sensor from the TH01 and the MS01: five data bindings, battery_2 last, no order", async () => {
    const { api, onClose } = setup([fixtures.th01(), fixtures.ms01()]);
    await fill("Climat", "sensor", ["TH01", "MS01"]);
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    const posts = write(api).filter((c) => c.key.endsWith("data-bindings"));
    expect(posts.map((c) => (c.body as { alias: string }).alias)).toEqual([
      "temperature",
      "humidity",
      "battery",
      "occupancy",
      "battery_2",
    ]);
    expect(api.count(`POST ${EQ}/new/order-bindings`)).toBe(0);
  });

  it("deletes the equipment and keeps the form open with the text when a binding is refused", async () => {
    const { api, onClose } = setup([fixtures.zbminir2()], {
      [`POST ${EQ}/new/data-bindings`]: {
        status: 404,
        body: { error: "DeviceData not found: zb-state" },
      },
    });
    await fill("Lumière", "light_onoff", ["ZBMINIR2"]);
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    expect(await screen.findByText("DeviceData not found: zb-state")).toBeTruthy();
    expect(api.count(`DELETE ${EQ}/new`)).toBe(1);
    expect(api.count(`POST ${EQ}/new/order-bindings`)).toBe(0);
    expect(onClose).not.toHaveBeenCalled();
  });

  it("shows the binding's text and keeps the equipment when the delete is refused too", async () => {
    const half = light("new", "Made", "garage", false);
    const { api } = setup([fixtures.zbminir2()], {
      [`POST ${EQ}/new/data-bindings`]: {
        status: 409,
        body: { error: "Alias already used: state" },
      },
      [`DELETE ${EQ}/new`]: { status: 500, body: { error: "disk error" } },
      [`GET ${EQ}`]: { body: [half] },
    });
    await fill("Lumière", "light_onoff", ["ZBMINIR2"]);
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    expect(await screen.findByText("Alias already used: state")).toBeTruthy();
    await waitFor(() =>
      expect(useEquipments.getState().equipments.map((e) => e.id)).toEqual(["new"]),
    );
    expect(api.count(`DELETE ${EQ}/new`)).toBe(1);
  });

  it("stays open with the API's text when the equipment is refused, no binding request", async () => {
    const { api, onClose } = setup([fixtures.zbminir2()], {
      [`POST ${EQ}`]: { status: 400, body: { error: "Too many equipments (64)" } },
    });
    await fill("Lumière", "light_onoff", ["ZBMINIR2"]);
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    expect(await screen.findByText("Too many equipments (64)")).toBeTruthy();
    expect(write(api)).toHaveLength(1);
    expect(onClose).not.toHaveBeenCalled();
  });

  it("sends one equipment when Create is clicked twice", async () => {
    const { api, onClose } = setup([fixtures.zbminir2()]);
    await fill("Lumière", "light_onoff", ["ZBMINIR2"]);
    const create = screen.getByRole("button", { name: "Create" });
    await act(async () => {
      fireEvent.click(create);
      fireEvent.click(create);
    });
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(api.count(`POST ${EQ}`)).toBe(1);
  });

  it("creates the equipment alone with no device, and opens its page", async () => {
    const { api, onClose } = setup([fixtures.zbminir2()]);
    await fill("Test", "sensor", []);
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(write(api)).toEqual([
      { key: `POST ${EQ}`, body: { name: "Test", type: "sensor", zoneId: "garage" } },
    ]);
    expect(window.location.hash).toBe("#/equipments/new");
  });

  it("does not open the page when devices were ticked", async () => {
    window.location.hash = "";
    const { onClose } = setup([fixtures.zbminir2()]);
    await fill("Lumière", "light_onoff", ["ZBMINIR2"]);
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(window.location.hash).toBe("");
  });
});
