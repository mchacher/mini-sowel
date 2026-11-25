import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../../App";
import {
  FakeWebSocket,
  openSocket,
  storeSession,
  advance,
  dataBinding,
  device,
  fixtures,
  homeRoutes,
  installFakeWebSocket,
  light,
  mockApi,
  resetStores,
  zone,
  type Route,
} from "../../test-utils";
import type { Device, EquipmentWithDetails } from "../../types";

const tree = () => [
  zone("root", "Home", [zone("garage", "Garage", [zone("bench", "Bench", [], "garage")], "root")]),
];

beforeEach(() => {
  installFakeWebSocket();
  window.location.hash = "#/devices";
  vi.spyOn(window, "confirm").mockReturnValue(true);
});

afterEach(async () => {
  cleanup();
  resetStores();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  // Preact's effects wait for a frame (a 100 ms timer): let it fire before the test environment goes.
  await new Promise((resolve) => setTimeout(resolve, 120));
});

async function boot(
  devices: Device[] = [],
  extra: Record<string, Route | Route[]> = {},
  equipments: EquipmentWithDetails[] = [],
) {
  const api = mockApi({ ...homeRoutes({ zones: tree(), devices, equipments }), ...extra });
  storeSession();
  render(<App />);
  await openSocket();
  await screen.findByRole("heading", { level: 1, name: "Devices" });
  await waitFor(() => expect(api.count("GET /api/v1/devices")).toBe(1));
  return api;
}

const row = (name: string) => screen.getByText(name).closest("tr")!;

/** A light in the garage that binds the device `relay`. */
function lamp(): EquipmentWithDetails {
  const eq = light("l", "Lumière", "garage", false);
  eq.dataBindings = [
    dataBinding("state", "light_state", false, { equipmentId: "l", deviceId: "relay" }),
  ];
  return eq;
}

describe("devices page", () => {
  it("lists the devices sorted by name, with the room and equipment of the one that is bound", async () => {
    await boot([device("relay", "Relay"), device("b", "Alpha"), device("c", "Zeta")], {}, [lamp()]);
    const names = screen
      .getAllByRole("row")
      .slice(1)
      .map((r) => within(r).getAllByRole("cell")[1].textContent);
    expect(names).toEqual(["Alpha", "Relay", "Zeta"]);
    const cell = within(row("Relay")).getAllByRole("cell")[4];
    expect(cell.textContent).toBe("Garage›Lumière");
    expect(within(cell).getByRole("link", { name: "Garage" }).getAttribute("href")).toBe(
      "#/zone/garage",
    );
    expect(within(row("Alpha")).getAllByRole("cell")[4].textContent).toBe("—");
    expect(within(row("Alpha")).getByRole("button", { name: "Create equipment" })).toBeTruthy();
    expect(within(row("Relay")).queryByRole("button", { name: "Create equipment" })).toBeNull();
    expect(screen.queryAllByRole("combobox")).toHaveLength(0);
  });

  it("shows maker, model and the status of each device, and the subtitle", async () => {
    await boot([device("a", "Old one"), device("b", "Newer one", { status: "offline" })]);
    expect(within(row("Newer one")).getByRole("img", { name: "Offline" })).toBeTruthy();
    expect(row("Newer one").textContent).toContain("SONOFF");
    expect(row("Newer one").textContent).toContain("ZBMINIR2");
    expect(screen.getByText("2 devices · 1 online")).toBeTruthy();
  });

  it("changes the column and takes the button away when the equipment is made", async () => {
    await boot([device("relay", "Relay")], {
      "GET /api/v1/equipments": [{ body: [] }, { body: [lamp()] }],
    });
    expect(within(row("Relay")).getByRole("button", { name: "Create equipment" })).toBeTruthy();
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    act(() => FakeWebSocket.last.receive({ type: "equipment.created" }));
    await advance(0);
    expect(within(row("Relay")).getAllByRole("cell")[4].textContent).toBe("Garage›Lumière");
    expect(within(row("Relay")).queryByRole("button", { name: "Create equipment" })).toBeNull();
  });

  it("says there is no device and where to pair one", async () => {
    await boot();
    expect(screen.getByText("No devices discovered")).toBeTruthy();
    const link = screen.getByRole("link", { name: "Pair one in Discovery." });
    expect(link.getAttribute("href")).toBe("#/discovery/solite");
  });

  it("shows the error state when the devices cannot be read, and Retry reads again", async () => {
    const api = await boot([], {
      "GET /api/v1/devices": [
        { status: 400, body: { error: "disk error" } },
        { body: [device("a", "Relay")] },
      ],
    });
    expect(await screen.findByText("Connection error")).toBeTruthy();
    expect(screen.getByText("disk error")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await screen.findByText("Relay");
    expect(api.count("GET /api/v1/devices")).toBe(2);
  });

  it("Add devices opens Discovery", async () => {
    await boot([device("a", "Relay")]);
    fireEvent.click(screen.getByRole("button", { name: "Add devices" }));
    expect(window.location.hash).toBe("#/discovery/solite");
  });

  it("shows integration tabs only when the devices come from more than one integration", async () => {
    await boot([device("a", "Relay"), device("b", "Other", { integrationId: "other" })]);
    expect(screen.getByRole("button", { name: /^All/ })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /^Zigbee/ }));
    expect(screen.queryByText("Other")).toBeNull();
    expect(screen.getByText("Relay")).toBeTruthy();
  });

  it("shows no tabs for one integration", async () => {
    await boot([device("a", "Relay")]);
    expect(screen.queryByRole("button", { name: /^All/ })).toBeNull();
  });

  it("renames; a refusal shows its text", async () => {
    const api = await boot([device("a", "Relay")], {
      "PUT /api/v1/devices/a": [
        { body: {} },
        { status: 400, body: { error: "Invalid field: name" } },
      ],
    });
    fireEvent.click(screen.getByTitle("Edit name"));
    const input = screen.getByRole("textbox");
    fireEvent.input(input, { target: { value: "Garage light" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await screen.findByText("Garage light");
    expect(api.calls.find((c) => c.key === "PUT /api/v1/devices/a")!.body).toEqual({
      name: "Garage light",
    });
    fireEvent.click(screen.getByTitle("Edit name"));
    fireEvent.input(screen.getByRole("textbox"), { target: { value: "x".repeat(65) } });
    fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter" });
    expect(await screen.findByText("Invalid field: name")).toBeTruthy();
  });

  it("deletes after confirmation, and sends nothing when cancelled", async () => {
    const api = await boot([device("a", "Relay")], {
      "DELETE /api/v1/devices/a": { status: 204 },
    });
    (window.confirm as ReturnType<typeof vi.fn>).mockReturnValueOnce(false);
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(api.count("DELETE /api/v1/devices/a")).toBe(0);
    expect(screen.getByText("Relay")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(screen.queryByText("Relay")).toBeNull());
    expect(api.count("DELETE /api/v1/devices/a")).toBe(1);
  });

  it("keeps the row and shows the text for 5 s when the device is being paired (409)", async () => {
    await boot([device("a", "Relay")], {
      "DELETE /api/v1/devices/a": { status: 409, body: { error: "Device is being paired" } },
    });
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    });
    await advance(0);
    expect(screen.getByText("Device is being paired")).toBeTruthy();
    expect(screen.getByText("Relay")).toBeTruthy();
    await advance(5000);
    expect(screen.queryByText("Device is being paired")).toBeNull();
  });

  it("removes the row when the device is already gone (404)", async () => {
    await boot([device("a", "Relay")], {
      "DELETE /api/v1/devices/a": { status: 404, body: { error: "Device not found" } },
    });
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(screen.queryByText("Relay")).toBeNull());
  });

  it("never sends a zoneId for a device", async () => {
    const api = await boot([device("a", "Relay")], {
      "PUT /api/v1/devices/a": { body: {} },
      "DELETE /api/v1/devices/a": { status: 204 },
    });
    fireEvent.click(screen.getByTitle("Edit name"));
    fireEvent.input(screen.getByRole("textbox"), { target: { value: "Other" } });
    fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter" });
    await screen.findByText("Other");
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(screen.queryByText("Other")).toBeNull());
    for (const call of api.calls) {
      expect(JSON.stringify(call.body ?? {})).not.toContain("zoneId");
    }
  });
});

