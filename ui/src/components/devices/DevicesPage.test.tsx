import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../../App";
import {
  FakeWebSocket,
  advance,
  device,
  homeRoutes,
  installFakeWebSocket,
  mockApi,
  resetStores,
  zone,
  type Route,
} from "../../test-utils";
import type { Device } from "../../types";

const PERMIT = "/api/v1/integrations/solite/permit-join";
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
  zones = tree(),
) {
  const api = mockApi({ ...homeRoutes({ zones, devices }), ...extra });
  render(<App />);
  act(() => FakeWebSocket.last.open());
  await screen.findByRole("heading", { level: 1, name: "Devices" });
  await waitFor(() => expect(api.count("GET /api/v1/devices")).toBe(1));
  return api;
}

const select = (name: string) =>
  within(screen.getByText(name).closest("li")!).getByRole("combobox") as HTMLSelectElement;

describe("devices page", () => {
  it("says there is no device and what to do", async () => {
    await boot();
    expect(screen.getByText("No devices discovered")).toBeTruthy();
    expect(
      screen.getByText("Press Add a device, then put the device in pairing mode."),
    ).toBeTruthy();
  });

  it("lists the devices newest first with maker, model and status", async () => {
    await boot([device("a", "Old one"), device("b", "Newer one", { status: "offline" })]);
    const rows = screen.getAllByRole("listitem");
    expect(rows[0].textContent).toContain("Newer one");
    expect(rows[0].textContent).toContain("SONOFF ZBMINIR2");
    expect(rows[0].textContent).toContain("Offline");
    expect(rows[1].textContent).toContain("Old one");
    expect(screen.getByText("2 devices · 1 online")).toBeTruthy();
  });

  it("Add a device posts 180, shows the countdown and Stop", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "setTimeout", "clearTimeout"] });
    const api = await boot([], {
      [`POST ${PERMIT}`]: { status: 202, body: { permitJoin: true, remaining: 180 } },
      [`GET ${PERMIT}`]: [
        { body: { permitJoin: false, remaining: 0 } },
        { body: { permitJoin: true, remaining: 179 } },
      ],
    });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Add a device" }));
    });
    expect(api.calls.find((c) => c.key === `POST ${PERMIT}`)!.body).toEqual({ time: 180 });
    await advance(0);
    expect(screen.getByText(/Pairing open: put the device in pairing mode/)).toBeTruthy();
    expect(screen.getByText("180 s")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Stop" })).toBeTruthy();
    await advance(1000);
    expect(screen.getByText("179 s")).toBeTruthy();
  });

  it("shows the radio's refusal", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "setTimeout", "clearTimeout"] });
    await boot([], {
      [`POST ${PERMIT}`]: { status: 202, body: { permitJoin: true, remaining: 180 } },
      [`GET ${PERMIT}`]: { body: { permitJoin: false, remaining: 0, error: "the radio refused" } },
    });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Add a device" }));
    });
    await advance(1000);
    expect(screen.getByText("the radio refused")).toBeTruthy();
  });

  it("a discovered device appears first, marked New; a status change adds no mark", async () => {
    await boot([device("a", "Known")]);
    act(() =>
      FakeWebSocket.last.receive({ type: "device.discovered", device: device("b", "Fresh") }),
    );
    const rows = screen.getAllByRole("listitem");
    expect(rows[0].textContent).toContain("Fresh");
    expect(within(rows[0]).getByText("New")).toBeTruthy();
    act(() =>
      FakeWebSocket.last.receive({
        type: "device.status_changed",
        deviceId: "a",
        deviceName: "Known",
        status: "offline",
      }),
    );
    expect(within(screen.getAllByRole("listitem")[1]).queryByText("New")).toBeNull();
    expect(screen.getAllByRole("listitem")[1].textContent).toContain("Offline");
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

  it("offers No room, the zones but the root indented, and New room", async () => {
    await boot([device("a", "Relay")]);
    const options = Array.from(select("Relay").options).map((o) => o.textContent);
    expect(options).toEqual(["No room", "Garage", "  Bench", "New room…"]);
  });

  it("offers only No room and New room when there is only the root zone", async () => {
    await boot([device("a", "Relay")], {}, [zone("root", "Home")]);
    expect(Array.from(select("Relay").options).map((o) => o.textContent)).toEqual([
      "No room",
      "New room…",
    ]);
  });

  it("places the device in a room, and back to No room", async () => {
    const api = await boot([device("a", "Relay")], { "PUT /api/v1/devices/a": { body: {} } });
    fireEvent.change(select("Relay"), { target: { value: "garage" } });
    await waitFor(() => expect(select("Relay").value).toBe("garage"));
    fireEvent.change(select("Relay"), { target: { value: "" } });
    await waitFor(() => expect(select("Relay").value).toBe(""));
    expect(api.calls.filter((c) => c.key === "PUT /api/v1/devices/a").map((c) => c.body)).toEqual([
      { zoneId: "garage" },
      { zoneId: null },
    ]);
  });

  it("shows the refusal of a room change for 5 s and the old zone", async () => {
    await boot([device("a", "Relay", { zoneId: "garage" })], {
      "PUT /api/v1/devices/a": { status: 404, body: { error: "Zone not found" } },
    });
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    await act(async () => {
      fireEvent.change(select("Relay"), { target: { value: "bench" } });
    });
    await advance(0);
    expect(screen.getByText("This item does not exist or was deleted.")).toBeTruthy();
    expect(select("Relay").value).toBe("garage");
    await advance(5000);
    expect(screen.queryByText("This item does not exist or was deleted.")).toBeNull();
  });

  it("New room: makes the zone then places the device; nothing is sent with an empty name", async () => {
    const api = await boot([device("a", "Relay")], {
      "POST /api/v1/zones": { status: 201, body: zone("new", "Cellar", [], "root") },
      "PUT /api/v1/devices/a": { body: {} },
    });
    fireEvent.change(select("Relay"), { target: { value: "__new__" } });
    const name = screen.getByLabelText("Name of the new room");
    fireEvent.keyDown(name, { key: "Enter" });
    expect(api.count("POST /api/v1/zones")).toBe(0);
    fireEvent.input(name, { target: { value: "Cellar" } });
    fireEvent.keyDown(name, { key: "Enter" });
    await waitFor(() => expect(api.count("PUT /api/v1/devices/a")).toBe(1));
    expect(api.calls.find((c) => c.key === "POST /api/v1/zones")!.body).toEqual({ name: "Cellar" });
    expect(api.calls.find((c) => c.key === "PUT /api/v1/devices/a")!.body).toEqual({
      zoneId: "new",
    });
  });

  it("New room refused by the zone limit: its text, no PUT", async () => {
    const api = await boot([device("a", "Relay")], {
      "POST /api/v1/zones": { status: 409, body: { error: "Zone limit reached (32)" } },
    });
    fireEvent.change(select("Relay"), { target: { value: "__new__" } });
    fireEvent.input(screen.getByLabelText("Name of the new room"), { target: { value: "Cellar" } });
    fireEvent.keyDown(screen.getByLabelText("Name of the new room"), { key: "Enter" });
    expect(await screen.findByText("Zone limit reached (32)")).toBeTruthy();
    expect(api.count("PUT /api/v1/devices/a")).toBe(0);
  });

  it("New room made, PUT refused: the new zone is deleted and the PUT's text shown", async () => {
    const api = await boot([device("a", "Relay")], {
      "POST /api/v1/zones": { status: 201, body: zone("new", "Cellar", [], "root") },
      "PUT /api/v1/devices/a": { status: 400, body: { error: "Invalid field: zoneId" } },
      "DELETE /api/v1/zones/new": { status: 204 },
    });
    fireEvent.change(select("Relay"), { target: { value: "__new__" } });
    fireEvent.input(screen.getByLabelText("Name of the new room"), { target: { value: "Cellar" } });
    fireEvent.keyDown(screen.getByLabelText("Name of the new room"), { key: "Enter" });
    expect(await screen.findByText("Invalid field: zoneId")).toBeTruthy();
    expect(api.count("DELETE /api/v1/zones/new")).toBe(1);
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

  it("keeps the row and shows the text when the device is being paired (409)", async () => {
    await boot([device("a", "Relay")], {
      "DELETE /api/v1/devices/a": { status: 409, body: { error: "Device is being paired" } },
    });
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(await screen.findByText("Device is being paired")).toBeTruthy();
    expect(screen.getByText("Relay")).toBeTruthy();
  });

  it("removes the row when the device is already gone (404)", async () => {
    await boot([device("a", "Relay")], {
      "DELETE /api/v1/devices/a": { status: 404, body: { error: "Device not found" } },
    });
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(screen.queryByText("Relay")).toBeNull());
  });

  it("goes on counting while the connection is lost, and reads the state and devices on reconnection", async () => {
    vi.useFakeTimers({
      toFake: ["setInterval", "clearInterval", "setTimeout", "clearTimeout"],
    });
    vi.spyOn(Math, "random").mockReturnValue(0);
    const api = await boot([], {
      [`POST ${PERMIT}`]: { status: 202, body: { permitJoin: true, remaining: 180 } },
      [`GET ${PERMIT}`]: [
        { body: { permitJoin: false, remaining: 0 } },
        { body: { permitJoin: true, remaining: 170 } },
        { body: { permitJoin: true, remaining: 170 } },
      ],
    });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Add a device" }));
    });
    act(() => FakeWebSocket.last.drop());
    await advance(500);
    expect(screen.getByText("180 s")).toBeTruthy();
    expect(api.count("GET /api/v1/devices")).toBe(1);
    await advance(1500);
    const before = api.count("GET /api/v1/devices");
    act(() => FakeWebSocket.last.open());
    await advance(0);
    expect(api.count("GET /api/v1/devices")).toBe(before + 1);
    expect(screen.getByText("170 s")).toBeTruthy();
  });
});
