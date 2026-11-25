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
    expect(screen.queryByRole("button", { name: "Create equipment" })).toBeNull();
    expect(screen.queryAllByRole("combobox")).toHaveLength(0);
  });

  it("shows maker, model and the status of each device, and the subtitle", async () => {
    await boot([device("a", "Old one"), device("b", "Newer one", { status: "offline" })]);
    expect(within(row("Newer one")).getByRole("img", { name: "Offline" })).toBeTruthy();
    expect(row("Newer one").textContent).toContain("SONOFF");
    expect(row("Newer one").textContent).toContain("ZBMINIR2");
    expect(screen.getByText("2 devices · 1 online")).toBeTruthy();
  });

  it("changes the column when the equipment is made", async () => {
    await boot([device("relay", "Relay")], {
      "GET /api/v1/equipments": [{ body: [] }, { body: [lamp()] }],
    });
    expect(within(row("Relay")).getAllByRole("cell")[4].textContent).toBe("—");
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    act(() => FakeWebSocket.last.receive({ type: "equipment.created" }));
    await advance(0);
    expect(within(row("Relay")).getAllByRole("cell")[4].textContent).toBe("Garage›Lumière");
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

  it("opens the device's page when a row is clicked (spec 030, R2)", async () => {
    await boot([device("relay", "Relay")], {}, [lamp()]);
    fireEvent.click(within(row("Relay")).getAllByRole("cell")[2]);
    expect(window.location.hash).toBe("#/devices/relay");
  });

  it("opens the device's page with Enter or Space on a focused row, not from an inner button", async () => {
    await boot([device("relay", "Relay")], {}, [lamp()]);
    const tr = row("Relay");
    expect(tr.getAttribute("tabindex")).toBe("0");
    fireEvent.keyDown(tr, { key: "Enter" });
    expect(window.location.hash).toBe("#/devices/relay");
    window.location.hash = "#/devices";
    fireEvent.keyDown(tr, { key: " " });
    expect(window.location.hash).toBe("#/devices/relay");
    window.location.hash = "#/devices";
    fireEvent.keyDown(within(tr).getAllByRole("button")[0], { key: "Enter" });
    expect(window.location.hash).toBe("#/devices");
  });

  it("does not open the page on the name editor or Delete", async () => {
    await boot([device("a", "Relay")], { "DELETE /api/v1/devices/a": { status: 204 } });
    fireEvent.click(screen.getByText("Relay"));
    expect(window.location.hash).toBe("#/devices");
    fireEvent.click(screen.getByTitle("Edit name"));
    fireEvent.click(screen.getByRole("textbox"));
    expect(window.location.hash).toBe("#/devices");
    fireEvent.keyDown(screen.getByRole("textbox"), { key: "Escape" });
    (window.confirm as ReturnType<typeof vi.fn>).mockReturnValueOnce(false);
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(window.location.hash).toBe("#/devices");
  });

  it("keeps the equipment link and the room link's own address, not the device's", async () => {
    await boot([device("relay", "Relay")], {}, [lamp()]);
    const cell = within(row("Relay")).getAllByRole("cell")[4];
    fireEvent.click(within(cell).getByRole("link", { name: "Lumière" }));
    expect(window.location.hash).toBe("#/equipments/l");
    window.location.hash = "#/devices";
    const room = within(cell).getByRole("link", { name: "Garage" });
    fireEvent.click(room);
    expect(room.getAttribute("href")).toBe("#/zone/garage");
    expect(window.location.hash).not.toBe("#/devices/relay");
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