describe("Create equipment, from a row", () => {
  const nextButton = () => screen.getByRole("button", { name: "Next: Select devices" });

  it("opens the form with the device's type and name, the room to choose, the device ticked", async () => {
    const relay = fixtures.zbminir2("relay");
    await boot([device("relay", "ZBMINIR2 567A")], {
      "GET /api/v1/devices/relay": { body: { ...relay, name: "ZBMINIR2 567A" } },
      "GET /api/v1/devices": { body: [{ ...relay, name: "ZBMINIR2 567A" }] },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create equipment" }));
    const form = await screen.findByRole("dialog");
    expect((within(form).getByLabelText("Type") as HTMLSelectElement).value).toBe("light_onoff");
    expect((within(form).getByLabelText("Name") as HTMLInputElement).value).toBe("ZBMINIR2 567A");
    expect((within(form).getByLabelText("Zone") as HTMLSelectElement).value).toBe("");
    expect((nextButton() as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(within(form).getByLabelText("Zone"), { target: { value: "garage" } });
    fireEvent.click(nextButton());
    const box = await within(form).findByLabelText("ZBMINIR2 567A");
    expect((box as HTMLInputElement).checked).toBe(true);
    expect(
      (within(form).getByRole("button", { name: "Create" }) as HTMLButtonElement).disabled,
    ).toBe(false);
  });

  it("shows the server's text under the row and removes it when the device is gone (404)", async () => {
    await boot([device("a", "Relay")], {
      "GET /api/v1/devices/a": { status: 404, body: { error: "Device not found: a" } },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create equipment" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(screen.queryByText("Relay")).toBeNull());
  });

  it("opens the form with light_onoff for a device with only a voltage, Create disabled", async () => {
    const volt = fixtures.voltageOnly("a");
    await boot([device("a", "Voltage only")], {
      "GET /api/v1/devices/a": { body: volt },
      "GET /api/v1/devices": { body: [volt] },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create equipment" }));
    const form = await screen.findByRole("dialog");
    expect((within(form).getByLabelText("Type") as HTMLSelectElement).value).toBe("light_onoff");
    fireEvent.change(within(form).getByLabelText("Zone"), { target: { value: "garage" } });
    fireEvent.click(nextButton());
    const box = await within(form).findByLabelText("Voltage only");
    expect((box as HTMLInputElement).checked).toBe(true);
    expect(
      (within(form).getByRole("button", { name: "Create" }) as HTMLButtonElement).disabled,
    ).toBe(true);
    expect(within(form).getByText(/give nothing this type uses yet/)).toBeTruthy();
  });

  it("reads the device once and opens one form when the button is clicked twice", async () => {
    const relay = fixtures.zbminir2("relay");
    const api = await boot([device("relay", "Relay")], {
      "GET /api/v1/devices/relay": { body: relay },
    });
    const button = screen.getByRole("button", { name: "Create equipment" });
    fireEvent.click(button);
    fireEvent.click(button);
    await screen.findByRole("dialog");
    expect(api.count("GET /api/v1/devices/relay")).toBe(1);
    expect(screen.getAllByRole("dialog")).toHaveLength(1);
  });
});
